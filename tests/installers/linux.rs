//! Native Linux installer subprocesses against the real compiled binary and isolated prefixes.
use sha2::{Digest, Sha256};
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
    fs::copy(env!("CARGO_BIN_EXE_maris"), source.join("bin/maris")).unwrap();
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    fs::write(
        source.join(".maris-package"),
        format!("maris-package-v1\nlinux\n{arch}\n0.1.0\n"),
    )
    .unwrap();
    let prefix = root.join("prefix with spaces");
    (temp, source, prefix)
}
fn run(source: &Path, prefix: &Path, options: &[&str]) -> Output {
    Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .arg("--from")
        .arg(source)
        .arg("--prefix")
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
fn linux_dry_run_and_noninteractive_rejection_preserve_the_filesystem() {
    let (_temp, source, prefix) = setup();
    ok(run(&source, &prefix, &["--allow-unsigned", "--dry-run"]));
    assert!(!prefix.exists());
    assert!(!run(&source, &prefix, &["--allow-unsigned"])
        .status
        .success());
    assert!(!prefix.exists());
}
#[test]
fn linux_requires_explicit_trust_and_checks_provided_hashes() {
    let (_temp, source, prefix) = setup();
    assert!(!run(&source, &prefix, &["--yes"]).status.success());
    assert!(
        !run(&source, &prefix, &["--sha256", &"0".repeat(64), "--yes"])
            .status
            .success()
    );
    assert!(!prefix.exists());
    let digest = hex::encode(Sha256::digest(fs::read(source.join("bin/maris")).unwrap()));
    ok(run(&source, &prefix, &["--sha256", &digest, "--yes"]));
    assert!(prefix.join("lib/maris/bin/maris").is_file());
}
#[test]
fn linux_installs_a_scoped_launcher_and_preserves_upgrade_backups() {
    let (_temp, source, prefix) = setup();
    fs::create_dir_all(&prefix).unwrap();
    fs::write(prefix.join("keep.txt"), "untouched").unwrap();
    ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
    assert_eq!(
        fs::read_link(prefix.join("bin/maris")).unwrap(),
        Path::new("../lib/maris/bin/maris")
    );
    fs::write(prefix.join("lib/maris/previous.txt"), "old").unwrap();
    ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
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
    assert!(!prefix.join("lib/maris/previous.txt").exists());
    assert_eq!(
        fs::read_to_string(prefix.join("keep.txt")).unwrap(),
        "untouched"
    );
    assert!(!prefix.join(".maris-install.lock").exists());
}
#[test]
fn linux_refuses_unrelated_launchers_and_busy_installations() {
    let (_temp, source, prefix) = setup();
    fs::create_dir_all(prefix.join("bin")).unwrap();
    fs::write(prefix.join("bin/maris"), "unrelated").unwrap();
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    assert_eq!(
        fs::read_to_string(prefix.join("bin/maris")).unwrap(),
        "unrelated"
    );
    fs::remove_file(prefix.join("bin/maris")).unwrap();
    fs::create_dir(prefix.join(".maris-install.lock")).unwrap();
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    assert!(prefix.join(".maris-install.lock").exists());
    assert!(!prefix.join("lib/maris").exists());
}
#[test]
fn linux_rejects_wrong_platform_architecture_and_linked_payloads() {
    use std::os::unix::fs::symlink;
    let (_temp, source, prefix) = setup();
    let marker = fs::read_to_string(source.join(".maris-package")).unwrap();
    fs::write(
        source.join(".maris-package"),
        marker.replace("linux", "windows"),
    )
    .unwrap();
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    fs::write(source.join(".maris-package"), marker).unwrap();
    let binary = fs::read(source.join("bin/maris")).unwrap();
    let mut wrong = binary.clone();
    wrong[18] = 0;
    fs::write(source.join("bin/maris"), wrong).unwrap();
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    fs::write(source.join("bin/maris"), binary).unwrap();
    symlink("../.maris-package", source.join("resources/link")).unwrap();
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    assert!(!prefix.exists());
}
#[test]
fn linux_promotion_failure_rolls_back_and_keeps_the_original_launcher() {
    use std::os::unix::fs::PermissionsExt;
    let (temp, source, prefix) = setup();
    ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
    fs::write(prefix.join("lib/maris/previous.txt"), "old").unwrap();
    let tools = temp.path().canonicalize().unwrap().join("fault-tools");
    fs::create_dir(&tools).unwrap();
    let command = tools.join("mv");
    fs::write(
        &command,
        "#!/bin/bash\ncase \"$2\" in */.maris-stage.*/Maris) exit 23;; esac\nexec /bin/mv \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new("bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .arg("--from")
        .arg(&source)
        .arg("--prefix")
        .arg(&prefix)
        .args(["--allow-unsigned", "--yes"])
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", tools.display()),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(prefix.join("lib/maris/previous.txt")).unwrap(),
        "old"
    );
    assert!(prefix.join("bin/maris").exists());
    assert!(!prefix.join(".maris-install.lock").exists());
}
#[test]
fn linux_does_not_replace_or_terminate_a_running_installed_binary() {
    use std::process::{Child, Stdio};
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            self.0.stdin.take();
            let _ = self.0.wait();
        }
    }
    let (temp, source, prefix) = setup();
    ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
    let mut child = Running(
        Command::new(prefix.join("bin/maris"))
            .arg("mcp")
            .env("MARIS_STATE_DIR", temp.path().join("isolated-state"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
        .status
        .success());
    assert!(child.0.try_wait().unwrap().is_none());
}
