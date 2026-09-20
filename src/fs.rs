use crate::model::FsStats;
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub fn filesystem_stats(path: &Path, fs_type: &str) -> Result<FsStats, String> {
    let path_string = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "target path contains a NUL byte".to_owned())?;
    let mut stats = MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: path_string is NUL terminated and stats points to writable memory.
    let result = unsafe { libc::statvfs(path_string.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "statvfs failed for {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: a successful statvfs initialized the structure.
    let stats = unsafe { stats.assume_init() };
    let fragment_size = if stats.f_frsize == 0 {
        stats.f_bsize
    } else {
        stats.f_frsize
    };

    Ok(FsStats {
        total_bytes: checked_bytes(stats.f_blocks, fragment_size, "total bytes")?,
        free_bytes: checked_bytes(stats.f_bfree, fragment_size, "free bytes")?,
        available_bytes: checked_bytes(stats.f_bavail, fragment_size, "available bytes")?,
        total_inodes: stats.f_files,
        free_inodes: stats.f_ffree,
        inode_accounting_supported: has_fixed_inode_accounting(fs_type, stats.f_files),
        read_only: stats.f_flag & libc::ST_RDONLY as libc::c_ulong != 0,
    })
}

fn checked_bytes(blocks: u64, size: u64, label: &str) -> Result<u64, String> {
    blocks
        .checked_mul(size)
        .ok_or_else(|| format!("overflow while calculating {label}"))
}

fn has_fixed_inode_accounting(fs_type: &str, total_inodes: u64) -> bool {
    total_inodes > 0 && matches!(fs_type, "ext2" | "ext3" | "ext4")
}
