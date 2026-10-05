//! Ablation uses real DSP output rather than counting UI toggles or inventing listening benefit.
#[path = "support/ablation.rs"]
mod ablation;

#[test]
fn each_retained_effect_has_a_distinct_conditioned_contribution() {
    let rates = [44_100, 48_000, 96_000, 192_000];
    let report = ablation::report(&rates).unwrap();
    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 33 * rates.len());
    for rate in rates {
        for variant in [
            "without_correction",
            "without_preference_tone",
            "without_dynamic_eq",
            "without_virtual_bass",
            "without_compression",
            "without_stereo_width",
            "without_virtual_surround",
            "without_stereo_focus",
            "without_profile_eq",
            "without_crossfeed",
        ] {
            assert!(
                results
                    .iter()
                    .filter(|row| row["sample_rate"] == rate && row["variant"] == variant)
                    .any(|row| row["level_matched_difference_rms"].as_f64().unwrap() > 1e-6),
                "No measurable conditioned contribution for {variant} at {rate} Hz; review rather than silently retain it"
            );
        }
    }
    for row in results.iter().filter(|row| row["variant"] == "reference") {
        assert!(row["level_matched_difference_rms"].as_f64().unwrap() < 1e-12);
    }
    // Spatial effects and crossfeed should preserve center-only mono content.
    for rate in rates {
        for variant in [
            "without_stereo_width",
            "without_virtual_surround",
            "without_stereo_focus",
            "without_crossfeed",
        ] {
            for fixture in ["bass_dominant_mono", "treble_pulses_mono"] {
                let mono = results
                    .iter()
                    .find(|row| {
                        row["sample_rate"] == rate
                            && row["variant"] == variant
                            && row["fixture"] == fixture
                    })
                    .unwrap();
                assert!(
                    mono["level_matched_difference_rms"].as_f64().unwrap() < 1e-8,
                    "{variant} changed mono content at {rate} Hz on {fixture}"
                );
            }
        }
    }
    for rate in rates {
        let high = results
            .iter()
            .find(|row| {
                row["sample_rate"] == rate
                    && row["variant"] == "without_virtual_bass"
                    && row["fixture"] == "treble_pulses_mono"
            })
            .unwrap();
        assert!(
            high["level_matched_difference_rms"].as_f64().unwrap() < 1e-5,
            "virtual bass leaked into treble-only content at {rate} Hz"
        );
        let bass = results
            .iter()
            .find(|row| {
                row["sample_rate"] == rate
                    && row["variant"] == "without_virtual_bass"
                    && row["fixture"] == "bass_dominant_mono"
            })
            .unwrap();
        assert!(
            bass["level_matched_difference_rms"].as_f64().unwrap() > 1e-3,
            "virtual bass lost its intended low-frequency contribution at {rate} Hz"
        );
    }
    assert_eq!(report["subjective_quality_validated"], false);
}
