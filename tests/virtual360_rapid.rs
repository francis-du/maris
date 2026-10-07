use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn settings(amount: f64) -> Settings {
    let music = MusicProfile {
        virtual_surround: amount,
        width: 1.25,
        stereo_focus: 0.2,
        level_match: true,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    Settings::compile(&Profile::default(), 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap()
}

#[test]
fn rapid_virtual360_amount_updates_keep_audio_and_makeup_continuous() {
    const RATE: u32 = 48_000;
    const WINDOW: usize = 240;
    let settings = [settings(0.2), settings(1.0), settings(0.4), settings(0.8)];
    let frame = |i: usize| {
        let t = i as f64 / RATE as f64;
        [
            (0.08
                * (0.6 * (std::f64::consts::TAU * 500.0 * t).sin()
                    + 0.4 * (std::f64::consts::TAU * 3500.0 * t).sin())) as f32,
            (0.08
                * (0.55 * (std::f64::consts::TAU * 700.0 * t + 0.3).sin()
                    + 0.45 * (std::f64::consts::TAU * 4200.0 * t + 0.8).sin())) as f32,
        ]
    };
    let mut processor = Processor::new(settings[0]);
    let mut index = 0usize;
    for _ in 0..RATE as usize {
        let _ = processor.process(frame(index));
        index += 1;
    }
    let interval = RATE as usize / 1000;
    let mut next = 1usize;
    let mut minimum_delta = f64::INFINITY;
    let mut maximum_delta = f64::NEG_INFINITY;
    let mut previous = [0.0_f32; 2];
    let mut maximum_sample_step = 0.0_f32;
    let mut previous_makeup = processor.level_match_makeup_db();
    let mut maximum_makeup_step = 0.0_f64;
    for _ in 0..100 {
        let mut input_power = 0.0_f64;
        let mut output_power = 0.0_f64;
        for offset in 0..WINDOW {
            let absolute = index + offset;
            if absolute.is_multiple_of(interval) {
                processor.update(settings[next]);
                next = (next + 1) % settings.len();
            }
            let input = frame(index);
            index += 1;
            let output = processor.process(input);
            for channel in 0..2 {
                input_power += f64::from(input[channel]).powi(2);
                output_power += f64::from(output[channel]).powi(2);
                maximum_sample_step =
                    maximum_sample_step.max((output[channel] - previous[channel]).abs());
                assert!(output[channel].is_finite() && output[channel].abs() <= 0.891_252);
            }
            previous = output;
            let makeup = processor.level_match_makeup_db();
            maximum_makeup_step =
                maximum_makeup_step.max((makeup - previous_makeup).abs());
            previous_makeup = makeup;
        }
        let delta = 10.0 * (output_power / input_power.max(1e-30)).log10();
        minimum_delta = minimum_delta.min(delta);
        maximum_delta = maximum_delta.max(delta);
    }
    assert!(
        minimum_delta > -2.0 && maximum_delta < 2.0,
        "rapid Virtual 360 updates changed 5 ms loudness too much: min={minimum_delta:.3} dB max={maximum_delta:.3} dB"
    );
    assert!(
        maximum_sample_step < 0.08,
        "rapid Virtual 360 updates produced a click-sized sample step: {maximum_sample_step:.4}"
    );
    assert!(
        maximum_makeup_step < 0.08,
        "rapid Virtual 360 updates produced a Level Match telemetry step: {maximum_makeup_step:.4}"
    );
}
