use nospace::diagnose;
use nospace::model::*;
use std::path::PathBuf;

fn evidence(create: ProbeResult, watch: ProbeResult, available: u64, inodes: u64) -> Evidence {
    Evidence {
        target: PathBuf::from("/test"),
        target_exists: true,
        probe_directory: PathBuf::from("/test"),
        mount: MountInfo {
            mount_id: 1,
            mount_point: PathBuf::from("/"),
            source: Some("/dev/test".to_owned()),
            fs_type: "ext4".to_owned(),
            options: vec!["rw".to_owned()],
        },
        filesystem: FsStats {
            total_bytes: 100,
            free_bytes: available,
            available_bytes: available,
            total_inodes: 100,
            free_inodes: inodes,
            inode_accounting_supported: true,
            read_only: false,
        },
        create_probe: CreateProbe { outcome: create },
        inotify_probe: InotifyProbe {
            init: ProbeResult::Success,
            add_watch: watch,
        },
        inotify_limits: InotifyLimits {
            max_user_watches: Some(100),
            max_user_instances: Some(10),
            max_queued_events: Some(1000),
        },
        proc_scan: ProcScan {
            deleted_open: Vec::new(),
            inotify_consumers: Vec::new(),
            processes_scanned: 1,
            processes_inaccessible: 0,
            complete: true,
        },
    }
}

fn error(errno: i32) -> ProbeResult {
    ProbeResult::Error {
        phase: ProbePhase::Create,
        errno: ErrnoInfo::new(errno),
    }
}

#[test]
fn confirms_inode_exhaustion_only_with_failed_create_and_supported_counter() {
    let result = diagnose(&evidence(error(libc::ENOSPC), ProbeResult::Success, 50, 0));
    assert_eq!(result.status, ResultStatus::Confirmed);
    assert_eq!(result.causes, vec![Diagnosis::InodeExhaustion]);
}

#[test]
fn confirms_block_exhaustion() {
    let result = diagnose(&evidence(error(libc::ENOSPC), ProbeResult::Success, 0, 80));
    assert_eq!(result.causes, vec![Diagnosis::BlockExhaustion]);
}

#[test]
fn reports_both_when_both_counters_are_exhausted() {
    let result = diagnose(&evidence(error(libc::ENOSPC), ProbeResult::Success, 0, 0));
    assert_eq!(
        result.causes,
        vec![Diagnosis::InodeExhaustion, Diagnosis::BlockExhaustion]
    );
}

#[test]
fn confirms_inotify_only_when_create_succeeds() {
    let watch_error = ProbeResult::Error {
        phase: ProbePhase::AddWatch,
        errno: ErrnoInfo::new(libc::ENOSPC),
    };
    let confirmed = diagnose(&evidence(ProbeResult::Success, watch_error.clone(), 50, 50));
    assert_eq!(confirmed.causes, vec![Diagnosis::InotifyExhaustion]);

    let unknown = diagnose(&evidence(error(libc::EACCES), watch_error, 50, 50));
    assert_eq!(unknown.status, ResultStatus::Unknown);
}

#[test]
fn confirms_read_only_from_the_syscall() {
    let result = diagnose(&evidence(error(libc::EROFS), ProbeResult::Success, 50, 50));
    assert_eq!(result.causes, vec![Diagnosis::ReadOnlyFilesystem]);
}

#[test]
fn unknown_is_first_class() {
    let result = diagnose(&evidence(error(libc::ENOSPC), ProbeResult::Success, 50, 50));
    assert_eq!(result.status, ResultStatus::Unknown);
    assert!(
        result
            .notes
            .iter()
            .any(|note| note.contains("not explained"))
    );
}

#[test]
fn successful_supported_probes_report_no_supported_failure() {
    let result = diagnose(&evidence(
        ProbeResult::Success,
        ProbeResult::Success,
        50,
        50,
    ));
    assert_eq!(result.status, ResultStatus::NoSupportedFailure);
    assert_eq!(
        serde_json::to_value(result.status).unwrap(),
        "no_supported_failure"
    );
}

#[test]
fn unsupported_inode_accounting_never_confirms_inodes() {
    let mut evidence = evidence(error(libc::ENOSPC), ProbeResult::Success, 50, 0);
    evidence.mount.fs_type = "btrfs".to_owned();
    evidence.filesystem.inode_accounting_supported = false;
    let result = diagnose(&evidence);
    assert_eq!(result.status, ResultStatus::Unknown);
    assert!(result.causes.is_empty());
}

#[test]
fn cleanup_enospc_is_not_classified_as_allocation_exhaustion() {
    let cleanup_error = ProbeResult::Error {
        phase: ProbePhase::Unlink,
        errno: ErrnoInfo::new(libc::ENOSPC),
    };
    let result = diagnose(&evidence(cleanup_error, ProbeResult::Success, 0, 0));
    assert_eq!(result.status, ResultStatus::Unknown);
    assert!(result.causes.is_empty());
}
