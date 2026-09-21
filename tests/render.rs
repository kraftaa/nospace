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
            max_user_watches: Some(1_048_576),
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
    assert!(!output.contains("Inotify usage"));
    assert!(!output.contains("Top inotify consumers"));

    let verbose = render::text(&evidence, &diagnosis, true);
    assert!(verbose.contains("Inotify usage (current UID):"));
    assert!(verbose.contains("observed watches       3"));
    assert!(verbose.contains("max_user_watches       1,048,576"));
    assert!(verbose.contains("Top inotify consumers"));
}

#[test]
fn incomplete_inotify_usage_is_labeled_as_a_lower_bound() {
    let mut evidence = successful_evidence();
    evidence.proc_scan.complete = false;
    let diagnosis = DiagnosisResult {
        status: ResultStatus::Unknown,
        causes: Vec::new(),
        contributing: Vec::new(),
        notes: Vec::new(),
    };

    let output = render::text(&evidence, &diagnosis, true);
    assert!(output.contains("3 (lower bound; process scan incomplete)"));
}

#[test]
fn confirmed_inotify_exhaustion_shows_context_without_verbose() {
    let evidence = successful_evidence();
    let diagnosis = DiagnosisResult {
        status: ResultStatus::Confirmed,
        causes: vec![Diagnosis::InotifyExhaustion],
        contributing: Vec::new(),
        notes: Vec::new(),
    };

    let output = render::text(&evidence, &diagnosis, false);
    assert!(output.contains("CONFIRMED: inotify resource exhaustion"));
    assert!(output.contains("Inotify usage (current UID):"));
    assert!(output.contains("Top inotify consumers:"));
}
