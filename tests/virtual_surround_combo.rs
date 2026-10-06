use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn pan_shift_db(rate: u32) -> f64 {
    let mut music = MusicProfile {
        virtual_surround: 1.0,
        level_match: true,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    music.bass_assist.enabled = true;
    music.bass_assist.amount = 1.0;

    let mut processor = Processor::new(
        Settings::compile(&Profile::default(), rate)
            .unwrap()
            .with_music(&music, rate)
            .unwrap(),
    );
    let mut power = [0.0_f64; 2];
    let total = rate as usize * 3;
    for i in 0..total {
        let x = (0.25 * (std::f64::consts::TAU * 65.0 * i as f64 / rate as f64).sin()) as f32;
        let output = processor.process([x, x * 0.2]);
        if i >= rate as usize * 2 {
            for channel in 0..2 {
                power[channel] += f64::from(output[channel]).powi(2);
            }
        }
        assert!(output
            .iter()
            .all(|sample| sample.is_finite() && sample.abs() <= 0.891_252));
    }

    10.0 * (power[0] / power[1]).log10() - 20.0 * 5.0_f64.log10()
}

#[test]
fn virtual_surround_bass_assist_and_level_match_preserve_deep_bass_pan_across_rates() {
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let shift = pan_shift_db(rate);
        assert!(
            shift.abs() < 0.15,
            "{rate} Hz: combined Virtual 360/Bass Assist/Level Match shifted 65 Hz pan by {shift:.3} dB"
        );
    }
}
