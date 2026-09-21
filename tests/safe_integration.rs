use nospace::{CollectOptions, ResultStatus, collect, diagnose};
use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("nospace-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn successful_directory_is_probed_and_cleaned_up() {
    let directory = temporary_directory("success");
    let evidence = collect(&directory, CollectOptions::default()).unwrap();
    assert_eq!(diagnose(&evidence).status, ResultStatus::NoSupportedFailure);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
    fs::remove_dir(&directory).unwrap();
}

#[test]
fn finds_a_deleted_open_file_by_link_count() {
    let directory = temporary_directory("deleted");
    let path = directory.join("held-open.log");
    let file = File::create(&path).unwrap();
    file.set_len(1024 * 1024).unwrap();
    file.sync_all().unwrap();
    fs::remove_file(&path).unwrap();

    let evidence = collect(&directory, CollectOptions { no_probe: true }).unwrap();
    assert!(evidence.proc_scan.deleted_open.iter().any(|entry| {
        entry
            .holders
            .iter()
            .any(|holder| holder.pid == std::process::id())
    }));

    drop(file);
    fs::remove_dir(&directory).unwrap();
}
