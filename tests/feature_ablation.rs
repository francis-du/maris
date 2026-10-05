//! Ablation uses real DSP output rather than counting UI toggles or inventing listening benefit.
#[path = "support/ablation.rs"]
mod ablation;

#[test]
fn each_retained_effect_has_a_distinct_conditioned_contribution() {
    let rates = [44_100, 48_000, 96_000];
    let report = ablation::report(&rates).unwrap();
    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 27 * rates.len());
    for variant in [
        "without_correction",
        "without_preference_tone",
        "without_dynamic_eq",
        "without_virtual_bass",
        "without_compression",
        "without_stereo_width",
        "without_profile_eq",
        "without_crossfeed",
    ] {
        assert!(results.iter().filter(|row| row["variant"] == variant)
            .any(|row| row["level_matched_difference_rms"].as_f64().unwrap() > 1e-6),
            "No measurable conditioned contribution for {variant}; review rather than silently retain it");
    }
    for row in results.iter().filter(|row| row["variant"] == "reference") {
        assert!(row["level_matched_difference_rms"].as_f64().unwrap() < 1e-12);
    }
    // A stereo-width effect has no reason to modify center-only mono content.
    for rate in rates {
        let mono = results
            .iter()
            .find(|row| {
                row["sample_rate"] == rate
                    && row["variant"] == "without_stereo_width"
                    && row["fixture"] == "bass_dominant_mono"
            })
            .unwrap();
        assert!(
            mono["level_matched_difference_rms"].as_f64().unwrap() < 1e-8,
            "stereo width changed mono content at {rate} Hz"
        );
    }
    assert_eq!(report["subjective_quality_validated"], false);
}
