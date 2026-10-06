use maris::music::MusicProfile;

fn side_difference_db(rate: u32, hz: f64) -> f64 {
    let base = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut surround = base.clone();
    surround.virtual_surround = 1.0;
    let mut dry = maris::music::Processor::new(base.compile(rate).unwrap());
    let mut wet = maris::music::Processor::new(surround.compile(rate).unwrap());

    let mut reference_power = 0.0_f64;
    let mut difference_power = 0.0_f64;
    let total = rate as usize * 3;
    for i in 0..total {
        let x = 0.12 * (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin();
        let a = dry.process([x, -x]);
        let b = wet.process([x, -x]);
        if i >= rate as usize * 2 {
            reference_power += a[0] * a[0];
            difference_power += (b[0] - a[0]).powi(2);
            assert!((b[0] + b[1]).abs() < 1e-10);
        }
    }
    10.0 * (difference_power.max(1e-30) / reference_power.max(1e-30)).log10()
}

#[test]
fn virtual_surround_leaves_bass_side_stable_across_sample_rates() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let low_bass = side_difference_db(rate, 80.0);
        let upper_bass = side_difference_db(rate, 100.0);
        assert!(
            low_bass < -25.0,
            "{rate} Hz: Virtual 360 changed 80 Hz Side by {low_bass:.2} dB relative error"
        );
        assert!(
            upper_bass < -20.0,
            "{rate} Hz: Virtual 360 changed 100 Hz Side by {upper_bass:.2} dB relative error"
        );
    }
}

#[test]
fn virtual_surround_still_decorrelates_high_frequency_side() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let difference = side_difference_db(rate, 4_000.0);
        assert!(
            difference > 3.0,
            "{rate} Hz: Virtual 360 no longer meaningfully changes 4 kHz Side: {difference:.2} dB"
        );
    }
}

fn proportional_pan_shift_db(rate: u32, hz: f64) -> f64 {
    let base = MusicProfile {
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut surround = base.clone();
    surround.virtual_surround = 1.0;
    let mut processor = maris::music::Processor::new(surround.compile(rate).unwrap());
    let mut power = [0.0_f64; 2];
    let total = rate as usize * 3;
    for i in 0..total {
        let x = 0.25 * (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin();
        let output = processor.process([x, x * 0.2]);
        if i >= rate as usize * 2 {
            for channel in 0..2 {
                power[channel] += output[channel] * output[channel];
            }
        }
    }
    let output_ratio_db = 10.0 * (power[0] / power[1]).log10();
    let input_ratio_db = 20.0 * 5.0_f64.log10();
    output_ratio_db - input_ratio_db
}

#[test]
fn virtual_surround_keeps_deep_bass_pan_stable_across_sample_rates() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let shift = proportional_pan_shift_db(rate, 65.0);
        assert!(
            shift.abs() < 0.15,
            "{rate} Hz: Virtual 360 shifted 65 Hz fixed pan by {shift:.3} dB"
        );
    }
}


#[test]
fn virtual_surround_still_decorrelates_low_mid_side_across_sample_rates() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let difference = side_difference_db(rate, 500.0);
        assert!(
            difference > 4.5,
            "{rate} Hz: Virtual 360 weakened 500 Hz Side decorrelation to {difference:.2} dB"
        );
    }
}
