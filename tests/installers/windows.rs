//! Native PowerShell installer tests. Every payload and destination is an isolated user temp path.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source = root.join("payload with spaces/Maris");
    fs::create_dir_all(source.join("bin")).unwrap();
    fs::create_dir(source.join("resources")).unwrap();
    fs::write(source.join("resources/notice.txt"), "fixture notice").unwrap();
    fs::copy(env!("CARGO_BIN_EXE_maris"), source.join("bin/maris.exe")).unwrap();
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    fs::write(
        source.join(".maris-package"),
        format!("maris-package-v1\nwindows\n{arch}\n0.1.0\n"),
    )
    .unwrap();
    (temp, source, root.join("Programs with spaces"))
}
fn run(source: &Path, prefix: &Path, options: &[&str]) -> Output {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"))
        .arg("-From")
        .arg(source)
        .arg("-Prefix")
        .arg(prefix)
        .args(options)
        .output()
        .unwrap()
}
fn ok(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn windows_checksum_helper_handles_empty_binary_and_multiblock_files_without_cmdlets() {
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir().unwrap();
    let mut cases = Vec::new();
    for (name, data) in [
        ("empty file", Vec::new()),
        ("abc.txt", b"abc".to_vec()),
        (
            "binary file",
            (0..1_048_577).map(|index| (index % 251) as u8).collect(),
        ),
    ] {
        fs::write(directory.path().join(name), &data).unwrap();
        cases.push(
            serde_json::json!({"name":name,"expected":hex::encode_upper(Sha256::digest(&data))}),
        );
    }
    fs::write(
        directory.path().join("cases.json"),
        serde_json::to_vec(&cases).unwrap(),
    )
    .unwrap();
    ok(Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/checksum_probe.ps1"
        ))
        .arg("-Fixture")
        .arg(directory.path())
        .output()
        .unwrap());
}

#[test]
fn windows_dry_run_and_missing_confirmation_never_write() {
    let (_temp, source, prefix) = setup();
    ok(run(&source, &prefix, &["-AllowUnsigned", "-DryRun"]));
    assert!(!prefix.exists());
    assert!(!run(&source, &prefix, &["-AllowUnsigned"]).status.success());
    assert!(!prefix.exists());
}
#[test]
fn windows_unsigned_payload_is_not_a_trusted_signed_release() {
    let (_temp, source, prefix) = setup();
    assert!(!run(&source, &prefix, &["-Yes"]).status.success());
    assert!(!prefix.exists());
}
#[test]
fn windows_install_and_upgrade_retain_the_previous_payload_and_other_files() {
    let (_temp, source, prefix) = setup();
    fs::create_dir_all(&prefix).unwrap();
    fs::write(prefix.join("keep.txt"), "untouched").unwrap();
    ok(run(&source, &prefix, &["-AllowUnsigned", "-Yes"]));
    assert!(prefix.join("Maris/bin/maris.exe").exists());
    fs::write(prefix.join("Maris/previous.txt"), "old").unwrap();
    ok(run(&source, &prefix, &["-AllowUnsigned", "-Yes"]));
    let backups: Vec<_> = fs::read_dir(&prefix)
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".maris-backup.")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert!(backups[0].path().join("Maris/previous.txt").exists());
    assert!(!prefix.join("Maris/previous.txt").exists());
    assert_eq!(
        fs::read_to_string(prefix.join("keep.txt")).unwrap(),
        "untouched"
    );
    assert!(!prefix.join(".maris-install.lock").exists());
}
#[test]
fn windows_lock_and_unrelated_destination_are_never_overwritten() {
    let (_temp, source, prefix) = setup();
    fs::create_dir_all(&prefix).unwrap();
    fs::write(prefix.join(".maris-install.lock"), "busy").unwrap();
    assert!(!run(&source, &prefix, &["-AllowUnsigned", "-Yes"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(prefix.join(".maris-install.lock")).unwrap(),
        "busy"
    );
    fs::remove_file(prefix.join(".maris-install.lock")).unwrap();
    fs::create_dir(prefix.join("Maris")).unwrap();
    fs::write(prefix.join("Maris/keep.txt"), "unrelated").unwrap();
    assert!(!run(&source, &prefix, &["-AllowUnsigned", "-Yes"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(prefix.join("Maris/keep.txt")).unwrap(),
        "unrelated"
    );
}
#[test]
fn windows_rejects_wrong_platform_or_non_pe_images_before_installation() {
    let (_temp, source, prefix) = setup();
    let marker = fs::read_to_string(source.join(".maris-package")).unwrap();
    fs::write(
        source.join(".maris-package"),
        marker.replace("windows", "linux"),
    )
    .unwrap();
    assert!(!run(&source, &prefix, &["-AllowUnsigned", "-Yes"])
        .status
        .success());
    fs::write(source.join(".maris-package"), marker).unwrap();
    fs::write(source.join("bin/maris.exe"), "not a PE executable").unwrap();
    assert!(!run(&source, &prefix, &["-AllowUnsigned", "-Yes"])
        .status
        .success());
    assert!(!prefix.exists());
}
#[test]
fn windows_rejects_traversal_and_machine_wide_destinations() {
    let (_temp, source, prefix) = setup();
    // PathBuf::join normalizes .. after a Windows verbatim prefix. Construct
    // the raw argument without joining so the installer receives the attack.
    for suffix in ["/../other", "\\..\\other", "/./other"] {
        let mut raw = prefix.as_os_str().to_os_string();
        raw.push(suffix);
        let traversal = PathBuf::from(raw);
        assert!(traversal.as_os_str().to_string_lossy().ends_with(suffix));
        let result = run(&source, &traversal, &["-AllowUnsigned", "-Yes"]);
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("traversal"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!prefix.exists());
        assert!(!prefix.parent().unwrap().join("other").exists());
    }
    let outside = Path::new("C:\\Maris-Unused-System-Destination");
    assert!(!run(&source, outside, &["-AllowUnsigned", "-DryRun"])
        .status
        .success());
    assert!(!prefix.exists());
}
#[test]
fn windows_does_not_replace_or_terminate_an_in_use_installed_executable() {
    use std::process::{Child, Stdio};
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            self.0.stdin.take();
            let _ = self.0.wait();
        }
    }
    let (temp, source, prefix) = setup();
    ok(run(&source, &prefix, &["-AllowUnsigned", "-Yes"]));
    let mut child = Running(
        Command::new(prefix.join("Maris/bin/maris.exe"))
            .arg("mcp")
            .env("MARIS_STATE_DIR", temp.path().join("isolated-state"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!run(&source, &prefix, &["-AllowUnsigned", "-Yes"])
        .status
        .success());
    assert!(child.0.try_wait().unwrap().is_none());
}
