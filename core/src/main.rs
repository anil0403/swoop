use std::error::Error;
use std::ffi::OsString;
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use uft::DEFAULT_PORT;
use uft::progress::Snapshot;
use uft::recv::{self, RecvEvent, RecvOptions};
use uft::send::{self, SendOptions, SendStats, Source, Streams};
use uft::tls::Identity;

const USAGE: &str = "usage:
  uft id                                   print this device's fingerprint
  uft recv [--dir D] [--port P] [--null]   listen for transfers
  uft send HOST[:PORT] FINGERPRINT PATH... [--streams auto|N]
  uft send HOST[:PORT] FINGERPRINT --synthetic SIZE [--streams auto|N]
  uft bench [--size SIZE] [--streams auto|N]
SIZE: bytes or K/M/G/T suffix (1024-based)";

#[derive(Debug, PartialEq)]
enum Cmd {
    Help,
    Id,
    Recv {
        dir: PathBuf,
        port: u16,
        null: bool,
    },
    Send {
        host: String,
        fp: String,
        paths: Vec<PathBuf>,
        synthetic: Option<u64>,
        streams: Streams,
    },
    Bench {
        size: u64,
        streams: Streams,
    },
}

type Res<T> = Result<T, Box<dyn Error>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("uft: {e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(cmd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("uft: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_size(s: &str) -> Result<u64, String> {
    let bad = || format!("bad size {s:?}");
    let (num, shift) = match s.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&s[..s.len() - 1], 10),
        Some('M') => (&s[..s.len() - 1], 20),
        Some('G') => (&s[..s.len() - 1], 30),
        Some('T') => (&s[..s.len() - 1], 40),
        _ => (s, 0),
    };
    let n: u64 = num.parse().map_err(|_| bad())?;
    n.checked_mul(1 << shift).ok_or_else(bad)
}

fn parse_streams(s: &str) -> Result<Streams, String> {
    match s {
        "auto" => Ok(Streams::Auto),
        _ => match s.parse() {
            Ok(n) if n > 0 => Ok(Streams::Fixed(n)),
            _ => Err(format!("bad --streams {s:?} (auto or N > 0)")),
        },
    }
}

fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let Some((cmd, rest)) = args.split_first() else {
        return Ok(Cmd::Help);
    };
    let mut pos = Vec::new();
    let mut flags: Vec<(&str, Option<&str>)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => return Ok(Cmd::Help),
            "--null" => flags.push(("--null", None)),
            f @ ("--dir" | "--port" | "--streams" | "--synthetic" | "--size") => {
                let v = it.next().ok_or(format!("{f} needs a value"))?;
                flags.push((f, Some(v)));
            }
            f if f.starts_with("--") => return Err(format!("unknown flag {f}")),
            p => pos.push(p),
        }
    }
    let allowed: &[&str] = match cmd.as_str() {
        "recv" => &["--dir", "--port", "--null"],
        "send" => &["--streams", "--synthetic"],
        "bench" => &["--size", "--streams"],
        _ => &[],
    };
    if let Some((f, _)) = flags.iter().find(|(f, _)| !allowed.contains(f)) {
        return Err(format!("{f} not valid for {cmd}"));
    }
    let get = |name: &str| {
        flags
            .iter()
            .rev()
            .find(|(f, _)| *f == name)
            .map(|(_, v)| v.unwrap_or(""))
    };
    let streams = get("--streams")
        .map(parse_streams)
        .transpose()?
        .unwrap_or(Streams::Auto);
    let no_pos = |c: Cmd| {
        if pos.is_empty() {
            Ok(c)
        } else {
            Err(format!("unexpected argument {:?}", pos[0]))
        }
    };
    match cmd.as_str() {
        "-h" | "--help" | "help" => Ok(Cmd::Help),
        "id" => no_pos(Cmd::Id),
        "recv" => no_pos(Cmd::Recv {
            dir: PathBuf::from(get("--dir").unwrap_or(".")),
            port: get("--port")
                .map(|p| p.parse().map_err(|_| format!("bad port {p:?}")))
                .transpose()?
                .unwrap_or(DEFAULT_PORT),
            null: get("--null").is_some(),
        }),
        "bench" => no_pos(Cmd::Bench {
            size: get("--size")
                .map(parse_size)
                .transpose()?
                .unwrap_or(4 << 30),
            streams,
        }),
        "send" => {
            let [host, fp, paths @ ..] = pos.as_slice() else {
                return Err("send needs HOST and FINGERPRINT".into());
            };
            let fp = fp.to_ascii_lowercase();
            if fp.len() != 64 || !fp.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("bad fingerprint {fp:?} (64 hex chars)"));
            }
            let synthetic = get("--synthetic").map(parse_size).transpose()?;
            if synthetic.is_some() == !paths.is_empty() {
                return Err("send needs PATH... or --synthetic SIZE (not both)".into());
            }
            let paths = paths.iter().map(PathBuf::from).collect();
            Ok(Cmd::Send {
                host: host.to_string(),
                fp,
                paths,
                synthetic,
                streams,
            })
        }
        c => Err(format!("unknown command {c:?}")),
    }
}

/// `$UFT_HOME`, else `$XDG_CONFIG_HOME/uft`, else `$HOME/.config/uft`. Empty values count as unset.
fn config_dir(
    uft_home: Option<OsString>,
    xdg: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    set(uft_home)
        .or_else(|| set(xdg).map(|p| p.join("uft")))
        .or_else(|| set(home).map(|p| p.join(".config/uft")))
}

/// Appends DEFAULT_PORT when `host` has no port (handles bare IPv6 too).
fn with_port(host: &str) -> String {
    if host.parse::<SocketAddr>().is_ok() {
        host.to_string()
    } else if let Ok(ip) = host.parse::<IpAddr>() {
        SocketAddr::new(ip, DEFAULT_PORT).to_string()
    } else if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:{DEFAULT_PORT}")
    }
}

fn identity() -> Res<Identity> {
    let dir = config_dir(
        std::env::var_os("UFT_HOME"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
    .ok_or("no config dir: set UFT_HOME or HOME")?;
    Identity::load_or_create(&dir).map_err(|e| format!("identity in {}: {e}", dir.display()).into())
}

fn device_name() -> String {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "uft".into())
}

/// Decimal units, picked from `n` (1 MB = 1e6 B).
fn unit(n: u64) -> (f64, &'static str) {
    match n {
        0..1_000 => (1.0, "B"),
        1_000..1_000_000 => (1e3, "KB"),
        1_000_000..1_000_000_000 => (1e6, "MB"),
        1_000_000_000..1_000_000_000_000 => (1e9, "GB"),
        _ => (1e12, "TB"),
    }
}

fn human(n: u64) -> String {
    let (d, u) = unit(n);
    if d == 1.0 {
        format!("{n} B")
    } else {
        format!("{:.2} {u}", n as f64 / d)
    }
}

fn mbps(bytes_per_s: f64) -> String {
    format!("{:.1} MB/s", bytes_per_s / 1e6)
}

fn clock(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn status_line(p: &Snapshot, streams: u32) -> String {
    let pct = if p.total == 0 {
        100.0
    } else {
        p.done as f64 * 100.0 / p.total as f64
    };
    let (d, u) = unit(p.total);
    let eta = p.eta.map_or("-".into(), clock);
    format!(
        "{pct:6.1}%  {:.2}/{:.2} {u}  {}  ETA {eta}  streams {streams}",
        p.done as f64 / d,
        p.total as f64 / d,
        mbps(p.speed)
    )
}

fn print_status(p: &Snapshot, streams: u32) {
    // ponytail: trailing spaces instead of ANSI clear-line, fine for non-tty logs too.
    eprint!("\r{}    ", status_line(p, streams));
    let _ = std::io::stderr().flush();
}

fn summary(s: &SendStats) -> String {
    format!(
        "sent {} in {:.1}s: avg {}, peak {}, streams {}, resumed {}",
        human(s.bytes),
        s.secs,
        mbps(s.avg),
        mbps(s.peak),
        s.streams,
        human(s.resumed)
    )
}

fn run(cmd: Cmd) -> Res<()> {
    match cmd {
        Cmd::Help => println!("{USAGE}"),
        Cmd::Id => println!("{}", identity()?.fingerprint()),
        Cmd::Recv { dir, port, null } => {
            let id = identity()?;
            let listener = TcpListener::bind(("0.0.0.0", port))?;
            println!(
                "listening on {}  fingerprint {}",
                listener.local_addr()?,
                id.fingerprint()
            );
            let opts = RecvOptions {
                dir,
                null_sink: null,
                name: device_name(),
            };
            recv::serve(listener, Arc::new(id), opts, print_event)?;
        }
        Cmd::Send {
            host,
            fp,
            paths,
            synthetic,
            streams,
        } => {
            let target = with_port(&host);
            let addr = target
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| format!("{host}: no address"))?;
            let sources: Vec<Source> = match synthetic {
                Some(size) => vec![Source::Synthetic {
                    name: "synthetic.bin".into(),
                    size,
                }],
                None => paths.into_iter().map(Source::Path).collect(),
            };
            let opts = SendOptions {
                streams,
                name: device_name(),
            };
            let stats = send::send(addr, &identity()?, &fp, &sources, &opts, print_status)?;
            eprintln!();
            println!("{}", summary(&stats));
        }
        Cmd::Bench { size, streams } => {
            let (rx, tx) = (Identity::generate()?, Identity::generate()?);
            let rx_fp = rx.fingerprint();
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let addr = listener.local_addr()?;
            let opts = RecvOptions {
                dir: PathBuf::new(),
                null_sink: true,
                name: "bench-recv".into(),
            };
            std::thread::spawn(move || {
                if let Err(e) = recv::serve(listener, Arc::new(rx), opts, |_| {}) {
                    eprintln!("uft: bench receiver: {e}");
                }
            });
            let sources = [Source::Synthetic {
                name: "bench.bin".into(),
                size,
            }];
            let opts = SendOptions {
                streams,
                name: "bench-send".into(),
            };
            let s = send::send(addr, &tx, &rx_fp, &sources, &opts, print_status)?;
            eprintln!();
            println!(
                "bench: {} in {:.2}s = {:.2} GB/s ({}), streams {}",
                human(size),
                s.secs,
                s.avg / 1e9,
                mbps(s.avg),
                s.streams
            );
        }
    }
    Ok(())
}

fn print_event(ev: RecvEvent) {
    match ev {
        RecvEvent::Started {
            session,
            peer_fp,
            peer_name,
            files,
            total,
            resumed,
        } => {
            println!(
                "[{session}] from {peer_name} ({peer_fp}): {files} files, {} (resumed {})",
                human(total),
                human(resumed)
            )
        }
        RecvEvent::Progress {
            session,
            done,
            total,
        } => {
            let pct = if total == 0 {
                100.0
            } else {
                done as f64 * 100.0 / total as f64
            };
            eprint!(
                "\r[{session}] {pct:5.1}%  {}/{}    ",
                human(done),
                human(total)
            );
            let _ = std::io::stderr().flush();
        }
        RecvEvent::Done {
            session,
            bytes,
            secs,
        } => {
            eprintln!();
            println!(
                "[{session}] done: {} in {secs:.1}s, {}",
                human(bytes),
                mbps(bytes as f64 / secs.max(1e-9))
            );
        }
        RecvEvent::Failed { session, error } => {
            eprintln!();
            println!("[{session}] failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    const FP: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    #[test]
    fn sizes() {
        assert_eq!(parse_size("123"), Ok(123));
        assert_eq!(parse_size("4k"), Ok(4096));
        assert_eq!(parse_size("2M"), Ok(2 << 20));
        assert_eq!(parse_size("4G"), Ok(4 << 30));
        assert_eq!(parse_size("1t"), Ok(1 << 40));
        for bad in ["", "G", "1.5G", "-1", "1X", "99999999999T"] {
            assert!(parse_size(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn commands() {
        assert_eq!(parse_args(&[]), Ok(Cmd::Help));
        assert_eq!(parse_args(&args("recv --help")), Ok(Cmd::Help));
        assert_eq!(parse_args(&args("id")), Ok(Cmd::Id));
        assert_eq!(
            parse_args(&args("recv")),
            Ok(Cmd::Recv {
                dir: ".".into(),
                port: DEFAULT_PORT,
                null: false
            })
        );
        assert_eq!(
            parse_args(&args("recv --null --port 9 --dir /tmp")),
            Ok(Cmd::Recv {
                dir: "/tmp".into(),
                port: 9,
                null: true
            })
        );
        assert_eq!(
            parse_args(&args("bench")),
            Ok(Cmd::Bench {
                size: 4 << 30,
                streams: Streams::Auto
            })
        );
        assert_eq!(
            parse_args(&args("bench --size 2G --streams 4")),
            Ok(Cmd::Bench {
                size: 2 << 30,
                streams: Streams::Fixed(4)
            })
        );
        assert_eq!(
            parse_args(&args(&format!(
                "send h:1 {} a b --streams auto",
                FP.to_uppercase()
            ))),
            Ok(Cmd::Send {
                host: "h:1".into(),
                fp: FP.into(),
                paths: vec!["a".into(), "b".into()],
                synthetic: None,
                streams: Streams::Auto
            })
        );
        assert_eq!(
            parse_args(&args(&format!("send h {FP} --synthetic 1M"))),
            Ok(Cmd::Send {
                host: "h".into(),
                fp: FP.into(),
                paths: vec![],
                synthetic: Some(1 << 20),
                streams: Streams::Auto
            })
        );
        for bad in [
            "nope".to_string(),
            "id extra".into(),
            "recv --port x".into(),
            "recv --dir".into(),
            "recv --size 1G".into(),
            "bench --streams 0".into(),
            "bench --frobnicate".into(),
            "send h".into(),
            "send h abc x".into(),
            format!("send h {FP}"),
            format!("send h {FP} a --synthetic 1G"),
        ] {
            assert!(parse_args(&args(&bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn config_dirs() {
        let s = |v: &str| Some(OsString::from(v));
        assert_eq!(config_dir(s("/u"), s("/x"), s("/h")), Some("/u".into()));
        assert_eq!(config_dir(None, s("/x"), s("/h")), Some("/x/uft".into()));
        assert_eq!(
            config_dir(s(""), s(""), s("/h")),
            Some("/h/.config/uft".into())
        );
        assert_eq!(config_dir(None, None, None), None);
    }

    #[test]
    fn ports() {
        assert_eq!(with_port("10.0.0.2"), format!("10.0.0.2:{DEFAULT_PORT}"));
        assert_eq!(with_port("10.0.0.2:9"), "10.0.0.2:9");
        assert_eq!(with_port("fe80::1"), format!("[fe80::1]:{DEFAULT_PORT}"));
        assert_eq!(with_port("[fe80::1]:9"), "[fe80::1]:9");
        assert_eq!(
            with_port("phone.local"),
            format!("phone.local:{DEFAULT_PORT}")
        );
        assert_eq!(with_port("phone.local:9"), "phone.local:9");
    }

    #[test]
    fn formatting() {
        let p = Snapshot {
            done: 1_210_000_000,
            total: 2_680_000_000,
            speed: 74.2e6,
            peak: 0.0,
            eta: Some(19.4),
            elapsed: 0.0,
        };
        assert_eq!(
            status_line(&p, 3),
            "  45.1%  1.21/2.68 GB  74.2 MB/s  ETA 0:19  streams 3"
        );
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1_500_000), "1.50 MB");
    }
}
