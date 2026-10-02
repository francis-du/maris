use super::*;

#[test]
fn silence_has_zero_log_mel_and_expected_patch_size() {
    let patch = mel_patch(&vec![0.0; RATE as usize * 3]).unwrap();
    assert_eq!(patch.len(), PATCH_FRAMES * MELS);
    assert!(patch.iter().all(|value| value.abs() < 1e-7));
}

#[test]
fn resampler_is_finite_and_preserves_duration() {
    let input: Vec<[f32; 2]> = (0..48_000 * 3)
        .map(|i| {
            let value = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin();
            [value, value]
        })
        .collect();
    let output = resample_mono(&input, 48_000).unwrap();
    assert_eq!(output.len(), 48_000);
    assert!(output.iter().all(|value| value.is_finite()));
    let patch = mel_patch(&output).unwrap();
    assert!(patch.iter().any(|value| *value > 0.01));
}

#[test]
fn downsampling_removes_out_of_band_tones_without_erasing_music() {
    for source_rate in [44_100, 48_000, 96_000, 192_000, 384_000] {
        let rms = |frequency: f64| {
            let frames: Vec<_> = (0..source_rate / 8)
                .map(|index| {
                    let value = (std::f64::consts::TAU * frequency * f64::from(index)
                        / f64::from(source_rate))
                    .sin() as f32;
                    [value; 2]
                })
                .collect();
            let output = resample_mono(&frames, source_rate).unwrap();
            assert_eq!(output.len(), 2_000);
            // Ignore filter edges; measure the retained waveform, not model labels.
            let steady = &output[128..output.len() - 128];
            (steady.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / steady.len() as f64).sqrt()
        };
        let passband = rms(1_000.0);
        assert!(
            (0.69..0.72).contains(&passband),
            "lost passband at {source_rate}: {passband}"
        );
        let rejected = rms(10_000.0);
        assert!(
            rejected < 0.005,
            "10 kHz aliases into the 16 kHz input at {source_rate}: {rejected}"
        );
    }
}

// The previous implementation is retained only here as an optimization comparison.
// This is a Maris regression reference, not a claim of reference-model parity.
fn uncached_mel_patch(samples: &[f32]) -> Vec<f32> {
    let filters = mel_filters();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT);
    let hann: Vec<f32> = (0..FFT)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / FFT as f32).cos())
        .collect();
    let usable = (1 + samples.len() / HOP).min(PATCH_FRAMES);
    let mut patch = vec![0.0; PATCH_FRAMES * MELS];
    let mut buffer = vec![Complex::new(0.0, 0.0); FFT];
    for frame in 0..usable {
        let start = frame as isize * HOP as isize - (FFT / 2) as isize;
        for i in 0..FFT {
            let source = start + i as isize;
            let sample = if source >= 0 && (source as usize) < samples.len() {
                samples[source as usize]
            } else {
                0.0
            };
            buffer[i] = Complex::new(sample * hann[i], 0.0);
        }
        fft.process(&mut buffer);
        let mut power = [0.0; FFT / 2 + 1];
        for bin in 0..=FFT / 2 {
            power[bin] = buffer[bin].norm_sqr();
        }
        for mel in 0..MELS {
            let energy = power
                .iter()
                .zip(filters[mel].iter())
                .map(|(value, weight)| value * weight)
                .sum::<f32>();
            patch[frame * MELS + mel] = (10_000.0_f32.mul_add(energy, 1.0)).log10();
        }
    }
    patch
}

#[test]
fn reused_setup_preserves_every_value_and_keeps_concurrent_windows_separate() {
    let fixtures: Vec<Vec<f32>> = [48_000, 48_001, 48_255, 48_256, 48_511, 48_512, 64_000]
        .into_iter()
        .map(|size| (0..size).map(|i| (i as f32 * 0.173).sin() * 0.25).collect())
        .collect();
    std::thread::scope(|scope| {
        let jobs: Vec<_> = fixtures
            .iter()
            .map(|samples| {
                scope.spawn(move || {
                    let expected = uncached_mel_patch(samples);
                    assert_eq!(mel_patch(samples).unwrap(), expected);
                    assert_eq!(mel_patch(samples).unwrap(), expected);
                })
            })
            .collect();
        for job in jobs {
            job.join().unwrap();
        }
    });
}

#[test]
fn preprocessing_setup_ablation_reports_cost_without_changing_audio() {
    use std::{hint::black_box, time::Instant};
    let samples: Vec<f32> = (0..48_000)
        .map(|i| (i as f32 * 0.173).sin() * 0.25)
        .collect();
    assert_eq!(mel_patch(&samples).unwrap(), uncached_mel_patch(&samples));
    let mut uncached = std::time::Duration::ZERO;
    let mut cached = std::time::Duration::ZERO;
    for round in 0..8 {
        // Alternate order to avoid consistently favoring the second warm run.
        for old in [round % 2 == 0, round % 2 != 0] {
            let began = Instant::now();
            if old {
                black_box(uncached_mel_patch(black_box(&samples)));
            } else {
                black_box(mel_patch(black_box(&samples)).unwrap());
            }
            if old {
                uncached += began.elapsed();
            } else {
                cached += began.elapsed();
            }
        }
    }
    eprintln!("musicnn-preprocessing-ablation: rounds=8 uncached_us={} cached_us={} identical=true; setup/scratch only, not full model CPU",
        uncached.as_micros(), cached.as_micros());
}

#[test]
fn short_music_windows_are_rejected_instead_of_padded_into_recognition() {
    for length in [1, 15_999, 16_000, 32_000, 47_999] {
        assert!(
            mel_patch(&vec![0.05; length]).is_err(),
            "accepted {length} samples as a complete three-second music window"
        );
    }
    let silence = mel_patch(&vec![0.0; 48_000]).unwrap();
    assert_eq!(silence.len(), PATCH_FRAMES * MELS);
    assert!(silence.iter().all(|value| *value == 0.0));
}

#[test]
fn invalid_audio_is_rejected_not_fabricated() {
    assert!(resample_mono(&[], 48_000).is_err());
    assert!(resample_mono(&[[0.0, 0.0]], 0).is_err());
    assert!(mel_patch(&[]).is_err());
}
