use maris::neural;

#[test]
fn product_inventory_contains_only_shipped_backends_and_no_model_download_requirement() {
    let inventory = neural::models();
    let models = inventory["models"].as_array().unwrap();
    assert_eq!(models[0]["compiled"], cfg!(feature = "neural"));
    assert_eq!(models[0]["sample_rate"], 48000);
    assert_eq!(models[0]["usable"], cfg!(feature = "neural"));
    assert_eq!(
        models[0]["status"],
        if cfg!(feature = "neural") {
            "integrated"
        } else {
            "unavailable"
        }
    );
    for model in models {
        assert!(model["downloadable"].is_boolean());
        assert!(model["usable"].is_boolean());
        if model["status"] == "candidate" || model["status"] == "researched" {
            assert_eq!(model["usable"], false);
        }
    }
    assert_eq!(models.len(), 3);
    assert_eq!(models[1]["id"], "signal-context");
    assert_eq!(models[1]["kind"], "signal_analysis");
    assert_eq!(models[1]["inference_available"], false);
    assert_eq!(models[0]["activation"], "explicit_speech_only");
    assert_eq!(models[0]["music_safe_default"], false);
    assert_eq!(models[0]["bundled"], cfg!(feature = "neural"));
    assert_eq!(inventory["scope"], "shipped_backends");
    assert_eq!(inventory["music_requires_model_download"], false);
    assert!(models.iter().all(|model| model["downloadable"] == false));
    assert!(models
        .iter()
        .all(|model| model["audio_callback_thread"] != true));
    assert!(models.iter().all(|model| {
        matches!(
            model["status"].as_str(),
            Some("integrated" | "candidate" | "researched" | "unavailable")
        )
    }));
    for removed in ["clap-music", "deepafx", "deepfilternet", "yamnet"] {
        assert!(models.iter().all(|model| model["id"] != removed));
    }
    let signal = models
        .iter()
        .find(|model| model["id"] == "signal-context")
        .unwrap();
    assert_eq!(signal["status"], "integrated");
    assert_eq!(signal["semantic"], false);
    let musicnn = models
        .iter()
        .find(|model| model["id"] == "musicnn")
        .unwrap();
    assert_eq!(musicnn["kind"], "semantic_model");
    assert_eq!(musicnn["downloadable"], false);
    assert_eq!(musicnn["inference_available"], musicnn["bundled"]);
    assert_eq!(inventory["automatic_downloads"], false);
}

#[test]
fn research_inventory_reports_build_bundled_candidate_without_runtime_download() {
    let directory = tempfile::tempdir().unwrap();
    let store = maris::store::Store::at(directory.path());
    let product = neural::models_at(&store).unwrap();
    assert_eq!(product["models"].as_array().unwrap().len(), 3);
    let inventory = neural::research_models_at(&store).unwrap();
    assert_eq!(inventory["scope"], "model_provenance");
    assert_eq!(inventory["models"].as_array().unwrap().len(), 1);
    let musicnn = inventory["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["id"] == "musicnn")
        .unwrap();
    assert!(matches!(
        musicnn["status"].as_str(),
        Some("integrated" | "build_resource_absent")
    ));
    assert_eq!(musicnn["inference_available"], musicnn["bundled"]);
    assert_eq!(musicnn["downloadable"], false);
    assert_eq!(
        musicnn["weights_sha256"],
        "cc0b9400fcaed6e9ce7fbcfa97ec91e4fcb5f2ab34ca3a0cd6bef4af74753e1a"
    );
    assert_eq!(musicnn["sample_rate"], 16000);
    assert_eq!(musicnn["mel_bands"], 96);
    assert!(!directory.path().join("models").exists());
}

#[cfg(feature = "neural")]
#[test]
fn neural_inference_is_finite_and_bounded() {
    let mut model = neural::VoiceModel::new();
    let mut output = [[0.0_f32; 2]; 480];
    let silence = [[0.0_f32; 2]; 480];
    for _ in 0..10 {
        let probability = model.process(&silence, &mut output);
        assert!(probability.is_finite() && (0.0..=1.0).contains(&probability));
        assert!(output
            .iter()
            .flatten()
            .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
    }
}
#[cfg(feature = "neural")]
#[test]
fn offline_neural_processing_preserves_length_and_refuses_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("speech.wav");
    let output = directory.path().join("clean.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&input, spec).unwrap();
    for _ in 0..2346 {
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
    let report = neural::enhance(&input, &output).unwrap();
    assert_eq!(report["frames"], 1173);
    let reader = hound::WavReader::open(&output).unwrap();
    assert_eq!(reader.len(), 2346);
    assert!(reader.into_samples::<f32>().all(|v| v.unwrap().is_finite()));
    assert!(neural::enhance(&input, &output).is_err());
}
#[cfg(feature = "neural")]
#[test]
fn unsupported_neural_sample_rate_does_not_create_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("speech.wav");
    let output = directory.path().join("clean.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&input, spec).unwrap();
    writer.write_sample(0_i16).unwrap();
    writer.finalize().unwrap();
    assert!(neural::enhance(&input, &output).is_err());
    assert!(!output.exists());
}
#[cfg(not(feature = "neural"))]
#[test]
fn missing_backend_fails_without_opening_files() {
    assert!(neural::enhance(
        std::path::Path::new("missing.wav"),
        std::path::Path::new("not-created.wav")
    )
    .is_err());
}
