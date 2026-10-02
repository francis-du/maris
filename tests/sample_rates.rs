use crossbeam_queue::ArrayQueue;
use maris::resample::ClockBridge;
use std::sync::Arc;

fn converted_rms(input_rate: u32, output_rate: u32, hz: f64) -> f64 {
    let queue = Arc::new(ArrayQueue::new(8192));
    let target = (input_rate / 100) as usize;
    let mut bridge =
        ClockBridge::with_rates(queue.clone(), target, input_rate, output_rate).unwrap();
    let mut index = 0_u64;
    let mut sum = 0.0;
    let mut count = 0;
    for frame in 0..output_rate / 4 {
        while queue.len() < target + 128 + 8 {
            let x =
                (std::f64::consts::TAU * hz * index as f64 / input_rate as f64).sin() as f32 * 0.25;
            queue.push([x, -x]).unwrap();
            index += 1;
        }
        let (sample, underrun) = bridge.next_frame();
        assert!(!underrun);
        assert!(sample.iter().all(|v| v.is_finite()));
        assert_eq!(sample[0], -sample[1]);
        if frame > output_rate / 10 {
            sum += f64::from(sample[0]).powi(2);
            count += 1;
        }
    }
    (sum / count as f64).sqrt()
}

#[test]
fn every_native_rate_pair_preserves_audible_passband_and_stereo() {
    for input in [44100, 48000, 96000, 192000] {
        for output in [44100, 48000, 96000, 192000] {
            let measured = converted_rms(input, output, 1000.0);
            let expected = 0.25 / std::f64::consts::SQRT_2;
            assert!(
                (20.0 * (measured / expected).log10()).abs() < 0.08,
                "{input} -> {output}: {measured}"
            );
        }
    }
}

#[test]
fn downsampling_filters_out_of_band_energy_instead_of_aliasing_it() {
    for (input, output, hz) in [
        (96000, 48000, 35000.0),
        (192000, 48000, 35000.0),
        (192000, 44100, 30000.0),
    ] {
        let pass = converted_rms(input, output, 1000.0);
        let stop = converted_rms(input, output, hz);
        let rejection = 20.0 * (stop / pass).log10();
        assert!(
            rejection < -65.0,
            "{input} -> {output} at {hz}: {rejection} dB"
        );
    }
}

#[test]
fn invalid_rates_and_insufficient_reserve_are_rejected_without_processing() {
    for (input, output) in [(0, 48000), (48000, 0), (48000, 384000), (32000, 48000)] {
        assert!(
            ClockBridge::with_rates(Arc::new(ArrayQueue::new(4096)), 480, input, output).is_err()
        );
    }
    assert!(ClockBridge::with_rates(Arc::new(ArrayQueue::new(100)), 480, 48000, 48000).is_err());
    assert!(
        ClockBridge::with_rates(Arc::new(ArrayQueue::new(4096)), usize::MAX, 48000, 48000).is_err()
    );
}

#[test]
fn conversion_restarts_cleanly_after_capture_starvation() {
    let queue = Arc::new(ArrayQueue::new(8192));
    let mut bridge = ClockBridge::with_rates(queue.clone(), 480, 192000, 44100).unwrap();
    for _ in 0..700 {
        queue.push([0.1, -0.1]).unwrap();
    }
    let mut underruns = 0;
    for _ in 0..1000 {
        underruns += usize::from(bridge.next_frame().1);
    }
    assert_eq!(underruns, 1);
    assert!(!bridge.is_ready());
    for _ in 0..1000 {
        queue.push([0.2, -0.2]).unwrap();
    }
    let (first, under) = bridge.next_frame();
    assert!(!under && bridge.is_ready());
    assert!(first[0].abs() < 0.001);
}
