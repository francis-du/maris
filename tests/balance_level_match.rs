use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn settled_channel_gain_db(balance: f64, level_match: bool) -> [f64; 2] {
    let profile = Profile::default();
    let music = MusicProfile {
        balance,
        level_match,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let mut processor = Processor::new(
        Settings::compile(&profile, 48_000)
            .unwrap()
            .with_music(&music, 48_000)
            .unwrap(),
    );
    let mut input_power = 0.0_f64;
    let mut output_power = [0.0_f64; 2];
    for i in 0..384_000 {
        let x = (0.08 * (std::f64::consts::TAU * 997.0 * i as f64 / 48_000.0).sin()) as f32;
        let output = processor.process([x, x]);
        if i >= 288_000 {
            input_power += f64::from(x).powi(2);
            for channel in 0..2 {
                output_power[channel] += f64::from(output[channel]).powi(2);
            }
        }
    }
    output_power.map(|power| 10.0 * (power / input_power).max(1e-15).log10())
}

#[test]
fn level_match_does_not_boost_the_unattenuated_side_of_balance() {
    for balance in [-1.0, -0.75, -0.5, 0.5, 0.75, 1.0] {
        let gains = settled_channel_gain_db(balance, true);
        let untouched = if balance > 0.0 { gains[1] } else { gains[0] };
        assert!(
            untouched.abs() < 0.25,
            "balance {balance:+.2} caused Level Match to boost the untouched channel by {untouched:.3} dB"
        );
    }
}
