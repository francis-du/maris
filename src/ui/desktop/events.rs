//! Main-thread desktop presentation. No audio callbacks, arbitrary commands or alternate state writes.
use crate::control::store::Store;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use anyhow::bail;
#[cfg(target_os = "windows")]
use anyhow::Context;
use anyhow::Result;
use std::process::Command;

pub(super) fn open_console(store: &Store) -> Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;
        fn quote(s: &str) -> String {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
        let script = store.directory.join("Open Maris.command");
        let contents = format!(
            "#!/bin/sh\nexport MARIS_STATE_DIR={}\nexec {} --no-tray --lang {} tui\n",
            quote(&store.directory.to_string_lossy()),
            quote(&exe.to_string_lossy()),
            quote(crate::i18n::code())
        );
        std::fs::write(&script, contents)?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let status = Command::new("/usr/bin/open")
            .args(["-a", "Terminal"])
            .arg(script)
            .status()?;
        anyhow::ensure!(
            status.success(),
            "Terminal launcher failed; run 'maris tui' manually"
        );
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("wt.exe")
            .arg("new-tab")
            .arg(exe)
            .args(["--no-tray", "--lang", crate::i18n::code(), "tui"])
            .env("MARIS_STATE_DIR", &store.directory)
            .spawn()
            .context("Install Windows Terminal or run 'maris tui' manually")?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        for terminal in ["x-terminal-emulator", "konsole", "xterm"] {
            if Command::new(terminal)
                .arg("-e")
                .arg(&exe)
                .args(["--no-tray", "--lang", crate::i18n::code(), "tui"])
                .env("MARIS_STATE_DIR", &store.directory)
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        bail!("No supported terminal launcher found; run 'maris tui' manually")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (store, exe);
        bail!("Unsupported desktop platform")
    }
}

/// None means this platform retains the visible native-menu Apply/Cancel workflow.
/// The native review is displayed immediately after selection, never from a telemetry refresh.
pub(super) fn confirm(title: &str, lines: &[String]) -> Result<Option<bool>> {
    let details = lines
        .iter()
        .map(|line| crate::ui::theme::clean(line))
        .collect::<Vec<_>>()
        .join("\n");
    #[cfg(target_os = "macos")]
    {
        use objc2::{
            class, msg_send,
            rc::{Allocated, Retained},
            runtime::AnyObject,
            MainThreadMarker,
        };
        use objc2_foundation::NSString;
        anyhow::ensure!(
            MainThreadMarker::new().is_some(),
            "Menu confirmation requires the main thread"
        );
        // SAFETY: documented AppKit selectors/NSInteger values, retained lifetime, main-thread only.
        unsafe {
            let allocated: Allocated<AnyObject> = msg_send![class!(NSAlert), alloc];
            let alert: Retained<AnyObject> = msg_send![allocated, init];
            let _: () = msg_send![&alert, setMessageText: &*NSString::from_str(title)];
            let _: () = msg_send![&alert, setInformativeText: &*NSString::from_str(&details)];
            let _: () = msg_send![&alert, setAlertStyle: 0_usize];
            // Cancel is the default button. Return cannot accidentally apply an output switch.
            let cancel: Retained<AnyObject> = msg_send![&alert, addButtonWithTitle: &*NSString::from_str(crate::i18n::text("Cancel selection"))];
            let _: () = msg_send![&cancel, setKeyEquivalent: &*NSString::from_str("\r")];
            let _: Retained<AnyObject> = msg_send![&alert, addButtonWithTitle: &*NSString::from_str(crate::i18n::text("Apply selection"))];
            let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![&app, activateIgnoringOtherApps: true];
            let response: isize = msg_send![&alert, runModal];
            Ok(Some(response == 1001))
        }
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(window: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
        }
        let text: Vec<u16> = details.encode_utf16().chain(Some(0)).collect();
        let caption: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        // SAFETY: live NUL-terminated UTF-16 buffers, no parent HWND; MB_DEFBUTTON2 defaults to No.
        let response =
            unsafe { MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x4 | 0x20 | 0x100) };
        anyhow::ensure!(response != 0, "Native menu confirmation failed");
        Ok(Some(response == 6))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (title, details);
        Ok(None)
    }
}
