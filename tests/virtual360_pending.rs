use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn settings(amount: f64) -> Settings {
    let music = MusicProfile {
        virtual_surround: amount,
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

fn frame(i: usize) -> [f32; 2] {
    let t = i as f64 / 48_000.0;
    [
        (0.08
            * (0.6 * (std::f64::consts::TAU * 500.0 * t).sin()
                + 0.4 * (std::f64::consts::TAU * 3500.0 * t).sin())) as f32,
        (0.08
            * (0.55 * (std::f64::consts::TAU * 700.0 * t + 0.3).sin()
                + 0.45 * (std::f64::consts::TAU * 4200.0 * t + 0.8).sin())) as f32,
    ]
}

fn maximum_makeup_step(schedule: &[(usize, usize)]) -> f64 {
    let settings = [settings(0.2), settings(1.0), settings(0.4), settings(0.8)];
    let mut processor = Processor::new(settings[0]);
    let mut index = 0usize;
    for _ in 0..48_000 {
        let _ = processor.process(frame(index));
        index += 1;
    }
    let mut previous = processor.level_match_makeup_db();
    let mut maximum = 0.0_f64;
    let mut schedule_index = 0usize;
    for local in 0..12_000 {
        if schedule_index < schedule.len() && local == schedule[schedule_index].0 {
            processor.update(settings[schedule[schedule_index].1]);
            schedule_index += 1;
        }
        let _ = processor.process(frame(index));
        index += 1;
        let makeup = processor.level_match_makeup_db();
        maximum = maximum.max((makeup - previous).abs());
        previous = makeup;
    }
    maximum
}

#[test]
fn virtual360_reversal_and_pending_targets_keep_makeup_telemetry_continuous() {
    for (name, step) in [
        ("A-B-A", maximum_makeup_step(&[(0, 1), (144, 0)])),
        (
            "A-B-C-A",
            maximum_makeup_step(&[(0, 1), (48, 2), (96, 3), (144, 0)]),
        ),
        ("A-B-C", maximum_makeup_step(&[(0, 1), (48, 2), (96, 3)])),
    ] {
        assert!(
            step < 0.08,
            "{name} produced a Level Match telemetry step: {step:.4} dB"
        );
    }
}
