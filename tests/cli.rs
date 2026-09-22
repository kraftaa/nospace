use std::process::Command;

#[test]
fn check_mode_returns_success_for_a_supported_healthy_probe() {
    let output = Command::new(env!("CARGO_BIN_EXE_nospace"))
        .args(["/tmp", "--check"])
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("OK: no supported failure detected"));
}

#[test]
fn check_mode_distinguishes_unknown_from_command_errors() {
    let unknown = Command::new(env!("CARGO_BIN_EXE_nospace"))
        .args(["/tmp", "--no-probe", "--check"])
        .output()
        .unwrap();
    assert_eq!(unknown.status.code(), Some(3), "{unknown:?}");
    assert!(String::from_utf8_lossy(&unknown.stdout).contains("UNKNOWN"));

    let command_error = Command::new(env!("CARGO_BIN_EXE_nospace"))
        .arg("--check")
        .output()
        .unwrap();
    assert_eq!(command_error.status.code(), Some(2), "{command_error:?}");
}
