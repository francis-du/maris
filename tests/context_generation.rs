use maris::{
    analysis,
    music_context::{MusicContext, SemanticBackend, Tag, Window, Worker},
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn window(time: u64) -> Window {
    let frames = vec![[0.01_f32; 2]; 512];
    let mut analysis = analysis::measure(&frames, 48_000).unwrap();
    analysis.updated_at_ms = time;
    Window {
        rate: 48_000,
        frames,
        analysis,
    }
}

struct GatedBackend {
    entered: mpsc::SyncSender<u64>,
    release: mpsc::Receiver<()>,
}
impl SemanticBackend for GatedBackend {
    fn id(&self) -> &'static str {
        "gated-fixture"
    }
    fn infer(&mut self, input: &Window) -> anyhow::Result<MusicContext> {
        self.entered.send(input.analysis.updated_at_ms)?;
        self.release.recv_timeout(Duration::from_secs(2))?;
        let mut context = MusicContext::signal_only(&input.analysis);
        context.mode = "semantic".into();
        context.confidence = 0.8;
        context.genre.push(Tag {
            label: "fixture".into(),
            confidence: 0.8,
        });
        Ok(context)
    }
}

#[test]
fn reset_rejects_in_flight_publication_and_replaces_queued_old_device_windows() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(4);
    let (release_tx, release_rx) = mpsc::sync_channel(4);
    let worker = Worker::start_with_backend(Box::new(GatedBackend {
        entered: entered_tx,
        release: release_rx,
    }))
    .unwrap();
    worker.submit(window(10_000));
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        10_000
    );
    worker.submit(window(14_000));
    worker.reset();
    worker.submit(window(15_000));
    assert!(worker.latest().is_none());
    release_tx.send(()).unwrap();
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        15_000
    );
    // The old inference has finished and the new one is held at the gate. The old
    // semantic result must not reappear, even briefly, after reset.
    assert!(worker.latest().is_none());
    assert_eq!(worker.status()["discarded_generation_windows"], 1);
    release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while worker.latest().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let context = worker.latest().unwrap();
    assert_eq!(context.updated_at_ms, 15_000);
    assert_eq!(context.model, "gated-fixture");
    assert_eq!(worker.status()["processed_windows"], 2);
}

#[test]
fn older_analysis_cannot_replace_a_newer_queued_window() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(4);
    let (release_tx, release_rx) = mpsc::sync_channel(4);
    let worker = Worker::start_with_backend(Box::new(GatedBackend {
        entered: entered_tx,
        release: release_rx,
    }))
    .unwrap();
    worker.submit(window(10_000));
    assert_eq!(
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        10_000
    );
    worker.submit(window(14_000));
    worker.submit(window(9_000));
    release_tx.send(()).unwrap();
    let next = entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    // Release before asserting so a failure cannot strand the test worker.
    release_tx.send(()).unwrap();
    assert_eq!(next, 14_000, "an older observation replaced the latest one");
    assert_eq!(worker.status()["dropped_windows"], 1);

    // A reset starts a new device/session history, so its first timestamp may be lower.
    worker.reset();
    worker.submit(window(8_000));
    let next = entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    release_tx.send(()).unwrap();
    assert_eq!(next, 8_000);
}

struct MisreportedMeasurements;
impl SemanticBackend for MisreportedMeasurements {
    fn id(&self) -> &'static str {
        "misreported-measurements-fixture"
    }
    fn infer(&mut self, input: &Window) -> anyhow::Result<MusicContext> {
        let mut context = MusicContext::signal_only(&input.analysis);
        context.mode = "semantic".into();
        context.model = "unrelated-model".into();
        context.updated_at_ms = u64::MAX;
        context.confidence = 0.8;
        context.genre.push(Tag {
            label: "fixture".into(),
            confidence: 0.8,
        });
        context.signal.bass_energy = 1.0;
        context.signal.treble_energy = 1.0;
        context.signal.rms_dbfs = -1.0;
        context.signal.crest_db = 0.0;
        context.signal.stereo_correlation = -1.0;
        context.signal.momentary_lufs = Some(-1.0);
        Ok(context)
    }
}

#[test]
fn recognition_tags_cannot_replace_the_captured_signal_measurements() {
    let input = window(10_000);
    let measured = MusicContext::signal_only(&input.analysis).signal;
    let worker = Worker::start_with_backend(Box::new(MisreportedMeasurements)).unwrap();
    worker.submit(input);
    let deadline = Instant::now() + Duration::from_secs(2);
    let context = loop {
        if let Some(context) = worker.latest() {
            break context;
        }
        assert!(
            Instant::now() < deadline,
            "recognition worker did not publish"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert_eq!(context.signal, measured);
    assert_eq!(context.model, "misreported-measurements-fixture");
    assert_eq!(context.updated_at_ms, 10_000);
    assert_eq!(context.genre[0].label, "fixture");
    assert_eq!(worker.status()["failed_windows"], 0);
}

struct FailedBackend;
impl SemanticBackend for FailedBackend {
    fn id(&self) -> &'static str {
        "failed-fixture"
    }
    fn infer(&mut self, _: &Window) -> anyhow::Result<MusicContext> {
        anyhow::bail!("fixture inference failed")
    }
}

#[test]
fn failed_inference_publishes_signal_only_fallback_and_explicit_error() {
    let worker = Worker::start_with_backend(Box::new(FailedBackend)).unwrap();
    worker.submit(window(10_000));
    let deadline = Instant::now() + Duration::from_secs(2);
    while worker.latest().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let context = worker.latest().unwrap();
    assert_eq!(context.mode, "signal_only");
    assert!(context.genre.is_empty());
    assert!(context.vocal_probability.is_none());
    let status = worker.status();
    assert_eq!(status["failed_windows"], 1);
    assert!(status["last_error"]
        .as_str()
        .unwrap()
        .contains("fixture inference failed"));
}

#[test]
fn invalid_window_is_rejected_before_any_backend_inference() {
    let worker = Worker::start().unwrap();
    let mut invalid = window(10_000);
    invalid.frames[0][0] = f32::NAN;
    worker.submit(invalid);
    assert_eq!(worker.status()["dropped_windows"], 1);
    assert!(worker.latest().is_none());
}
