use crate::model::{ErrnoInfo, InotifyLimits, InotifyProbe, ProbePhase, ProbeResult};
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub fn inotify_probe(path: &Path) -> InotifyProbe {
    // SAFETY: inotify_init1 has no pointer arguments.
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
    if fd < 0 {
        return InotifyProbe {
            init: last_failure(ProbePhase::Init),
            add_watch: ProbeResult::NotRun,
        };
    }

    let path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(path) => path,
        Err(_) => {
            close_fd(fd);
            return InotifyProbe {
                init: ProbeResult::Success,
                add_watch: failure(ProbePhase::AddWatch, libc::EINVAL),
            };
        }
    };
    // SAFETY: fd is an inotify instance and path is NUL terminated.
    let watch = unsafe { libc::inotify_add_watch(fd, path.as_ptr(), libc::IN_ATTRIB) };
    let add_watch = if watch < 0 {
        last_failure(ProbePhase::AddWatch)
    } else {
        ProbeResult::Success
    };
    close_fd(fd);
    InotifyProbe {
        init: ProbeResult::Success,
        add_watch,
    }
}

pub fn read_limits() -> InotifyLimits {
    InotifyLimits {
        max_user_watches: read_number("/proc/sys/fs/inotify/max_user_watches"),
        max_user_instances: read_number("/proc/sys/fs/inotify/max_user_instances"),
        max_queued_events: read_number("/proc/sys/fs/inotify/max_queued_events"),
    }
}

fn read_number(path: &str) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
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

fn close_fd(fd: i32) {
    // SAFETY: caller owns the descriptor.
    unsafe {
        libc::close(fd);
    }
}
