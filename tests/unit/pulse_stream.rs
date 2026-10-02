use super::*;
use crate::profile::Profile;
use std::io::Cursor;

fn setup() -> (Renderer, Arc<Metrics>, Arc<ArrayQueue<Update>>) {
    let metrics = Arc::new(Metrics::default());
    let updates = Arc::new(ArrayQueue::new(8));
    let renderer = Renderer::for_worker(
        Settings::compile(&Profile::default(), 48_000).unwrap(),
        48_000,
        updates.clone(),
        metrics.clone(),
    );
    (renderer, metrics, updates)
}

#[test]
fn worker_runs_actual_dsp_with_split_pcm_transfers_and_separate_timing() {
    struct Pieces(Cursor<Vec<u8>>);
    impl Read for Pieces {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let limit = bytes.len().min(13);
            self.0.read(&mut bytes[..limit])
        }
    }
    struct SmallWrites(Vec<u8>);
    impl Write for SmallWrites {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let count = bytes.len().min(17);
            self.0.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let (mut renderer, metrics, _) = setup();
    let samples: Vec<f32> = (0..4800)
        .flat_map(|i| {
            let x = (i as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin() * 0.2;
            [x, -x]
        })
        .collect();
    let bytes = samples.iter().flat_map(|x| x.to_le_bytes()).collect();
    let mut output = SmallWrites(Vec::new());
    let error = transfer(
        Pieces(Cursor::new(bytes)),
        &mut output,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        &metrics,
        &mut renderer,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    assert_eq!(output.0.len(), samples.len() * 4);
    let mut reference = super::super::super::bridge::Renderer::new(
        Settings::compile(&Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        Arc::new(Metrics::default()),
    );
    let mut expected = vec![0_f32; samples.len()];
    let mut pairs = samples.as_chunks::<2>().0.iter();
    reference.render(&mut expected, 2, || {
        let p = pairs.next().unwrap();
        ([p[0], p[1]], true)
    });
    for (bytes, expected) in output.0.as_chunks::<4>().0.iter().zip(expected) {
        assert_eq!(f32::from_le_bytes(*bytes), expected);
    }
    assert_eq!(metrics.captured_frames.load(Ordering::Relaxed), 4800);
    assert_eq!(metrics.worker_frames.load(Ordering::Relaxed), 4800);
    assert_eq!(metrics.callback_calls.load(Ordering::Relaxed), 0);
    assert!(metrics.worker_processing_nanos.load(Ordering::Relaxed) > 0);
}

#[test]
fn worker_rejects_partial_frames_and_does_not_publish_them() {
    let (mut renderer, metrics, _) = setup();
    let mut output = Vec::new();
    let result = transfer(
        Cursor::new(vec![1; 3839]),
        &mut output,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        &metrics,
        &mut renderer,
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    assert!(output.is_empty());
    assert_eq!(metrics.frames.load(Ordering::Relaxed), 0);
}

#[test]
fn worker_sanitizes_nonfinite_audio_and_observes_revisioned_updates() {
    let (mut renderer, metrics, updates) = setup();
    let profile = Profile {
        preamp_db: -6.0,
        ..Profile::default()
    };
    updates
        .push(Update {
            settings: Settings::compile(&profile, 48_000).unwrap(),
            revision: 8,
            music_revision: 12,
        })
        .ok()
        .unwrap();
    let values = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 4.0];
    let bytes: Vec<_> = (0..960).flat_map(|i| values[i % 4].to_le_bytes()).collect();
    let mut output = Vec::new();
    assert!(transfer(
        Cursor::new(bytes),
        &mut output,
        &AtomicBool::new(false),
        &AtomicBool::new(true),
        &metrics,
        &mut renderer
    )
    .is_err());
    assert_eq!(output.len(), 3840);
    for value in output.as_chunks::<4>().0 {
        let value = f32::from_le_bytes(*value);
        assert!(value.is_finite() && value.abs() <= 0.892);
    }
    assert_eq!(metrics.revision.load(Ordering::Relaxed), 8);
    assert_eq!(metrics.music_revision.load(Ordering::Relaxed), 12);
}

#[test]
fn stopped_worker_does_not_read_or_write_audio() {
    let (mut renderer, metrics, _) = setup();
    let mut output = Vec::new();
    transfer(
        Cursor::new(vec![0; 3840]),
        &mut output,
        &AtomicBool::new(true),
        &AtomicBool::new(true),
        &metrics,
        &mut renderer,
    )
    .unwrap();
    assert!(output.is_empty());
    assert_eq!(metrics.captured_frames.load(Ordering::Relaxed), 0);
}

#[test]
fn monitor_preroll_does_not_start_tonal_transition_before_route_confirmation() {
    let (mut renderer, metrics, _) = setup();
    let bytes: Vec<_> = (0..960).flat_map(|_| 0.5_f32.to_le_bytes()).collect();
    let mut output = Vec::new();
    assert!(transfer(
        Cursor::new(bytes),
        &mut output,
        &AtomicBool::new(false),
        &AtomicBool::new(false),
        &metrics,
        &mut renderer
    )
    .is_err());
    assert_eq!(output.len(), 3840);
    assert!(output.iter().all(|byte| *byte == 0));
    assert!(!metrics.source_started.load(Ordering::Relaxed));
}

#[test]
fn renderer_rejects_invalid_channel_layout_without_calling_source() {
    let (mut renderer, metrics, _) = setup();
    for channels in [0, 3, 8] {
        let mut output = [1_f32; 8];
        renderer.render(&mut output, channels, || {
            panic!("invalid layout consumed audio")
        });
        assert_eq!(output, [0.0; 8]);
    }
    assert_eq!(metrics.errors.load(Ordering::Relaxed), 3);
}
