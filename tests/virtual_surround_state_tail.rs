use maris::music::MusicProfile;

fn profile() -> MusicProfile {
    MusicProfile {
        virtual_surround: 1.0,
        level_match: false,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    }
}

fn ratio_db(power: [f64; 2]) -> f64 {
    10.0 * (power[0] / power[1].max(1e-30)).log10()
}

fn assert_state_tail_bounds(rate: u32) {
    let settings = profile().compile(rate).unwrap();
    let mut stressed = maris::music::Processor::new(settings);
    let mut fresh = maris::music::Processor::new(settings);

    for i in 0..rate as usize {
        let x = 0.15 * (std::f64::consts::TAU * 4_000.0 * i as f64 / rate as f64).sin();
        let _ = stressed.process([x, -x]);
    }

    let windows = [rate as usize / 100, rate as usize / 20, rate as usize / 10];
    let mut stressed_power = [[0.0_f64; 2]; 3];
    let mut fresh_power = [[0.0_f64; 2]; 3];

    for i in 0..windows[2] {
        let x = 0.25 * (std::f64::consts::TAU * 65.0 * i as f64 / rate as f64).sin();
        let actual = stressed.process([x, x * 0.2]);
        let expected = fresh.process([x, x * 0.2]);
        for (window_index, &limit) in windows.iter().enumerate() {
            if i < limit {
                for channel in 0..2 {
                    stressed_power[window_index][channel] += actual[channel] * actual[channel];
                    fresh_power[window_index][channel] += expected[channel] * expected[channel];
                }
            }
        }
    }

    let limits_db = [0.8, 0.15, 0.08];
    for (index, (&samples, &limit_db)) in windows.iter().zip(limits_db.iter()).enumerate() {
        let drift = ratio_db(stressed_power[index]) - ratio_db(fresh_power[index]);
        assert!(
            drift.abs() < limit_db,
            "{rate} Hz: stale Virtual 360 state shifted 65 Hz fixed pan by {drift:.3} dB over {} ms",
            samples * 1000 / rate as usize
        );
    }
}

#[test]
fn virtual_surround_high_side_history_does_not_linger_in_deep_bass_pan() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        assert_state_tail_bounds(rate);
    }
}
