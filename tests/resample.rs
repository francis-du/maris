use crossbeam_queue::ArrayQueue;
use maris::resample::{ClockBridge, SincInterpolator, TAPS};
use std::sync::Arc;

#[test]
fn fractional_delay_preserves_audio_band_including_treble() {
    for rate in [44100, 48000, 96000] {
        for hz in [31.0, 997.0, 10000.0, 18000.0, 20000.0] {
            let mut interpolator = SincInterpolator::new();
            let (mut reference, mut measured, mut residual) = (0.0, 0.0, 0.0);
            for i in 0..8192 {
                let x = (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin() as f32;
                interpolator.push([x, -x]);
                if i < TAPS {
                    continue;
                }
                let fraction = 0.373;
                let expected =
                    (std::f64::consts::TAU * hz * (i as f64 - 64.0 + fraction) / rate as f64).sin();
                let actual = interpolator.sample(fraction);
                assert_eq!(actual[0], -actual[1]);
                reference += expected.powi(2);
                measured += (actual[0] as f64).powi(2);
                residual += (actual[0] as f64 - expected).powi(2);
            }
            let error = 10.0_f64 * (measured / reference).log10();
            println!("sinc {rate} Hz / {hz} Hz: {error:.5} dB");
            assert!(error.abs() < 0.05, "Treble loss at {rate}/{hz}: {error} dB");
            assert!(
                residual / reference < 1e-5,
                "Excess interpolation error at {rate}/{hz}"
            );
        }
    }
}
#[test]
fn starvation_does_not_invent_audio_or_grow_buffers() {
    let queue = Arc::new(ArrayQueue::new(8192));
    let mut bridge = ClockBridge::new(queue.clone(), 480);
    for _ in 0..1000 {
        assert_eq!(bridge.next_frame(), ([0.0; 2], false));
    }
    for _ in 0..4096 {
        queue.push([0.1, -0.1]).unwrap();
    }
    let mut gaps = 0;
    for _ in 0..10000 {
        let (frame, gap) = bridge.next_frame();
        gaps += usize::from(gap);
        assert!(frame.iter().all(|v| v.is_finite() && v.abs() < 0.101));
    }
    assert_eq!(gaps, 1);
    assert!(bridge.next_frame().0[0].abs() < 1e-12);
}
