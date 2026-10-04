use maris::{
    dsp::{Processor, Settings},
    music::{AdaptiveEq, MusicProfile},
    profile::Profile,
};

#[test]
fn level_match_telemetry_reports_static_recovery() {
    let profile = Profile::default();
    let music = MusicProfile {
        bass_db: 6.0,
        adaptive: AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        ..MusicProfile::default()
    };
    let settings = Settings::compile(&profile, 48_000)
        .unwrap()
        .with_music(&music, 48_000)
        .unwrap();
    assert!(settings.effective_preamp_db() < -5.0);

    let mut processor = Processor::new(settings);
    let _ = processor.process([0.01, 0.01]);
    assert!(
        processor.level_match_makeup_db() > 5.0,
        "level-match telemetry did not report recovered safety headroom"
    );
}
