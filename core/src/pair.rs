//! Pairing: a short code both devices show, and the list of trusted devices.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// 6-digit code like `"482 913"` for one connection, from both certs and a nonce from each side.
///
/// The sender commits to its nonce (`commit`, in Hello) before it sees the receiver's (Welcome)
/// and reveals it after (Offer). So neither side, nor anyone in the middle, can pick keys or
/// nonces that make two different connections show the same code: a man in the middle gets
/// one guess in a million per prompt the user sees.
pub fn code(
    sender_fp: &str,
    receiver_fp: &str,
    sender_nonce: &str,
    receiver_nonce: &str,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"swoop pair 2\n");
    for part in [sender_fp, receiver_fp, sender_nonce, receiver_nonce] {
        h.update(part.as_bytes());
        h.update(b"\n");
    }
    let bytes = h.finalize();
    let n = u64::from_le_bytes(bytes.as_bytes()[..8].try_into().unwrap()) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

/// 256 random bits, hex: one side's share of a pairing code.
pub fn nonce() -> io::Result<String> {
    let mut b = [0u8; 32];
    rustls::crypto::aws_lc_rs::default_provider()
        .secure_random
        .fill(&mut b)
        .map_err(|_| io::Error::other("no secure random numbers"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// What the sender sends first instead of its nonce.
pub fn commit(nonce: &str) -> String {
    blake3::hash(nonce.as_bytes()).to_hex().to_string()
}

/// Fingerprints of trusted devices, kept in `dir/trusted` as `FINGERPRINT NAME` lines.
pub struct Trust {
    path: PathBuf,
    /// Fingerprint → the name it had when added.
    fps: HashMap<String, String>,
}

impl Trust {
    /// Reads `dir/trusted`; a missing file is an empty store, malformed lines are skipped.
    pub fn load(dir: &Path) -> io::Result<Trust> {
        Trust::load_file(dir.join("trusted"))
    }

    /// Reads a store in the same format, e.g. `dir/known`: receivers this device sent to.
    pub fn load_file(path: PathBuf) -> io::Result<Trust> {
        let text = match fs::read_to_string(&path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            r => r?,
        };
        let fps = text
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(fp, _)| is_fp(fp))
            .map(|(fp, name)| (fp.to_owned(), name.to_owned()))
            .collect();
        Ok(Trust { path, fps })
    }

    pub fn contains(&self, fp: &str) -> bool {
        self.fps.contains_key(fp)
    }

    /// Whether some device in here goes by `name` (ignoring case and outer spaces).
    pub fn has_name(&self, name: &str) -> bool {
        self.fps
            .values()
            .any(|n| n.trim().eq_ignore_ascii_case(name.trim()))
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
        self.fps.insert(fp.to_owned(), name);
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
    fn code_is_six_digits_per_connection() {
        let (a, b) = ("a".repeat(64), "b".repeat(64));
        let (na, nb) = (nonce().unwrap(), nonce().unwrap());
        assert_eq!(na.len(), 64);
        assert_ne!(na, nb);
        let c = code(&a, &b, &na, &nb);
        assert_eq!(c, code(&a, &b, &na, &nb));
        assert_ne!(c, code(&b, &a, &na, &nb));
        assert_ne!(c, code(&a, &b, &na, &nonce().unwrap()));
        assert_ne!(commit(&na), commit(&nb));
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
        assert!(t.has_name(" MY PHONE") && !t.has_name("again"));
        assert_eq!(t.fps.len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }
}
