use maris::ui::desktop::status_icon::{self, Animation, Frame, Mode};
use serde_json::json;

#[test]
fn measured_levels_drive_the_mark_but_unavailable_and_reduced_motion_do_not() {
    let mut runtime = json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":-48.0});
    let quiet = Frame::read(&runtime, 20_010, false);
    runtime["peak_dbfs"] = json!(-6.0);
    let loud = Frame::read(&runtime, 20_010, false);
    assert_eq!(quiet.mode, Mode::Live);
    assert!(loud.level > quiet.level);
    assert_ne!(
        status_icon::rgba(quiet, true),
        status_icon::rgba(loud, true)
    );
    let reduced = Frame::read(&runtime, 20_010, true);
    runtime["peak_dbfs"] = json!(-48.0);
    assert_eq!(Frame::read(&runtime, 20_010, true), reduced);
    runtime.as_object_mut().unwrap().remove("peak_dbfs");
    assert_eq!(Frame::read(&runtime, 20_010, false).level, 0);
    assert_eq!(Frame::read(&runtime, 22_000, false).mode, Mode::Stale);
    assert_eq!(Frame::read(&runtime, 19_999, false).mode, Mode::Stale);
    runtime["active"] = json!(false);
    assert_eq!(Frame::read(&runtime, 20_010, false).mode, Mode::Idle);
    runtime["stale"] = json!(true);
    runtime["session_id"] = json!("previously-live");
    assert_eq!(Frame::read(&runtime, 20_010, false).mode, Mode::Stale);
    runtime.as_object_mut().unwrap().remove("session_id");
    assert_eq!(Frame::read(&runtime, 20_010, false).mode, Mode::Idle);
    runtime["stale"] = json!(false);
    runtime["active"] = json!(true);
    runtime["tonal_bypass"] = json!(true);
    assert_eq!(Frame::read(&runtime, 20_010, false).mode, Mode::Bypass);
}

#[test]
fn installed_idle_frame_does_not_issue_a_redundant_native_update() {
    let idle = Frame {
        mode: Mode::Idle,
        level: 0,
    };
    let mut animation = Animation::seeded(idle);
    assert!(animation
        .update(&json!({"active":false}), 20_000, false)
        .is_none());

    let live = json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":-6.0});
    assert!(animation.update(&live, 20_000, false).is_some());
}

#[test]
fn updates_are_bounded_and_static_states_allocate_no_more_frames() {
    let mut animation = Animation::default();
    let mut runtime = json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":-48.0});
    assert!(animation.update(&runtime, 20_000, false).is_some());
    runtime["peak_dbfs"] = json!(-6.0);
    for now in 20_001..20_125 {
        assert!(animation.update(&runtime, now, false).is_none());
    }
    assert!(animation.update(&runtime, 20_125, false).is_some());
    for now in 20_126..20_600 {
        assert!(animation.update(&runtime, now, false).is_none());
    }
    runtime["active"] = json!(false);
    assert!(animation.update(&runtime, 20_600, false).is_some());
    for now in 20_601..25_000 {
        assert!(animation.update(&runtime, now, false).is_none());
    }
}

#[test]
fn polling_only_uses_the_fast_hud_cadence_when_live_visible_and_motion_is_allowed() {
    let live = json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":-6.0});
    assert_eq!(
        status_icon::poll_interval_ms(&live, 20_010, false, false),
        125
    );
    assert_eq!(
        status_icon::poll_interval_ms(&live, 20_010, false, true),
        33
    );
    for visible in [false, true] {
        assert_eq!(
            status_icon::poll_interval_ms(&live, 20_010, true, visible),
            500
        );
        assert_eq!(
            status_icon::poll_interval_ms(&live, 22_000, false, visible),
            500
        );
        assert_eq!(
            status_icon::poll_interval_ms(&json!({"active":false}), 20_010, false, visible),
            500
        );
    }
    let mut bypass = live.clone();
    bypass["tonal_bypass"] = json!(true);
    assert_eq!(
        status_icon::poll_interval_ms(&bypass, 20_010, false, false),
        500
    );
    assert_eq!(
        status_icon::poll_interval_ms(&bypass, 20_010, false, true),
        33
    );
    let mut missing = live;
    missing.as_object_mut().unwrap().remove("peak_dbfs");
    assert_eq!(
        status_icon::poll_interval_ms(&missing, 20_010, false, false),
        500
    );
}

#[test]
fn retina_template_frames_have_a_bounded_mask_and_distinct_state_symbols() {
    let frames: Vec<_> = status_icon::frames().collect();
    assert_eq!(frames.len(), 20);
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame.cache_index(), index);
        let pixels = status_icon::rgba(*frame, true);
        assert_eq!(pixels.len(), status_icon::PIXELS.pow(2) * 4);
        assert!(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[..3] == [0, 0, 0]));
        assert!(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 255 || pixel[3] >= 150));
        for x in 0..status_icon::PIXELS {
            assert_eq!(pixels[x * 4 + 3], 0);
            assert_eq!(
                pixels[((status_icon::PIXELS - 1) * status_icon::PIXELS + x) * 4 + 3],
                0
            );
        }
    }
    let masks: Vec<_> = frames[17..]
        .iter()
        .map(|frame| status_icon::rgba(*frame, true))
        .collect();
    assert_ne!(masks[0], masks[1]);
    assert_ne!(masks[1], masks[2]);
}
