use maris::{
    tui_view,
    visualizer::{Analyzer, Motion},
};
use ratatui::{layout::Rect, text::Line};
use serde_json::json;
#[test]
fn silence_has_no_spectrum() {
    let mut analyzer = Analyzer::new(48000);
    let mut last = None;
    for _ in 0..12000 {
        if let Some(frame) = analyzer.push([0.0; 2]) {
            last = Some(frame);
        }
    }
    assert!(last.unwrap().bands_dbfs.iter().all(|db| *db <= -119.0));
}
#[test]
fn stale_telemetry_stops_animation() {
    let r = json!({"active":true,"updated_at_ms":1000,"peak_left_dbfs":-6.0,"peak_right_dbfs":-12.0,"visualization":{"updated_at_ms":1000,"bands_dbfs":vec![-12.0;24]}});
    let mut m = Motion::default();
    for _ in 0..30 {
        m.update(&r, 0.033, 1000);
    }
    assert!(m.bands[0] > 0.8);
    for _ in 0..100 {
        m.update(&r, 0.033, 2000);
    }
    assert!(m.bands.iter().chain(m.levels.iter()).all(|v| *v < 0.0001));
}
#[test]
fn music_linked_theme_uses_semantics_only_when_semantics_exist() {
    let now = maris::analysis::now_ms();
    let _clock = maris::analysis::DebugClock::freeze_at(now);
    let electronic = json!({"active":true,"music_context":{"mode":"semantic","updated_at_ms":maris::analysis::now_ms(),"confidence":0.9,"genre":[{"label":"electronic","confidence":0.9}]}});
    let semantic = maris::theme::palette(&electronic);
    assert_eq!(semantic.name, "Neon Drive");
    assert_eq!(semantic.source, "semantic");

    let bass_signal = json!({
        "active":true,
        "analysis":{
            "band_energy_share":[0.30,0.20,0.10,0.08,0.08,0.06,0.05,0.04,0.04,0.05],
            "crest_db":8.0,
            "stereo_correlation":0.8
        }
    });
    let signal = maris::theme::palette(&bass_signal);
    assert_eq!(signal.name, "Lime Bass");
    assert_eq!(signal.source, "signal");

    let idle = maris::theme::palette(&json!({"active":false}));
    assert_eq!(idle.name, "Violet Pulse");
    assert_eq!(idle.source, "default");
}

#[test]
fn theme_tracker_debounces_and_smoothly_adopts_stable_signal_context() {
    let bass_signal = json!({
        "active": true,
        "analysis": {
            "rms_dbfs": -22.0,
            "band_energy_share": [0.30,0.20,0.10,0.08,0.08,0.06,0.05,0.04,0.04,0.05],
            "crest_db": 8.0,
            "stereo_correlation": 0.8
        }
    });
    let bright_signal = json!({
        "active": true,
        "analysis": {
            "rms_dbfs": -22.0,
            "band_energy_share": [0.10,0.08,0.07,0.08,0.08,0.08,0.08,0.16,0.14,0.13],
            "crest_db": 8.0,
            "stereo_correlation": 0.8
        }
    });
    let mut tracker = maris::theme::Tracker::default();
    for index in 0..10 {
        let runtime = if index % 2 == 0 {
            &bass_signal
        } else {
            &bright_signal
        };
        let current = tracker.update(runtime, 0.2);
        assert_eq!(
            current.source, "default",
            "rapid theme changes must be debounced"
        );
    }
    let mut current = maris::theme::VIOLET;
    for _ in 0..12 {
        current = tracker.update(&bass_signal, 0.2);
    }
    assert_eq!(current.name, "Lime Bass");
    assert_eq!(current.source, "signal");
}

#[test]
fn semantic_theme_requires_stable_context_before_switching() {
    let now = maris::analysis::now_ms();
    let _clock = maris::analysis::DebugClock::freeze_at(now);
    let semantic = json!({
        "active": true,
        "music_context": {
            "mode": "semantic",
            "updated_at_ms": maris::analysis::now_ms(),
            "genre": [{"label": "electronic", "confidence": 0.9}],
            "confidence": 0.9,
            "model": "test"
        }
    });
    let mut tracker = maris::theme::Tracker::default();
    for _ in 0..4 {
        assert_eq!(tracker.update(&semantic, 0.25).source, "default");
    }
    let mut current = maris::theme::VIOLET;
    for _ in 0..8 {
        current = tracker.update(&semantic, 0.25);
    }
    assert_eq!(current.name, "Neon Drive");
    assert_eq!(current.source, "semantic");
}

#[test]
fn popup_fits_content() {
    let r = tui_view::popup(
        Rect::new(0, 0, 120, 40),
        &[Line::from("Ready"), Line::from("Y confirms; Esc cancels")],
    );
    assert!(r.width <= 66 && r.height <= 7);
}
