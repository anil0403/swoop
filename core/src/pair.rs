//! Pairing: a short code both devices show, and the list of trusted devices.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// 6-digit code like `"482 913"` from two cert fingerprints, the same in either order.
pub fn code(a: &str, b: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut h = blake3::Hasher::new();
    h.update(b"uft pair\n");
    h.update(lo.as_bytes());
    h.update(b"\n");
    h.update(hi.as_bytes());
    let bytes = h.finalize();
    let n = u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap()) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

/// Fingerprints of trusted devices, kept in `dir/trusted` as `FINGERPRINT NAME` lines.
pub struct Trust {
    path: PathBuf,
    fps: HashSet<String>,
}

impl Trust {
    /// Reads `dir/trusted`; a missing file is an empty store, malformed lines are skipped.
    pub fn load(dir: &Path) -> io::Result<Trust> {
        let path = dir.join("trusted");
        let text = match fs::read_to_string(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            r => r?,
        };
        let fps = text
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(fp, _)| fp)
            .filter(|fp| is_fp(fp))
            .map(str::to_owned)
            .collect();
        Ok(Trust { path, fps })
    }

    pub fn contains(&self, fp: &str) -> bool {
        self.fps.contains(fp)
    }

    /// Trusts `fp` from now on (appends to the file, mode 0600); no-op if already trusted.
    pub fn add(&mut self, fp: &str, name: &str) -> io::Result<()> {
        if !is_fp(fp) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bad fingerprint",
            ));
        }
        if self.contains(fp) {
            return Ok(());
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let name = name.replace(['\n', '\r'], " ");
        OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(&self.path)?
            .write_all(format!("{fp} {name}\n").as_bytes())?;
        self.fps.insert(fp.to_owned());
        Ok(())
    }
}

/// 64 hex chars, as `tls::fingerprint` makes.
fn is_fp(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn code_is_symmetric_six_digits() {
        let (a, b) = ("a".repeat(64), "b".repeat(64));
        let c = code(&a, &b);
        assert_eq!(c, code(&b, &a));
        assert_ne!(c, code(&a, &"c".repeat(64)));
        let (x, y) = c.split_once(' ').unwrap();
        assert!(
            [x, y]
                .iter()
                .all(|p| p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))
        );
    }

    #[test]
    fn trust_round_trips() {
        let dir = std::env::temp_dir().join(format!("uft-pair-{}", crate::proto::random_u64()));
        let mut t = Trust::load(&dir).unwrap();
        let (a, b) = ("a".repeat(64), "B".repeat(64));
        assert!(!t.contains(&a));
        t.add(&a, "my phone").unwrap();
        t.add(&b, "evil\nffff name\r").unwrap();
        t.add(&a, "again").unwrap();
        assert!(t.add("x y", "z").is_err());
        let path = dir.join("trusted");
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::write(&path, format!("{text}garbage\n\nshort name\n")).unwrap();
        let t = Trust::load(&dir).unwrap();
        assert!(t.contains(&a) && t.contains(&b));
        assert_eq!(t.fps.len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
