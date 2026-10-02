use super::*;

fn analysis() -> crate::analysis::Analysis {
    crate::analysis::Analysis {
        sample_rate: 48_000,
        frames: 96_000,
        rms_dbfs: -24.0,
        peak_dbfs: -6.0,
        momentary_lufs: Some(-22.0),
        shortterm_lufs: None,
        integrated_lufs: None,
        true_peak_dbtp: Some(-5.5),
        crest_db: 18.0,
        clipped_fraction: 0.0,
        band_energy_share: [0.1; 10],
        stereo_correlation: 1.0,
        dropped_frames: 0,
        updated_at_ms: crate::analysis::now_ms(),
        stage: "pre_eq".into(),
    }
}

#[test]
fn tag_mapping_is_bounded_and_uses_only_reviewed_labels() {
    let mut scores = vec![0.01_f32; LABELS.len()];
    scores[LABELS.iter().position(|label| *label == "rock").unwrap()] = 0.91;
    scores[LABELS.iter().position(|label| *label == "guitar").unwrap()] = 0.82;
    scores[LABELS.iter().position(|label| *label == "vocal").unwrap()] = 0.76;
    let window = Window {
        rate: 48_000,
        frames: vec![[0.0, 0.0]; 100],
        analysis: analysis(),
    };
    let context = context_from_probabilities(&scores, &window).unwrap();
    assert_eq!(context.mode, "semantic");
    assert_eq!(context.model, "musicnn-mtt");
    assert_eq!(context.genre[0].label, "rock");
    assert_eq!(context.instruments[0].label, "guitar");
    assert!(context.vocal_probability.unwrap() > 0.7);
    assert!(context.validate().is_ok());
}

#[cfg(not(maris_musicnn_bundled))]
#[test]
fn absent_checkpoint_is_explicitly_unavailable_not_a_successful_inference() {
    assert!(crate::models::musicnn_weights().is_none());
    assert!(crate::models::music_context_backend().unwrap().is_none());
    assert!(MusicNn::load(&[]).is_err());
}

#[cfg(maris_musicnn_bundled)]
#[test]
fn bundled_checkpoint_executes_end_to_end() {
    let bytes =
        crate::models::musicnn_weights().expect("bundled build must supply verified weights");
    let mut model = MusicNn::load(bytes).unwrap();
    let frames: Vec<[f32; 2]> = (0..48_000 * 3)
        .map(|i| {
            let value = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.05;
            [value, value]
        })
        .collect();
    let window = Window {
        rate: 48_000,
        frames,
        analysis: analysis(),
    };
    let window = Window {
        analysis: crate::analysis::measure(&window.frames, window.rate).unwrap(),
        ..window
    };
    let context = model.infer(&window).unwrap();
    assert_eq!(context.mode, "semantic");
    assert_eq!(context.model, "musicnn-mtt");
    assert!(context.confidence.is_finite());
    assert!(context.validate().is_ok());
}

#[test]
fn invalid_probabilities_cannot_be_laundered_into_semantic_evidence() {
    let window = Window {
        rate: 48_000,
        frames: vec![[0.0; 2]; 1],
        analysis: analysis(),
    };
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
        for index in 0..LABELS.len() {
            let mut probabilities = [0.5; LABELS.len()];
            probabilities[index] = bad;
            assert!(
                context_from_probabilities(&probabilities, &window).is_err(),
                "invalid score {bad:?} at {index} was reported as a valid semantic result"
            );
        }
    }
}

#[test]
fn recognition_input_requires_real_complete_audio_with_paired_statistics() {
    for seconds in [1, 2, 3, 4] {
        let frames = vec![[0.01; 2]; 48_000 * seconds];
        let mut window = Window {
            rate: 48_000,
            analysis: crate::analysis::measure(&frames, 48_000).unwrap(),
            frames,
        };
        assert_eq!(prepare_window(&window).is_ok(), seconds == 3);
        if seconds == 3 {
            window.analysis.frames -= 1;
            assert!(prepare_window(&window).is_err());
            window.analysis.frames += 1;
            window.analysis.dropped_frames = 1;
            assert!(prepare_window(&window).is_err());
            window.analysis.dropped_frames = 0;
            window.frames[0][0] = f32::NAN;
            assert!(prepare_window(&window).is_err());
        }
    }
}

#[test]
fn malformed_model_output_is_rejected() {
    let window = Window {
        rate: 48_000,
        frames: vec![[0.0, 0.0]; 1],
        analysis: analysis(),
    };
    assert!(context_from_probabilities(&[0.5], &window).is_err());
}
