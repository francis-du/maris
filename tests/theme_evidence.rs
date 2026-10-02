use maris::theme;
use serde_json::{json, Value};

fn semantic() -> Value {
    json!({"active":true,"music_context":{
        "mode":"semantic","confidence":0.9,"updated_at_ms":20_000,
        "genre":[{"label":"jazz","confidence":0.6},{"label":"electronic","confidence":0.9}]
    }})
}

#[test]
fn semantic_theme_needs_explicit_mode_confidence_freshness_and_active_audio() {
    let valid = semantic();
    assert_eq!(theme::semantic_label(&valid, 21_000), Some("electronic"));
    assert!(theme::semantic_label(&valid, 40_001).is_none());
    assert!(theme::semantic_label(&valid, 19_999).is_none());
    let mut stopped = valid.clone();
    stopped["active"] = json!(false);
    assert!(theme::semantic_label(&stopped, 21_000).is_none());
    assert_eq!(theme::palette(&stopped).source, "default");
    let mut signal_only = valid.clone();
    signal_only["music_context"]["mode"] = json!("signal_only");
    assert!(theme::semantic_label(&signal_only, 21_000).is_none());
    let mut uncertain = valid.clone();
    uncertain["music_context"]["confidence"] = json!(0.1);
    assert!(theme::semantic_label(&uncertain, 21_000).is_none());
    let legacy_guess = json!({"active":true,"music_context":{"top_style":"electronic"}});
    assert!(theme::semantic_label(&legacy_guess, 21_000).is_none());
}

#[test]
fn inactive_signal_metrics_do_not_select_a_fake_live_theme() {
    let runtime = json!({"active":false,"analysis":{
        "band_energy_share":[0.8,0.1,0.1,0,0,0,0,0,0,0],
        "rms_dbfs":-8.0
    }});
    assert_eq!(theme::palette(&runtime).source, "default");
}
