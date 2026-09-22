use crate::fs::filesystem_stats;
use crate::inotify::{inotify_probe, read_limits};
use crate::model::{CreateProbe, Evidence, InotifyProbe, ProbeResult};
use crate::mountinfo::resolve_mount;
use crate::probe::create_probe;
use crate::procfs;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default)]
pub struct CollectOptions {
    pub no_probe: bool,
}

pub fn collect(path: &Path, options: CollectOptions) -> Result<Evidence, String> {
    let resolved = resolve_target(path)?;
    let target = resolved.target;
    let target_exists = resolved.target_exists;
    let probe_directory = resolved.probe_directory;
    let inspection_path = resolved.inspection_path;
    let metadata = fs::metadata(&inspection_path)
        .map_err(|error| format!("cannot inspect {}: {error}", inspection_path.display()))?;

    let mount = resolve_mount(&inspection_path)?;
    let filesystem = filesystem_stats(&inspection_path, &mount.fs_type)?;
    let create_probe_result = if options.no_probe {
        CreateProbe {
            outcome: ProbeResult::NotRun,
        }
    } else {
        create_probe(&probe_directory)
    };
    let inotify_probe_result = if options.no_probe {
        InotifyProbe {
            init: ProbeResult::NotRun,
            add_watch: ProbeResult::NotRun,
        }
    } else {
        inotify_probe(&inspection_path)
    };
    let proc_scan = procfs::scan(metadata.dev());

    Ok(Evidence {
        target,
        target_exists,
        probe_directory,
        mount,
        filesystem,
        create_probe: create_probe_result,
        inotify_probe: inotify_probe_result,
        inotify_limits: read_limits(),
        proc_scan,
    })
}

struct ResolvedTarget {
    target: PathBuf,
    target_exists: bool,
    probe_directory: PathBuf,
    inspection_path: PathBuf,
}

fn resolve_target(path: &Path) -> Result<ResolvedTarget, String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("cannot resolve current directory: {error}"))?
            .join(path)
    };

    match fs::metadata(&absolute) {
        Ok(metadata) => {
            let target = fs::canonicalize(&absolute)
                .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
            let probe_directory = if metadata.is_dir() {
                target.clone()
            } else {
                target
                    .parent()
                    .map(PathBuf::from)
                    .ok_or_else(|| format!("{} has no parent directory", target.display()))?
            };
            Ok(ResolvedTarget {
                inspection_path: target.clone(),
                target,
                target_exists: true,
                probe_directory,
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            resolve_missing_target(&absolute)
        }
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

fn resolve_missing_target(absolute: &Path) -> Result<ResolvedTarget, String> {
    let mut candidate = absolute;
    let mut missing = Vec::new();
    loop {
        let name = candidate
            .file_name()
            .ok_or_else(|| format!("cannot find an existing parent for {}", absolute.display()))?;
        missing.push(name.to_owned());
        candidate = candidate
            .parent()
            .ok_or_else(|| format!("cannot find an existing parent for {}", absolute.display()))?;

        match fs::metadata(candidate) {
            Ok(metadata) if metadata.is_dir() => {
                let inspection_path = fs::canonicalize(candidate).map_err(|error| {
                    format!("cannot resolve parent {}: {error}", candidate.display())
                })?;
                let mut target = inspection_path.clone();
                for component in missing.iter().rev() {
                    target.push(component);
                }
                return Ok(ResolvedTarget {
                    target,
                    target_exists: false,
                    probe_directory: inspection_path.clone(),
                    inspection_path,
                });
            }
            Ok(_) => {
                return Err(format!(
                    "nearest existing parent is not a directory: {}",
                    candidate.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!("cannot inspect {}: {error}", candidate.display()));
            }
        }
    }
}
