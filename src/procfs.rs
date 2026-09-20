use crate::model::{DeletedOpenFile, InotifyConsumer, ProcScan, ProcessFd};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub fn scan(target_device: u64) -> ProcScan {
    // SAFETY: getuid has no pointer arguments or preconditions.
    let current_uid = unsafe { libc::getuid() };
    let mut deleted: BTreeMap<(u64, u64), DeletedOpenFile> = BTreeMap::new();
    let mut consumers = Vec::new();
    let mut scanned = 0u64;
    let mut inaccessible = 0u64;

    let entries = match fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => {
            return ProcScan {
                deleted_open: Vec::new(),
                inotify_consumers: Vec::new(),
                processes_scanned: 0,
                processes_inaccessible: 1,
                complete: false,
            };
        }
    };

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process_dir = entry.path();
        let process_metadata = match fs::metadata(&process_dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                inaccessible += 1;
                continue;
            }
        };
        let process = process_name(&process_dir, pid);
        let fd_dir = process_dir.join("fd");
        let descriptors = match fs::read_dir(&fd_dir) {
            Ok(descriptors) => descriptors,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                inaccessible += 1;
                continue;
            }
        };
        scanned += 1;
        let mut process_incomplete = false;

        for descriptor in descriptors {
            let descriptor = match descriptor {
                Ok(descriptor) => descriptor,
                Err(_) => {
                    process_incomplete = true;
                    continue;
                }
            };
            let Some(fd) = descriptor
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())
            else {
                continue;
            };
            let descriptor_path = descriptor.path();
            let metadata = match fs::metadata(&descriptor_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    process_incomplete = true;
                    continue;
                }
            };
            if metadata.file_type().is_file()
                && metadata.nlink() == 0
                && metadata.dev() == target_device
            {
                let key = (metadata.dev(), metadata.ino());
                let holder = ProcessFd {
                    pid,
                    process: process.clone(),
                    fd,
                };
                if let Some(file) = deleted.get_mut(&key) {
                    file.holders.push(holder);
                } else {
                    let allocated_bytes = metadata.blocks().saturating_mul(512);
                    let path = fs::read_link(&descriptor_path)
                        .unwrap_or_else(|_| PathBuf::from("<deleted file>"));
                    deleted.insert(
                        key,
                        DeletedOpenFile {
                            device: metadata.dev(),
                            inode: metadata.ino(),
                            path,
                            allocated_bytes,
                            holders: vec![holder],
                        },
                    );
                }
            }
        }

        if process_metadata.uid() == current_uid {
            let (watches, watch_scan_complete) = count_inotify_watches(&process_dir);
            process_incomplete |= !watch_scan_complete;
            if watches > 0 {
                consumers.push(InotifyConsumer {
                    pid,
                    process,
                    watches,
                });
            }
        }
        if process_incomplete {
            inaccessible += 1;
        }
    }

    let mut deleted_open: Vec<_> = deleted.into_values().collect();
    deleted_open.sort_by_key(|file| std::cmp::Reverse(file.allocated_bytes));
    consumers.sort_by_key(|consumer| std::cmp::Reverse(consumer.watches));
    ProcScan {
        deleted_open,
        inotify_consumers: consumers,
        processes_scanned: scanned,
        processes_inaccessible: inaccessible,
        complete: inaccessible == 0,
    }
}

fn count_inotify_watches(process_dir: &Path) -> (u64, bool) {
    let fdinfo = match fs::read_dir(process_dir.join("fdinfo")) {
        Ok(entries) => entries,
        Err(_) => return (0, false),
    };
    let mut watches = 0u64;
    let mut complete = true;
    for entry in fdinfo {
        let contents = match entry.and_then(|entry| fs::read_to_string(entry.path())) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        watches += contents
            .lines()
            .filter(|line| line.starts_with("inotify wd:"))
            .count() as u64;
    }
    (watches, complete)
}

fn process_name(process_dir: &Path, pid: u32) -> String {
    fs::read_to_string(process_dir.join("comm"))
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|_| pid.to_string())
}
