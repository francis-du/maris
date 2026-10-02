//! Native installer process tests use isolated state; they never start system audio.
#[cfg(target_os = "linux")]
#[path = "installers/linux.rs"]
mod linux;
#[cfg(target_os = "windows")]
#[path = "installers/windows.rs"]
mod windows;
#[cfg(target_os = "macos")]
mod macos {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::{Command, Output},
    };

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let source = root.join("source with spaces/Maris.app");
        fs::create_dir_all(source.join("Contents/MacOS")).unwrap();
        fs::write(
            source.join("Contents/Info.plist"),
            include_str!("../Maris-Info.plist"),
        )
        .unwrap();
        // Use the real built Maris binary. Validation inspects it, never executes it.
        fs::copy(
            env!("CARGO_BIN_EXE_maris"),
            source.join("Contents/MacOS/maris"),
        )
        .unwrap();
        let prefix = root.join("Applications with spaces");
        (temp, source, prefix)
    }
    fn run(source: &Path, prefix: &Path, options: &[&str]) -> Output {
        Command::new("/bin/bash")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
            .arg("--from")
            .arg(source)
            .arg("--prefix")
            .arg(prefix)
            .args(options)
            .output()
            .unwrap()
    }
    fn assert_ok(output: Output) {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn dry_run_does_not_create_prefix_or_touch_preferences() {
        let (_temp, source, prefix) = setup();
        assert_ok(run(&source, &prefix, &["--allow-unsigned", "--dry-run"]));
        assert!(!prefix.exists());
    }
    #[test]
    fn noninteractive_install_needs_explicit_confirmation() {
        let (_temp, source, prefix) = setup();
        let output = run(&source, &prefix, &["--allow-unsigned"]);
        assert!(!output.status.success());
        assert!(!prefix.exists());
    }
    #[test]
    fn development_bundle_is_not_accepted_as_a_verified_release() {
        let (_temp, source, prefix) = setup();
        assert!(!run(&source, &prefix, &["--yes"]).status.success());
        assert!(!prefix.exists());
    }
    #[test]
    fn install_and_upgrade_retain_previous_bundle_and_unrelated_files() {
        let (_temp, source, prefix) = setup();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(prefix.join("keep.txt"), "unrelated").unwrap();
        assert_ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
        let installed = prefix.join("Maris.app");
        assert_eq!(
            fs::read(installed.join("Contents/Info.plist")).unwrap(),
            fs::read(source.join("Contents/Info.plist")).unwrap()
        );
        fs::write(installed.join("previous-marker"), "preserved").unwrap();
        assert_ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
        let backups: Vec<_> = fs::read_dir(&prefix)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".maris-backup.")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].path().join("Maris.app/previous-marker").exists());
        assert!(!installed.join("previous-marker").exists());
        assert_eq!(
            fs::read_to_string(prefix.join("keep.txt")).unwrap(),
            "unrelated"
        );
        assert!(!prefix.join(".maris-install.lock").exists());
    }
    #[test]
    fn unrelated_destination_and_active_install_lock_are_never_clobbered() {
        let (_temp, source, prefix) = setup();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(prefix.join("Maris.app"), "unrelated").unwrap();
        assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
            .status
            .success());
        assert_eq!(
            fs::read_to_string(prefix.join("Maris.app")).unwrap(),
            "unrelated"
        );
        fs::remove_file(prefix.join("Maris.app")).unwrap();
        fs::create_dir(prefix.join(".maris-install.lock")).unwrap();
        assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
            .status
            .success());
        assert!(prefix.join(".maris-install.lock").is_dir());
        assert!(!prefix.join("Maris.app").exists());
    }
    #[test]
    fn symlink_components_and_bundle_payloads_fail_without_writes() {
        use std::os::unix::fs::symlink;
        let (temp, source, prefix) = setup();
        let root = temp.path().canonicalize().unwrap();
        symlink(&root, root.join("alias")).unwrap();
        assert!(!run(
            &source,
            &root.join("alias/Applications"),
            &["--allow-unsigned", "--yes"]
        )
        .status
        .success());
        symlink("Info.plist", source.join("Contents/extra")).unwrap();
        assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
            .status
            .success());
        assert!(!prefix.exists());
    }
    #[test]
    fn a_failed_promotion_restores_the_previous_bundle() {
        use std::os::unix::fs::PermissionsExt;
        let (temp, source, prefix) = setup();
        assert_ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
        fs::write(prefix.join("Maris.app/keep-old"), "old").unwrap();
        let tools = temp.path().canonicalize().unwrap().join("fault-tools");
        fs::create_dir(&tools).unwrap();
        let injected = tools.join("mv");
        // A deterministic filesystem-command failure, not simulated hardware acceptance.
        fs::write(&injected, "#!/bin/bash\ncase \"$2\" in */.maris-stage.*/Maris.app) exit 23;; esac\nexec /bin/mv \"$@\"\n").unwrap();
        fs::set_permissions(&injected, fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new("/bin/bash")
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
            fs::read_to_string(prefix.join("Maris.app/keep-old")).unwrap(),
            "old"
        );
        assert!(!prefix.join(".maris-install.lock").exists());
    }
    #[test]
    fn an_open_installed_executable_is_not_replaced_or_terminated() {
        use std::process::{Child, Stdio};
        struct Running(Child);
        impl Drop for Running {
            fn drop(&mut self) {
                self.0.stdin.take();
                let _ = self.0.wait();
            }
        }
        let (temp, source, prefix) = setup();
        assert_ok(run(&source, &prefix, &["--allow-unsigned", "--yes"]));
        let mut running = Running(
            Command::new(prefix.join("Maris.app/Contents/MacOS/maris"))
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
        assert!(running.0.try_wait().unwrap().is_none());
    }
    #[test]
    fn incorrect_bundle_identity_fails_closed() {
        let (_temp, source, prefix) = setup();
        fs::write(
            source.join("Contents/Info.plist"),
            include_str!("../Maris-Info.plist").replace("audio.maris.app", "example.unrelated"),
        )
        .unwrap();
        assert!(!run(&source, &prefix, &["--allow-unsigned", "--yes"])
            .status
            .success());
        assert!(!prefix.exists());
    }
}

#[test]
fn installer_syntax_and_side_effect_contract_are_explicit() {
    let script = include_str!("../install.sh");
    let local = include_str!("../scripts/install_macos.sh");
    for forbidden in [
        "sudo ",
        "launchctl ",
        "osascript ",
        "xattr -",
        "killall ",
        "spctl --master-disable",
    ] {
        assert!(!script
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .any(|line| line.contains(forbidden)));
    }
    assert!(
        !script.contains("cargo build"),
        "normal installation must not compile"
    );
    assert!(script.contains("https://github.com/francis-du/maris/releases"));
    assert!(script.contains("--proto-redir '=https'"));
    assert!(script.contains("trap cleanup_download EXIT"));
    assert!(local.contains("cargo build --release --locked"));
    assert!(local.contains("trap cleanup EXIT"));
    assert!(
        !local.contains("curl "),
        "native offline installation remains network-free"
    );
    assert!(script.contains("--dry-run"));
}
