use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn sine_gain(profile: &Profile, music: &MusicProfile, hz: f64) -> f64 {
    let mut dsp = Processor::new(
        Settings::compile(profile, 48000)
            .unwrap()
            .with_music(music, 48000)
            .unwrap(),
    );
    let (mut input, mut output) = (0.0, 0.0);
    for i in 0..96000 {
        let x = (std::f64::consts::TAU * hz * i as f64 / 48000.0).sin() as f32 * 0.01;
        let y = dsp.process([x, x])[0];
        if i >= 48000 {
            input += (x as f64).powi(2);
            output += (y as f64).powi(2);
        }
    }
    10.0_f64 * (output / input).log10()
}

#[test]
fn disabled_music_is_transparent_even_with_nonzero_settings() {
    let music = MusicProfile {
        bass_db: 6.0,
        air_db: -3.0,
        correction_preamp_db: -12.0,
        enabled: false,
        ..MusicProfile::default()
    };
    let mut dsp = maris::music::Processor::new(music.compile(48000).unwrap());
    for i in 0..9600 {
        let x = (i as f64 * 0.19).sin() * 0.1;
        let y = dsp.process([x, -x]);
        assert!(
            (y[0] - x).abs() < 1e-10,
            "Disabled music still changes amplitude"
        );
        assert!((y[1] + x).abs() < 1e-10);
    }
}

#[test]
fn bass_and_air_reserve_shared_headroom_not_two_independent_cuts() {
    let eq = Profile::default();
    let music = MusicProfile {
        bass_db: 6.0,
        air_db: 3.0,
        level_match: false,
        ..MusicProfile::default()
    };
    let mid = sine_gain(&eq, &music, 1000.0);
    assert!(mid > -7.2, "Stacked attenuation wastes level: {mid:.3} dB");
    let bass = sine_gain(&eq, &music, 30.0);
    assert!(
        bass - mid > 5.5,
        "Bass change must survive the entire processing chain"
    );
}

#[test]
fn bypass_and_processing_share_static_safety_gain() {
    let music = MusicProfile {
        air_db: 1.0,
        adaptive: maris::music::AdaptiveEq {
            enabled: false,
            strength: 0.0,
        },
        level_match: false,
        ..MusicProfile::default()
    };
    let active = Profile::preset("bass").unwrap();
    let mut bypass = active.clone();
    bypass.bypass = true;
    let active_gain = Settings::compile(&active, 48000)
        .unwrap()
        .with_music(&music, 48000)
        .unwrap()
        .effective_preamp_db();
    let bypass_gain = Settings::compile(&bypass, 48000)
        .unwrap()
        .with_music(&music, 48000)
        .unwrap()
        .effective_preamp_db();
    assert!(
        (active_gain - bypass_gain).abs() < 1e-9,
        "Bypass and processing used different safety gains: {bypass_gain:.3} vs {active_gain:.3} dB"
    );
}

fn rms_gain(profile: &Profile, music: &MusicProfile) -> f64 {
    let mut processor = Processor::new(
        Settings::compile(profile, 48000)
            .unwrap()
            .with_music(music, 48000)
            .unwrap(),
    );
    let mut power = 0.0;
    for i in 0..384000 {
        let t = i as f64 / 48000.0;
        let x = (0.12 * (std::f64::consts::TAU * 220.0 * t).sin()
            + 0.08 * (std::f64::consts::TAU * 1800.0 * t).sin()
            + 0.04 * (std::f64::consts::TAU * 7600.0 * t).sin()) as f32;
        let y = processor.process([x, x])[0] as f64;
        if i >= 288000 {
            power += y * y;
        }
    }
    power
}

#[test]
fn global_bypass_is_loudness_matched_to_music_processing() {
    let music = MusicProfile {
        air_db: 0.5,
        softness: 1.0,
        width: 1.5,
        adaptive: maris::music::AdaptiveEq {
            enabled: true,
            strength: 1.0,
        },
        compressor: maris::music::Compressor {
            enabled: true,
            ..maris::music::Compressor::default()
        },
        reference: false,
        level_match: true,
        ..MusicProfile::default()
    };
    let active = Profile::default();
    let mut bypass = active.clone();
    bypass.bypass = true;
    let wet = rms_gain(&active, &music);
    let dry = rms_gain(&bypass, &music);
    let delta_db = 10.0 * (dry / wet).log10();
    assert!(
        delta_db.abs() < 0.35,
        "Bypass/processing loudness mismatch remains {delta_db:.3} dB"
    );
}

#[test]
fn gain_only_identity_has_no_added_distortion_or_crosstalk() {
    let eq = Profile::default();
    let mut dsp = Processor::new(Settings::compile(&eq, 48000).unwrap());
    let gain = 10.0_f64.powf(eq.preamp_db / 20.0);
    let (mut signal, mut residual) = (0.0, 0.0);
    for i in 0..48000 {
        let x = (std::f64::consts::TAU * 997.0 * i as f64 / 48000.0).sin() as f32 * 0.2;
        let y = dsp.process([x, 0.0]);
        let reference = x as f64 * gain;
        signal += reference * reference;
        residual += (y[0] as f64 - reference).powi(2);
        assert_eq!(y[1], 0.0);
    }
    assert!(10.0_f64 * (residual / signal).log10() < -120.0);
}
