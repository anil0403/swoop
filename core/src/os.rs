//! The few file operations that differ between Linux (and Android), Apple's systems and Windows.

use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(unix)]
pub use std::os::unix::fs::FileExt;

/// `read_exact_at` / `write_all_at` with Unix's meaning: positional, the file cursor doesn't matter.
#[cfg(windows)]
pub trait FileExt {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
    fn write_all_at(&self, buf: &[u8], offset: u64) -> io::Result<()>;
}

#[cfg(windows)]
impl FileExt for File {
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_read(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn write_all_at(&self, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_write(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Hints that `f` will be read front to back.
pub fn advise_sequential(f: &File) {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let _ = rustix::fs::fadvise(f, 0, None, rustix::fs::Advice::Sequential);
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    let _ = f;
}

/// Grows `f` to `size` bytes, reserving the space where the filesystem can.
pub fn preallocate(f: &File, size: u64) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use rustix::io::Errno;
        match rustix::fs::fallocate(f, rustix::fs::FallocateFlags::empty(), 0, size) {
            Err(Errno::OPNOTSUPP | Errno::NOSYS) => f.set_len(size),
            r => Ok(r?),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    f.set_len(size)
}

/// Makes everything written under `dir` durable. Linux: one syncfs for the whole filesystem, so
/// `files` aren't needed. Apple and Windows have no such call: each of `files` is flushed instead.
pub fn sync(dir: &Path, files: &[&Path]) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let _ = files;
        // Android shared storage (FUSE) sometimes says ENOENT, or EACCES (seen on API 37), for a
        // folder that exists and was just written to, on open or on syncfs; a full sync() is
        // slower but just as safe.
        match File::open(dir).and_then(|f| Ok(rustix::fs::syncfs(f)?)) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                rustix::fs::sync();
                Ok(())
            }
            r => r,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = dir;
        // Windows' FlushFileBuffers needs a handle with write access; Apple's sync_all is F_FULLFSYNC.
        for f in files {
            std::fs::OpenOptions::new()
                .write(true)
                .open(f)?
                .sync_all()?;
        }
        Ok(())
    }
}

/// Bytes free for this user on the disk holding `dir`, or None when the system won't say.
pub fn free_space(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let fs = rustix::fs::statvfs(dir).ok()?;
        Some(fs.f_bavail.saturating_mul(fs.f_frsize))
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let mut free = 0u64;
        // SAFETY: a NUL-terminated path and a valid out pointer; the other outputs may be null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide(dir).as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }
}

/// Whether `dir` is on FAT32 (or older FAT), which can't hold a file of 4 GiB or more.
// f_type's integer type differs between targets.
#[allow(clippy::unnecessary_cast)]
pub fn is_fat(dir: &Path) -> bool {
    #[cfg(all(unix, not(target_vendor = "apple")))]
    {
        const MSDOS_SUPER_MAGIC: i64 = 0x4d44;
        rustix::fs::statfs(dir).is_ok_and(|s| s.f_type as i64 == MSDOS_SUPER_MAGIC)
    }
    // Apple numbers filesystem types as they're loaded: the name says it.
    #[cfg(target_vendor = "apple")]
    {
        rustix::fs::statfs(dir).is_ok_and(|s| {
            s.f_fstypename
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .eq(*b"msdos")
        })
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetVolumeInformationW, GetVolumePathNameW};
        let mut root = [0u16; 261];
        let mut name = [0u16; 261];
        // SAFETY: NUL-terminated input, output buffers with their real lengths, null for the
        // outputs not asked for.
        let ok = unsafe {
            GetVolumePathNameW(wide(dir).as_ptr(), root.as_mut_ptr(), root.len() as u32) != 0
                && GetVolumeInformationW(
                    root.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    name.as_mut_ptr(),
                    name.len() as u32,
                ) != 0
        };
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        ok && String::from_utf16_lossy(&name[..end]).starts_with("FAT")
    }
}

/// `p` as a NUL-terminated UTF-16 string for Win32 calls.
#[cfg(windows)]
fn wide(p: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    p.as_os_str().encode_wide().chain([0]).collect()
}
