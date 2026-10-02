//! End-to-end preset regressions: real PTY bytes and persisted state, not synthetic key reducers.
use super::*;

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

fn select_preset(process: &mut ConsoleProcess, id: &str) {
    let catalog = maris::presets::console_catalog();
    let selected = catalog
        .iter()
        .position(|p| p.id == id)
        .expect("known preset");
    // Start at the first visible item regardless of the current profile's selection.
    // Do not call the preset implementation or synthesize internal KeyEvents.
    process.send(&format!(
        "p{}{}",
        "\x1b[A".repeat(catalog.len()),
        "\x1b[B".repeat(selected)
    ));
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
    idle(&mut process, Duration::from_millis(200));
    // Independent fixed-size UI contract: at 140x40 the popup starts at (28,4),
    // its header is y=5, and Dialogue is the third data row, y=8.
    process.send(&click(Rect::new(35, 8, 1, 1)));
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
