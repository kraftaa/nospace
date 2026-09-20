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
    if !path.exists() {
        return Err(format!("target does not exist: {}", path.display()));
    }
    let target = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    let metadata = fs::metadata(&target)
        .map_err(|error| format!("cannot inspect {}: {error}", target.display()))?;
    let probe_directory = if metadata.is_dir() {
        target.clone()
    } else {
        target
            .parent()
            .map(PathBuf::from)
            .ok_or_else(|| format!("{} has no parent directory", target.display()))?
    };

    let mount = resolve_mount(&target)?;
    let filesystem = filesystem_stats(&target, &mount.fs_type)?;
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
        inotify_probe(&target)
    };
    let proc_scan = procfs::scan(metadata.dev());

    Ok(Evidence {
        target,
        probe_directory,
        mount,
        filesystem,
        create_probe: create_probe_result,
        inotify_probe: inotify_probe_result,
        inotify_limits: read_limits(),
        proc_scan,
    })
}
