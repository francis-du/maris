//! End-to-end preset regressions: real PTY bytes and persisted state, not synthetic key reducers.
use super::*;

fn wait_screen(process: &mut ConsoleProcess, label: &str, done: impl Fn(&str) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = String::new();
    loop {
        process.drain(&mut output);
        let screen = process.terminal.screen().contents();
        if done(&screen) {
            return;
        }
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "console exited: {label}"
        );
        assert!(Instant::now() < deadline, "{label}: {screen}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_terminal_search_keeps_repeated_letters_read_only_then_confirms_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/Bass");
    wait_screen(&mut process, "repeated search letters", |text| {
        text.contains("Search: Bass")
    });
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(
        !dir.path().join("control.json").exists(),
        "search letter s requested audio stop"
    );
    process.send("\x15scene:dialogue");
    wait_screen(&mut process, "filtered dialogue", |text| {
        text.contains("Search: scene:dialogue")
    });
    wait_for_preset(
        &mut process,
        Some("Dialogue"),
        Instant::now() + Duration::from_secs(5),
    );
    process.send("\r");
    wait_until(&mut process, "filtered scene applied", || {
        listening::load(&store).unwrap().revision == 1
    });
    process.send("\r\r");
    idle(&mut process, Duration::from_millis(150));
    assert_eq!(listening::load(&store).unwrap().revision, 1);
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_empty_search_cannot_apply_and_escape_clears_before_closing() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/zzzz-no-match");
    wait_screen(&mut process, "empty search", |text| {
        text.contains("No matches")
    });
    process.send("\r");
    idle(&mut process, Duration::from_millis(100));
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    process.send("\x1b");
    wait_screen(&mut process, "search cleared", |text| {
        text.contains("PRESETS") && !text.contains("Search:")
    });
    process.send("\x1b");
    wait_screen(&mut process, "picker closed", |text| {
        !text.contains("PRESETS")
    });
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_search_does_not_bypass_changed_configuration_guard() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/bass");
    wait_screen(&mut process, "search opened", |text| {
        text.contains("Search: bass")
    });
    store
        .edit(Some(0), |profile| {
            profile.bypass = true;
            Ok(())
        })
        .unwrap();
    wait_screen(&mut process, "picker invalidated", |text| {
        text.contains(maris::i18n::for_language(
            "en",
            "Configuration changed; reopen the picker",
        ))
    });
    process.send("\x1b[200~scene:dialogue\rqs\x1b[201~\r");
    idle(&mut process, Duration::from_millis(100));
    assert_eq!(store.load().unwrap().revision, 1);
    assert!(store.load().unwrap().profile.bypass);
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_bracketed_paste_search_waits_for_explicit_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/");
    wait_screen(&mut process, "search ready for paste", |text| {
        text.contains("Search:")
    });
    process.send("\x1b[200~scene:dialogue\r\n\x1b[201~");
    wait_screen(&mut process, "pasted search text", |text| {
        text.contains("Search: scene:dialogue")
    });
    wait_for_preset(
        &mut process,
        Some("Dialogue"),
        Instant::now() + Duration::from_secs(5),
    );
    assert_eq!(store.load().unwrap().revision, 0);
    assert_eq!(
        listening::load(&store).unwrap().revision,
        0,
        "pasted newline confirmed a scene"
    );
    assert!(!dir.path().join("control.json").exists());
    process.send("\r");
    wait_until(&mut process, "explicit confirmation after paste", || {
        listening::load(&store).unwrap().revision == 1
    });
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        1.0
    );
}

#[test]
fn real_terminal_bracketed_paste_mode_is_enabled_and_restored_on_exit() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    assert!(
        process.first_output.contains("\x1b[?2004h"),
        "terminal never enabled bracketed paste"
    );
    process.send("\x1b[200~p\rqs\x03\x1b[201~");
    idle(&mut process, Duration::from_millis(100));
    assert!(!dir.path().join("control.json").exists());
    assert_eq!(store.load().unwrap().revision, 0);
    process.send("q");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = String::new();
    loop {
        process.drain(&mut output);
        if let Some(status) = process.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "console failed to exit");
        std::thread::sleep(Duration::from_millis(10));
    }
    process.drain(&mut output);
    assert!(
        output.contains("\x1b[?2004l"),
        "terminal left bracketed paste enabled"
    );
}

#[test]
fn real_terminal_oversized_paste_blocks_old_selection_until_search_is_edited() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/scene:dialogue");
    wait_for_preset(
        &mut process,
        Some("Dialogue"),
        Instant::now() + Duration::from_secs(5),
    );
    process.send(&format!("\x1b[200~{}\x1b[201~", "x".repeat(257)));
    wait_screen(&mut process, "oversized paste warning", |text| {
        text.contains("Search text is too long")
    });
    process.send("\r");
    idle(&mut process, Duration::from_millis(100));
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert_eq!(store.load().unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
    assert!(!dir.path().join("listening-previous.json").exists());
    process.send("\x15\x1b[200~scene:focus\x1b[201~");
    wait_for_preset(
        &mut process,
        Some("Focus"),
        Instant::now() + Duration::from_secs(5),
    );
    process.send("\r");
    wait_until(&mut process, "edited search after rejected paste", || {
        listening::load(&store).unwrap().revision == 1
    });
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        -0.25
    );
}

#[test]
fn real_terminal_paste_payload_can_arrive_in_separate_reads_without_shortcuts() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p/");
    wait_screen(&mut process, "search before split paste", |text| {
        text.contains("Search:")
    });
    process.send("\x1b[200~scene:");
    idle(&mut process, Duration::from_millis(60));
    process.send("dialogue\r\n");
    idle(&mut process, Duration::from_millis(60));
    process.send("\x1b[201~");
    wait_screen(&mut process, "split paste completes", |text| {
        text.contains("Search: scene:dialogue")
    });
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
    process.send("\x1b");
    wait_screen(&mut process, "split paste cancellation", |text| {
        !text.contains("Search:")
    });
    assert_eq!(listening::load(&store).unwrap().revision, 0);
}

fn wait_until(process: &mut ConsoleProcess, label: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut output = String::new();
    while !done() {
        process.drain(&mut output);
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "console exited: {label}"
        );
        assert!(
            Instant::now() < deadline,
            "{label}; terminal tail: {:?}",
            output.chars().rev().take(1800).collect::<String>()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn idle(process: &mut ConsoleProcess, duration: Duration) {
    let until = Instant::now() + duration;
    let mut output = String::new();
    while Instant::now() < until {
        process.drain(&mut output);
        assert!(process.child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
}

// Read the rendered popup row rather than a fixed visual-column slice.
// PTY cell widths can differ across native runners (notably macOS Intel), so
// column cropping can cut the selected preset name even when the row is
// correctly highlighted. Use the last marker on a row because the dashboard
// and preset popup can both render a ▶ on the same line.
fn highlighted_preset(process: &ConsoleProcess) -> Option<String> {
    process
        .terminal
        .screen()
        .contents()
        .lines()
        .find_map(|row| {
            let (prefix, highlighted) = row.rsplit_once('▶')?;
            if !prefix.contains('│') {
                return None;
            }
            let highlighted = highlighted.trim_start();
            let name = highlighted
                .split_once("  ")
                .map_or(highlighted, |(name, _)| name)
                .trim();
            (!name.is_empty()).then(|| name.to_owned())
        })
}

fn visible_preset_row(process: &ConsoleProcess, expected: &str) -> Option<u16> {
    process
        .terminal
        .screen()
        .rows(29, 70)
        .enumerate()
        .find_map(|(row, text)| {
            text.contains(expected)
                .then_some(u16::try_from(row).expect("PTY row fits u16"))
        })
}

fn wait_for_preset(
    process: &mut ConsoleProcess,
    expected: Option<&str>,
    deadline: Instant,
) -> String {
    let mut output = String::new();
    loop {
        process.drain(&mut output);
        assert!(process.child.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "selected preset was not rendered: {expected:?}; screen: {}",
            process.terminal.screen().contents()
        );
        if let Some(name) = highlighted_preset(process) {
            if maris::presets::console_catalog()
                .iter()
                .any(|preset| preset.name.eq_ignore_ascii_case(&name))
                && expected.is_none_or(|expected| name.eq_ignore_ascii_case(expected))
            {
                return name;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn select_preset(process: &mut ConsoleProcess, id: &str) {
    let catalog = maris::presets::console_catalog();
    let selected = catalog
        .iter()
        .position(|p| p.id == id)
        .expect("known preset");
    let deadline = Instant::now() + Duration::from_secs(5);
    // A persisted confirmation can precede its closing redraw. Consume that
    // frame before reopening so an old highlighted row cannot acknowledge P.
    let mut output = String::new();
    loop {
        process.drain(&mut output);
        assert!(process.child.try_wait().unwrap().is_none());
        assert!(
            Instant::now() < deadline,
            "previous preset picker did not close"
        );
        if highlighted_preset(process).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    process.send("p");
    let current = wait_for_preset(process, None, deadline);
    let mut current = catalog
        .iter()
        .position(|preset| preset.name.eq_ignore_ascii_case(&current))
        .expect("rendered preset belongs to the catalog");
    // A highlighted first row does not acknowledge a queued burst of no-op Up
    // keys. Navigate from the rendered row and acknowledge each real move, so
    // later pointer/Enter actions cannot sit behind an unconsumed reset burst.
    // Do not call the preset implementation or synthesize internal KeyEvents.
    // Retain both keyboard directions: reach the first row with real Up keys,
    // then move down to the requested scene. No saturated no-op keys are queued.
    for destination in [0, selected] {
        while current != destination {
            if current < destination {
                process.send("\x1b[B");
                current += 1;
            } else {
                process.send("\x1b[A");
                current -= 1;
            }
            wait_for_preset(process, Some(&catalog[current].name), deadline);
        }
    }
}

#[test]
fn real_terminal_preset_enter_survives_reading_delay_on_home_and_settings() {
    for settings in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path());
        let _lease = store.session_lock().unwrap();
        let _heartbeat = Heartbeat::start(store.clone());
        let mut process = spawn_console(dir.path());
        if settings {
            process.send("e");
            idle(&mut process, Duration::from_millis(100));
        }
        select_preset(&mut process, "scene:dialogue");
        idle(&mut process, Duration::from_millis(1400));
        assert_eq!(
            store.load().unwrap().revision,
            0,
            "browsing wrote global EQ"
        );
        assert_eq!(
            listening::load(&store).unwrap().revision,
            0,
            "browsing applied a scene"
        );
        process.send("\r");
        wait_until(
            &mut process,
            "Enter did not save the selected Dialogue scene",
            || listening::load(&store).unwrap().revision == 1,
        );
        let saved = listening::load(&store).unwrap();
        assert_eq!(saved.effective("OFFLINE PTY fixture").bass_db, -1.0);
        assert_eq!(saved.effective("OFFLINE PTY fixture").presence_db, 1.0);
        assert_eq!(store.load().unwrap().revision, 0, "scene rewrote global EQ");
        assert!(
            !dir.path().join("control.json").exists(),
            "preset changed routing"
        );
    }
}

#[test]
fn real_terminal_reopening_preset_allows_new_enter_but_not_held_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    for (id, revision, bass) in [("scene:focus", 1, -0.5), ("scene:dialogue", 2, -1.0)] {
        select_preset(&mut process, id);
        process.send("\r");
        wait_until(
            &mut process,
            "deliberate preset Enter was swallowed",
            || listening::load(&store).unwrap().revision == revision,
        );
        assert_eq!(
            listening::load(&store)
                .unwrap()
                .effective("OFFLINE PTY fixture")
                .bass_db,
            bass
        );
        process.send("\r\r\r");
        idle(&mut process, Duration::from_millis(350));
        assert_eq!(listening::load(&store).unwrap().revision, revision);
        assert!(!dir.path().join("control.json").exists());
    }
}

#[test]
fn real_terminal_reopening_preset_starts_on_the_last_applied_matching_layer() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());

    select_preset(&mut process, "scene:dialogue");
    process.send("\r");
    wait_until(&mut process, "Dialogue scene did not apply", || {
        listening::load(&store).unwrap().revision == 1
    });
    process.send("p");
    wait_for_preset(
        &mut process,
        Some("Dialogue"),
        Instant::now() + Duration::from_secs(5),
    );
    process.send("\x1b");
    idle(&mut process, Duration::from_millis(150));

    select_preset(&mut process, "warm");
    process.send("\r");
    wait_until(&mut process, "Warm EQ preset did not apply", || {
        store.load().unwrap().revision == 1
    });
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        1.0,
        "EQ preset must not erase the active listening scene"
    );
    process.send("p");
    wait_for_preset(
        &mut process,
        Some("warm"),
        Instant::now() + Duration::from_secs(5),
    );
}

#[test]
fn real_terminal_cancelled_preset_does_not_write_or_consume_undo() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    select_preset(&mut process, "scene:dialogue");
    process.send("\x1b");
    idle(&mut process, Duration::from_millis(350));
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    assert_eq!(store.load().unwrap().revision, 0);
    assert!(!dir.path().join("listening-previous.json").exists());
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_clicking_a_visible_preset_then_enter_applies_that_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    process.send("p");
    wait_for_preset(&mut process, None, Instant::now() + Duration::from_secs(5));
    let deadline = Instant::now() + Duration::from_secs(5);
    let dialogue_row = loop {
        let mut output = String::new();
        process.drain(&mut output);
        if let Some(row) = visible_preset_row(&process, "Dialogue") {
            break row;
        }
        assert!(
            Instant::now() < deadline,
            "Dialogue preset was not visible for pointer selection; screen: {}",
            process.terminal.screen().contents()
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    process.send(&click(Rect::new(35, dialogue_row, 1, 1)));
    idle(&mut process, Duration::from_millis(100));
    assert_eq!(
        listening::load(&store).unwrap().revision,
        0,
        "selection applied before Enter"
    );
    process.send("\r");
    wait_until(
        &mut process,
        "clicked preset row was ignored by Enter",
        || listening::load(&store).unwrap().revision == 1,
    );
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        1.0
    );
    assert_eq!(store.load().unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_preset_wheel_browses_without_applying() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    select_preset(&mut process, "scene:focus");
    idle(&mut process, Duration::from_millis(200));
    process.send("\x1b[<65;36;9M\x1b[<65;36;9M");
    idle(&mut process, Duration::from_millis(100));
    assert_eq!(listening::load(&store).unwrap().revision, 0);
    process.send("\r");
    wait_until(&mut process, "wheel-selected preset did not apply", || {
        listening::load(&store).unwrap().revision == 1
    });
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        1.0
    );
    assert_eq!(store.load().unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
}

#[test]
fn real_terminal_preset_header_preview_and_drag_cannot_retarget_selection() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path());
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start(store.clone());
    let mut process = spawn_console(dir.path());
    select_preset(&mut process, "scene:focus");
    idle(&mut process, Duration::from_millis(200));
    process.send(&click(Rect::new(35, 5, 1, 1)));
    process.send(&click(Rect::new(35, 28, 1, 1)));
    process.send("\x1b[<32;36;9M\x1b[<0;36;9m");
    process.send("\r");
    wait_until(&mut process, "original preset was not confirmed", || {
        listening::load(&store).unwrap().revision == 1
    });
    assert_eq!(
        listening::load(&store)
            .unwrap()
            .effective("OFFLINE PTY fixture")
            .presence_db,
        -0.25
    );
    assert_eq!(store.load().unwrap().revision, 0);
    assert!(!dir.path().join("control.json").exists());
}
