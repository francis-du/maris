use maris::{analysis, music_context};

fn tone(hz: f64, rate: u32, seconds: usize) -> Vec<[f32; 2]> {
    (0..rate as usize * seconds)
        .map(|index| {
            let value =
                (std::f64::consts::TAU * hz * index as f64 / rate as f64).sin() as f32 * 0.1;
            [value, value]
        })
        .collect()
}

#[test]
fn signal_only_context_never_invents_semantics() {
    let frames = tone(100.0, 48_000, 2);
    let evidence = analysis::measure(&frames, 48_000).unwrap();
    let context = music_context::MusicContext::signal_only(&evidence);
    context.validate().unwrap();
    assert_eq!(context.mode, "signal_only");
    assert_eq!(context.confidence, 0.0);
    assert!(context.genre.is_empty());
    assert!(context.instruments.is_empty());
    assert!(context.vocal_probability.is_none());
    assert!(context.instrumental_probability.is_none());
    assert!(context.signal.bass_energy > 0.9);
}

#[test]
fn configured_music_context_worker_publishes_its_actual_backend() {
    let frames = tone(1000.0, 48_000, 3);
    let evidence = analysis::measure(&frames, 48_000).unwrap();
    let worker = music_context::Worker::start().unwrap();
    let backend = worker.status();
    worker.submit(music_context::Window {
        rate: 48_000,
        frames,
        analysis: evidence,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while worker.latest().is_none() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let context = worker.latest().expect("configured worker did not publish");
    assert_eq!(context.model, backend["backend"].as_str().unwrap());
    assert_eq!(
        context.mode,
        if backend["semantic_backend_available"] == true {
            "semantic"
        } else {
            "signal_only"
        }
    );
    assert_eq!(worker.status()["failed_windows"], 0);
    assert!(context.inference_ms >= 0.0);
}

#[test]
fn signal_only_validation_rejects_semantic_claims() {
    let frames = tone(1000.0, 48_000, 2);
    let evidence = analysis::measure(&frames, 48_000).unwrap();
    let mut context = music_context::MusicContext::signal_only(&evidence);
    context.genre.push(music_context::Tag {
        label: "rock".into(),
        confidence: 0.9,
    });
    assert!(context.validate().is_err());
}

struct FixtureSemantic;
impl music_context::SemanticBackend for FixtureSemantic {
    fn id(&self) -> &'static str {
        "fixture-semantic"
    }

    fn infer(
        &mut self,
        input: &music_context::Window,
    ) -> anyhow::Result<music_context::MusicContext> {
        let mut context = music_context::MusicContext::signal_only(&input.analysis);
        context.mode = "semantic".into();
        context.model = self.id().into();
        context.confidence = 0.8;
        context.genre.push(music_context::Tag {
            label: "electronic".into(),
            confidence: 0.8,
        });
        Ok(context)
    }
}

#[test]
fn pluggable_semantic_backend_is_reported_truthfully() {
    let frames = tone(1000.0, 48_000, 2);
    let evidence = analysis::measure(&frames, 48_000).unwrap();
    let worker = music_context::Worker::start_with_backend(Box::new(FixtureSemantic)).unwrap();
    worker.submit(music_context::Window {
        rate: 48_000,
        frames,
        analysis: evidence,
    });
    for _ in 0..100 {
        if worker.latest().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let context = worker.latest().unwrap();
    assert_eq!(context.mode, "semantic");
    assert_eq!(context.top_genre(), Some("electronic"));
    let status = worker.status();
    assert_eq!(status["backend"], "fixture-semantic");
    assert_eq!(status["semantic_backend_available"], true);
    assert_eq!(status["raw_audio_uploads"], false);
}

#[test]
fn semantic_worker_rate_limits_windows_and_reset_clears_context() {
    let frames = tone(1000.0, 48_000, 2);
    let mut evidence = analysis::measure(&frames, 48_000).unwrap();
    evidence.updated_at_ms = 10_000;
    let worker = music_context::Worker::start_with_backend(Box::new(FixtureSemantic)).unwrap();
    worker.submit(music_context::Window {
        rate: 48_000,
        frames: frames.clone(),
        analysis: evidence.clone(),
    });
    for _ in 0..100 {
        if worker.latest().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(worker.latest().is_some());
    let mut too_soon = evidence.clone();
    too_soon.updated_at_ms += 1_000;
    worker.submit(music_context::Window {
        rate: 48_000,
        frames: frames.clone(),
        analysis: too_soon,
    });
    std::thread::sleep(std::time::Duration::from_millis(30));
    assert_eq!(worker.status()["skipped_interval_windows"], 1);
    assert_eq!(worker.status()["minimum_interval_ms"], 3_000);

    worker.reset();
    assert!(worker.latest().is_none());
    let mut after_reset = evidence;
    after_reset.updated_at_ms += 1_100;
    worker.submit(music_context::Window {
        rate: 48_000,
        frames,
        analysis: after_reset,
    });
    for _ in 0..100 {
        if worker.latest().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        worker.latest().is_some(),
        "reset must permit immediate context for a new device"
    );
}

#[test]
fn public_resampler_uses_the_model_filter_and_keeps_its_duration_contract() {
    let output = music_context::resample_mono_16k(&tone(10_000.0, 48_000, 1), 48_000).unwrap();
    let steady = &output[128..output.len() - 128];
    assert!(steady.iter().all(|sample| sample.abs() < 0.001));
    assert!(music_context::resample_mono_16k(&[], 48_000)
        .unwrap()
        .is_empty());
    assert!(music_context::resample_mono_16k(&[[0.0; 2]], 48_000)
        .unwrap()
        .is_empty());
    assert_eq!(
        music_context::resample_mono_16k(&[[0.0; 2]; 4], 44_100)
            .unwrap()
            .len(),
        1
    );
    assert!(music_context::resample_mono_16k(&[], 0).is_err());
}

#[test]
fn worker_only_resampler_produces_finite_16khz_mono() {
    let frames = tone(997.0, 48_000, 3);
    let resampled = music_context::resample_mono_16k(&frames, 48_000).unwrap();
    assert_eq!(resampled.len(), 48_000);
    assert!(resampled
        .iter()
        .all(|sample| sample.is_finite() && sample.abs() <= 0.11));
}
