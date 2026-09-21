use nospace::model::*;
use nospace::render;
use std::path::PathBuf;

fn successful_evidence() -> Evidence {
    Evidence {
        target: PathBuf::from("/test"),
        probe_directory: PathBuf::from("/test"),
        mount: MountInfo {
            mount_id: 1,
            mount_point: PathBuf::from("/"),
            source: Some("/dev/test".to_owned()),
            fs_type: "ext4".to_owned(),
            options: vec!["rw".to_owned()],
        },
        filesystem: FsStats {
            total_bytes: 1024,
            free_bytes: 512,
            available_bytes: 512,
            total_inodes: 100,
            free_inodes: 50,
            inode_accounting_supported: true,
            read_only: false,
        },
        create_probe: CreateProbe {
            outcome: ProbeResult::Success,
        },
        inotify_probe: InotifyProbe {
            init: ProbeResult::Success,
            add_watch: ProbeResult::Success,
        },
        inotify_limits: InotifyLimits {
            max_user_watches: Some(100),
            max_user_instances: Some(10),
            max_queued_events: Some(1000),
        },
        proc_scan: ProcScan {
            deleted_open: Vec::new(),
            inotify_consumers: vec![InotifyConsumer {
                pid: 42,
                process: "editor".to_owned(),
                watches: 3,
            }],
            processes_scanned: 1,
            processes_inaccessible: 0,
            complete: true,
        },
    }
}

#[test]
fn successful_output_makes_a_narrow_claim_and_stays_quiet() {
    let evidence = successful_evidence();
    let diagnosis = DiagnosisResult {
        status: ResultStatus::NoSupportedFailure,
        causes: Vec::new(),
        contributing: Vec::new(),
        notes: Vec::new(),
    };

    let output = render::text(&evidence, &diagnosis, false);
    assert!(output.contains("file probe       OK"));
    assert!(output.contains("OK: no supported failure detected"));
    assert!(!output.contains("HEALTHY"));
    assert!(!output.contains("Top inotify consumers"));

    let verbose = render::text(&evidence, &diagnosis, true);
    assert!(verbose.contains("Top inotify consumers"));
}
