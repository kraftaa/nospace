use crate::model::{CreateProbe, ErrnoInfo, ProbePhase, ProbeResult};
use std::ffi::CString;
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn create_probe(directory: &Path) -> CreateProbe {
    CreateProbe {
        outcome: run_create_probe(directory),
    }
}

fn run_create_probe(directory: &Path) -> ProbeResult {
    let directory = match CString::new(directory.as_os_str().as_bytes()) {
        Ok(path) => path,
        Err(_) => return failure(ProbePhase::OpenDirectory, libc::EINVAL),
    };
    // SAFETY: directory is a valid NUL-terminated path.
    let directory_fd = unsafe {
        libc::open(
            directory.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if directory_fd < 0 {
        return last_failure(ProbePhase::OpenDirectory);
    }

    let name = CString::new(format!(
        ".nospace-probe-{}-{:016x}",
        std::process::id(),
        random_suffix()
    ))
    .expect("generated probe name cannot contain NUL");

    // SAFETY: directory_fd is open and name is NUL terminated.
    let file_fd = unsafe {
        libc::openat(
            directory_fd,
            name.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY | libc::O_CLOEXEC,
            0o600,
        )
    };
    if file_fd < 0 {
        let result = last_failure(ProbePhase::Create);
        close_fd(directory_fd);
        return result;
    }

    // Remove the directory entry before writing. The open descriptor remains
    // valid, but no probe pathname is left behind if write or fsync fails.
    // SAFETY: directory_fd is open and name is relative to it.
    if unsafe { libc::unlinkat(directory_fd, name.as_ptr(), 0) } != 0 {
        let result = last_failure(ProbePhase::Unlink);
        close_fd(file_fd);
        close_fd(directory_fd);
        return result;
    }

    let byte = [0x4eu8];
    // SAFETY: file_fd is open and byte points to one readable byte.
    let written = unsafe { libc::write(file_fd, byte.as_ptr().cast(), byte.len()) };
    if written != byte.len() as isize {
        let result = if written < 0 {
            last_failure(ProbePhase::Write)
        } else {
            failure(ProbePhase::Write, libc::EIO)
        };
        close_fd(file_fd);
        close_fd(directory_fd);
        return result;
    }

    // Force delayed allocation before claiming that a write succeeded.
    // SAFETY: file_fd is a valid open file descriptor.
    if unsafe { libc::fsync(file_fd) } != 0 {
        let result = last_failure(ProbePhase::Fsync);
        close_fd(file_fd);
        close_fd(directory_fd);
        return result;
    }

    // SAFETY: file_fd is valid and is closed exactly once here.
    if unsafe { libc::close(file_fd) } != 0 {
        let result = last_failure(ProbePhase::Close);
        close_fd(directory_fd);
        return result;
    }
    close_fd(directory_fd);
    ProbeResult::Success
}

fn close_fd(fd: RawFd) {
    // SAFETY: callers pass an owned descriptor and ignore cleanup-only errors.
    unsafe {
        libc::close(fd);
    }
}

fn random_suffix() -> u64 {
    let mut value = 0u64;
    // SAFETY: value points to writable memory of the requested size.
    let received = unsafe {
        libc::getrandom(
            (&mut value as *mut u64).cast(),
            std::mem::size_of::<u64>(),
            0,
        )
    };
    if received == std::mem::size_of::<u64>() as isize {
        return value;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

fn last_failure(phase: ProbePhase) -> ProbeResult {
    let errno = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO);
    failure(phase, errno)
}

fn failure(phase: ProbePhase, errno: i32) -> ProbeResult {
    ProbeResult::Error {
        phase,
        errno: ErrnoInfo::new(errno),
    }
}
