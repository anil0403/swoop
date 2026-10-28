//! End-to-end: real sender ↔ real receiver over loopback TLS.

use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uft::recv::{self, RecvOptions};
use uft::send::{self, SendOptions, Source, Streams};
use uft::tls::Identity;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("uft-it-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Deterministic pseudo-random bytes.
fn data(seed: &str, len: usize) -> Vec<u8> {
    let mut out = vec![0; len];
    blake3::Hasher::new().update(seed.as_bytes()).finalize_xof().fill(&mut out);
    out
}

/// Starts a receiver; returns its address and fingerprint.
fn receiver(dir: &Path, null_sink: bool) -> (SocketAddr, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let id = Arc::new(Identity::generate().unwrap());
    let fp = id.fingerprint();
    let opts = RecvOptions { dir: dir.to_path_buf(), null_sink, name: "rx".into() };
    std::thread::spawn(move || recv::serve(listener, id, opts, |_| {}));
    (addr, fp)
}

fn opts(streams: Streams) -> SendOptions {
    SendOptions { streams, name: "tx".into() }
}

#[test]
fn transfers_tree_intact() {
    let src = temp_dir("src");
    let dst = temp_dir("dst");
    fs::create_dir_all(src.join("album/sub")).unwrap();
    let files = [
        ("album/big.bin", data("big", 5 * (2 << 20) + 12345)),
        ("album/empty.txt", Vec::new()),
        ("album/sub/small.txt", data("small", 100)),
    ];
    for (p, d) in &files {
        fs::write(src.join(p), d).unwrap();
    }

    let (addr, fp) = receiver(&dst, false);
    let sender = Identity::generate().unwrap();
    let stats = send::send(addr, &sender, &fp, &[Source::Path(src.join("album"))], &opts(Streams::Fixed(3)), |_, _| {})
        .unwrap();

    let total: u64 = files.iter().map(|(_, d)| d.len() as u64).sum();
    assert_eq!(stats.bytes + stats.resumed, total);
    for (p, d) in &files {
        assert_eq!(&fs::read(dst.join(p)).unwrap(), d, "{p}");
    }
}

#[test]
fn many_small_files() {
    let src = temp_dir("many-src");
    let dst = temp_dir("many-dst");
    fs::create_dir_all(src.join("photos")).unwrap();
    for i in 0..2000 {
        fs::write(src.join(format!("photos/{i:04}.jpg")), data(&i.to_string(), 4096)).unwrap();
    }
    let (addr, fp) = receiver(&dst, false);
    let sender = Identity::generate().unwrap();
    send::send(addr, &sender, &fp, &[Source::Path(src.join("photos"))], &opts(Streams::Auto), |_, _| {}).unwrap();
    assert_eq!(fs::read_dir(dst.join("photos")).unwrap().count(), 2000);
    assert_eq!(fs::read(dst.join("photos/1234.jpg")).unwrap(), data("1234", 4096));
}

#[test]
fn synthetic_to_null_sink() {
    let (addr, fp) = receiver(&temp_dir("null"), true);
    let sender = Identity::generate().unwrap();
    let size = 256 << 20;
    let src = [Source::Synthetic { name: "bench.bin".into(), size }];
    let stats = send::send(addr, &sender, &fp, &src, &opts(Streams::Fixed(2)), |_, _| {}).unwrap();
    assert_eq!(stats.bytes, size);
    assert!(stats.avg > 0.0);
}

#[test]
fn wrong_fingerprint_is_refused() {
    let (addr, _) = receiver(&temp_dir("wrongfp"), true);
    let sender = Identity::generate().unwrap();
    let src = [Source::Synthetic { name: "x".into(), size: 1 }];
    assert!(send::send(addr, &sender, &"0".repeat(64), &src, &opts(Streams::Fixed(1)), |_, _| {}).is_err());
}

#[test]
fn second_send_resumes_completed_file() {
    let src = temp_dir("resume-src");
    let dst = temp_dir("resume-dst");
    let file = src.join("video.bin");
    fs::write(&file, data("video", 3 << 20)).unwrap();
    let (addr, fp) = receiver(&dst, false);
    let sender = Identity::generate().unwrap();
    let first = send::send(addr, &sender, &fp, &[Source::Path(file.clone())], &opts(Streams::Fixed(1)), |_, _| {}).unwrap();
    assert_eq!(first.resumed, 0);
    let second = send::send(addr, &sender, &fp, &[Source::Path(file)], &opts(Streams::Fixed(1)), |_, _| {}).unwrap();
    assert_eq!((second.bytes, second.resumed), (0, 3 << 20));
    assert!(!dst.join("video (1).bin").exists());
}
