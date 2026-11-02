//! Real terminal input against isolated mixer state. The parent lease prevents
//! audio startup; synthetic heartbeats do not claim a running DSP or device.
use super::*;
use maris::mixer::{self, MixerStrip};

fn mixer_heartbeat(store: &Store) {
    let saved = mixer::load(store).unwrap();
    store.write_json("runtime.json", &json!({
        "active":true,"session_id":"offline-mixer-pty","profile_key":"OFFLINE PTY fixture",
        "output":"OFFLINE PTY fixture","sample_rate":48000,"music_processing":true,
        "updated_at_ms":maris::analysis::now_ms(),"rebind_count":0,"device_binding_revision":0,
        "device_identity":{"stable_id":"offline-pty"},"output_mode":"mixer_assignments",
        "system_backend":"multi_device_mixer","applied_revision":0,"applied_music_revision":0,
        "mixer":{"revision":saved.revision,"processing_revision":1,"restart_required":false,
            "strips":[],"buses":[]}
    })).unwrap();
}

fn setup() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    mixer::edit(&store, Some(0), |config| {
        config.strips.push(MixerStrip {
            id: "music".into(),
            name: "Music input".into(),
            source_device: Some("pid:4242".into()),
            ..MixerStrip::default()
        });
        config.buses[0].name = "Main speakers".into();
        config.buses[0].output_device = Some("uid:fixture-main".into());
        config.buses[1].name = "Monitor phones".into();
        config.buses[1].output_device = Some("uid:fixture-monitor".into());
        Ok(())
    })
    .unwrap();
    (directory, store)
}

fn wait_for(process: &mut ConsoleProcess, label: &str, done: impl Fn(&str) -> bool) {
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

fn no_other_changes(store: &Store) {
    for name in [
        "control.json",
        "profile.json",
        "listening.json",
        "listening-previous.json",
    ] {
        assert!(
            !store.directory.join(name).exists(),
            "unexpected write: {name}"
        );
    }
}

#[test]
fn real_terminal_bus_keyboard_gain_mute_and_undo_keep_routes_and_inputs() {
    let (_directory, store) = setup();
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start_with(store.clone(), mixer_heartbeat);
    let mut process = spawn_console(&store.directory);
    let before = mixer::load(&store).unwrap();
    process.send("m");
    wait_for(&mut process, "mixer opened", |screen| {
        screen.contains("Inputs and output buses")
    });
    process.send("\x1b[F"); // End selects bus B, not the last input.
    wait_for(&mut process, "bus selected", |screen| {
        screen.contains("Output: uid:fixture-monitor")
    });
    assert_eq!(
        mixer::load(&store).unwrap(),
        before,
        "navigation wrote state"
    );
    process.send("\x1b[C");
    wait_for(&mut process, "bus gain saved", |_| {
        mixer::load(&store).unwrap().revision == 2
    });
    wait_for(&mut process, "bus gain redrawn", |screen| {
        screen.contains("0.50 dB")
    });
    let gained = mixer::load(&store).unwrap();
    assert_eq!(gained.config.buses[1].gain_db, 0.5);
    assert_eq!(gained.config.strips, before.config.strips);
    assert_eq!(gained.config.buses[0], before.config.buses[0]);
    process.send(" ");
    wait_for(&mut process, "bus mute saved", |_| {
        mixer::load(&store).unwrap().revision == 3
    });
    wait_for(&mut process, "mute redrawn", |screen| {
        screen
            .lines()
            .any(|line| line.contains("Monitor phones") && line.contains("[x]"))
    });
    process.send("u");
    wait_for(&mut process, "mixer undo saved", |_| {
        mixer::load(&store).unwrap().revision == 4
    });
    let undone = mixer::load(&store).unwrap();
    assert_eq!(undone.config, gained.config);
    assert_eq!(undone.scenes, before.scenes);
    no_other_changes(&store);
}

#[test]
fn real_terminal_bus_pointer_selects_without_writing_then_adjusts_visible_button() {
    let (_directory, store) = setup();
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start_with(store.clone(), mixer_heartbeat);
    let mut process = spawn_console(&store.directory);
    let before = mixer::load(&store).unwrap();
    process.send("m");
    wait_for(&mut process, "mixer opened", |screen| {
        screen.contains("Inputs and output buses")
    });
    let area = Rect::new(0, 0, 140, 40);
    let work = maris::inspector::inset(
        maris::inspector::layout(studio_controls::shell(area).content).work,
    );
    let list = maris::inspector::app_rows(work);
    process.send(&click(Rect::new(list.x + 8, list.y + 1, 1, 1)));
    wait_for(&mut process, "bus A selected", |screen| {
        screen.contains("Output: uid:fixture-main")
    });
    assert_eq!(mixer::load(&store).unwrap(), before);
    process.send(&click(maris::inspector::command_rects(work)[2]));
    wait_for(&mut process, "bus A plus button", |_| {
        mixer::load(&store).unwrap().revision == 2
    });
    let after = mixer::load(&store).unwrap();
    assert_eq!(after.config.buses[0].gain_db, 0.5);
    assert_eq!(after.config.buses[1], before.config.buses[1]);
    assert_eq!(after.config.strips, before.config.strips);
    assert_eq!(
        after.config.buses[0].output_device,
        before.config.buses[0].output_device
    );
    assert_eq!(
        after.config.buses[0].delay_ms,
        before.config.buses[0].delay_ms
    );
    no_other_changes(&store);
}

#[test]
fn real_terminal_mixer_undo_keeps_bus_identity_after_input_count_changes() {
    let (_directory, store) = setup();
    let before = mixer::load(&store).unwrap();
    mixer::edit(&store, Some(before.revision), |config| {
        config.strips.push(MixerStrip {
            id: "temporary".into(),
            name: "Temporary input".into(),
            source_device: Some("pid:4343".into()),
            ..MixerStrip::default()
        });
        Ok(())
    })
    .unwrap();
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start_with(store.clone(), mixer_heartbeat);
    let mut process = spawn_console(&store.directory);
    process.send("m");
    wait_for(&mut process, "mixer opened", |screen| {
        screen.contains("Inputs and output buses")
    });
    process.send("\x1b[F");
    wait_for(&mut process, "bus B selected", |screen| {
        screen.contains("Output: uid:fixture-monitor")
    });
    process.send("u");
    wait_for(&mut process, "undo redrawn", |screen| {
        screen.contains("Previous mixer change undone")
    });
    assert_eq!(mixer::load(&store).unwrap().config, before.config);
    assert!(
        process
            .terminal
            .screen()
            .contents()
            .contains("Output: uid:fixture-monitor"),
        "undo lost the selected bus when the input list became shorter"
    );
    process.send("\x1b[C");
    wait_for(&mut process, "bus B gain saved", |_| {
        mixer::load(&store).unwrap().revision == 4
    });
    let after = mixer::load(&store).unwrap();
    let mut expected = before.config;
    expected.buses[1].gain_db = 0.5;
    assert_eq!(after.config, expected);
    no_other_changes(&store);
}

#[test]
fn real_terminal_mixer_undo_keeps_input_identity_after_row_changes() {
    let (_directory, store) = setup();
    let before = mixer::load(&store).unwrap();
    mixer::edit(&store, Some(before.revision), |config| {
        config.strips.insert(
            0,
            MixerStrip {
                id: "temporary".into(),
                name: "Temporary input".into(),
                source_device: Some("pid:4343".into()),
                ..MixerStrip::default()
            },
        );
        Ok(())
    })
    .unwrap();
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start_with(store.clone(), mixer_heartbeat);
    let mut process = spawn_console(&store.directory);
    process.send("m");
    wait_for(&mut process, "mixer opened", |screen| {
        screen.contains("Inputs and output buses")
    });
    process.send("\x1b[H\x1b[B");
    wait_for(&mut process, "music input selected", |screen| {
        screen.contains("Source: pid:4242")
    });
    process.send("u");
    wait_for(&mut process, "undo redrawn", |screen| {
        screen.contains("Previous mixer change undone")
    });
    assert_eq!(mixer::load(&store).unwrap().config, before.config);
    assert!(
        process
            .terminal
            .screen()
            .contents()
            .contains("Source: pid:4242"),
        "undo lost the selected input when its row changed"
    );
    process.send("\x1b[C");
    wait_for(&mut process, "music input gain saved", |_| {
        mixer::load(&store).unwrap().revision == 4
    });
    let after = mixer::load(&store).unwrap();
    let mut expected = before.config;
    expected.strips[0].gain_db = 0.5;
    assert_eq!(after.config, expected);
    no_other_changes(&store);
}

#[test]
fn real_terminal_mixer_undo_removal_cannot_retarget_an_output_bus() {
    let (_directory, store) = setup();
    let before = mixer::load(&store).unwrap();
    mixer::edit(&store, Some(before.revision), |config| {
        config.strips.push(MixerStrip {
            id: "temporary".into(),
            name: "Temporary input".into(),
            source_device: Some("pid:4343".into()),
            ..MixerStrip::default()
        });
        Ok(())
    })
    .unwrap();
    let _lease = store.session_lock().unwrap();
    let _heartbeat = Heartbeat::start_with(store.clone(), mixer_heartbeat);
    let mut process = spawn_console(&store.directory);
    process.send("m");
    wait_for(&mut process, "mixer opened", |screen| {
        screen.contains("Inputs and output buses")
    });
    process.send("\x1b[H\x1b[B");
    wait_for(&mut process, "temporary input selected", |screen| {
        screen.contains("Source: pid:4343")
    });
    process.send("u");
    wait_for(&mut process, "undo redrawn", |screen| {
        screen.contains("Previous mixer change undone")
    });
    let undone = mixer::load(&store).unwrap();
    assert_eq!(undone.config, before.config);
    process.send("\x1b[C");
    wait_for(&mut process, "removed input rejected", |screen| {
        screen.contains("Choose a mixer channel first") || mixer::load(&store).unwrap() != undone
    });
    assert_eq!(
        mixer::load(&store).unwrap(),
        undone,
        "a removed input must not silently become an output bus"
    );
    assert!(process
        .terminal
        .screen()
        .contents()
        .contains("Choose a mixer channel first"));
    no_other_changes(&store);
}
