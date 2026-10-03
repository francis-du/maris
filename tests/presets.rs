use maris::{
    dsp::{Processor, Settings},
    presets,
};
use std::collections::BTreeSet;

#[test]
fn fixed_eqmac_source_extracts_all_22_presets_with_stable_ids() {
    presets::assert_source_contract().unwrap();
    let raw = presets::raw_eqmac();
    assert_eq!(raw.len(), 22);
    let ids: BTreeSet<_> = raw.iter().map(|preset| preset.id).collect();
    assert_eq!(ids.len(), 22);
    assert!(ids.contains("eqmac:flat"));
    assert!(ids.contains("eqmac:acoustic"));
    assert!(ids.contains("eqmac:bass-reducer"));
    assert!(ids.contains("eqmac:treble-reducer"));
    for preset in raw {
        assert!(preset.id.starts_with("eqmac:"));
        assert!(preset.gains_db.iter().all(|gain| gain.is_finite()));
    }
}

#[test]
fn eqmac_flat_reuses_the_existing_unit_response_flat() {
    let source = presets::eqmac_by_original_name("Flat").unwrap();
    assert_eq!(source.gains_db, [0.0; 10]);
    let flat = presets::profile("flat", 48000).unwrap();
    assert_eq!(flat.preamp_db, 0.0);
    assert!(flat.bands.iter().all(|band| band.gain_db == 0.0));
    assert_eq!(flat.effective_preamp_at(48000), 0.0);
    let alias = presets::profile("eqmac:flat", 48000).unwrap();
    assert_eq!(alias, flat);

    let mut processor = Processor::new(Settings::compile(&flat, 48000).unwrap());
    for i in 0..48000 {
        let x = (std::f64::consts::TAU * 997.0 * i as f64 / 48000.0).sin() as f32 * 0.1;
        let y = processor.process([x, -x]);
        assert!((y[0] - x).abs() < 1e-6);
        assert!((y[1] + x).abs() < 1e-6);
    }
}

#[test]
fn octave_bandwidth_conversion_is_sample_rate_aware_and_supported() {
    for rate in [44100_u32, 48000, 96000] {
        for frequency in presets::EQMAC_FREQUENCIES_HZ {
            assert!(frequency < rate as f64 * 0.5);
            let q = presets::bandwidth_to_q(0.5, frequency, rate).unwrap();
            assert!(q.is_finite() && (0.2..=5.0).contains(&q));
        }
    }
    let low_rate_q = presets::bandwidth_to_q(0.5, 16000.0, 44100).unwrap();
    let high_rate_q = presets::bandwidth_to_q(0.5, 16000.0, 96000).unwrap();
    assert!((low_rate_q - high_rate_q).abs() > 0.1);
}

#[test]
fn every_eqmac_preset_compiles_with_conservative_cascade_headroom() {
    for rate in [44100_u32, 48000, 96000] {
        for raw in presets::raw_eqmac() {
            let profile = presets::profile(raw.id, rate).unwrap();
            profile.validate().unwrap();
            let peak = presets::response_peak_db(&profile, rate).unwrap();
            let preamp = profile.effective_preamp_at(rate);
            if peak > 0.01 {
                assert!(
                    peak + preamp <= -presets::SAFETY_MARGIN_DB + 0.02,
                    "{} at {rate}: peak={peak:.3}, preamp={preamp:.3}",
                    raw.id
                );
            }
            Settings::compile(&profile, rate).unwrap();
        }
    }
}

#[test]
fn high_gain_source_values_are_preserved_instead_of_clamped() {
    let acoustic = presets::eqmac_by_original_name("Acoustic").unwrap();
    assert_eq!(acoustic.gains_db[4], 18.22);
    let profile = presets::profile(acoustic.id, 48000).unwrap();
    assert_eq!(profile.bands[4].gain_db, 18.22);
    assert!(profile.effective_preamp_at(48000) < -10.0);
}

#[test]
fn eqmac_bypass_keeps_safety_headroom_for_loudness_safe_comparison() {
    let active = presets::profile("eqmac:acoustic", 48000).unwrap();
    let active_gain = active.effective_preamp_at(48000);
    assert!(active_gain < -10.0);
    let mut reference = active.clone();
    reference.bypass = true;
    assert!((reference.effective_preamp_at(48000) - active_gain).abs() < 1e-9);

    let mut legacy = presets::profile("bass", 48000).unwrap();
    legacy.bypass = true;
    assert_eq!(legacy.effective_preamp_at(48000), legacy.preamp_db);
}

#[test]
fn representative_presets_remain_finite_and_peak_limited_on_full_scale_material() {
    for id in [
        "eqmac:flat",
        "eqmac:bass-reducer",
        "eqmac:treble-reducer",
        "eqmac:acoustic",
    ] {
        let profile = presets::profile(id, 48000).unwrap();
        let mut processor = Processor::new(Settings::compile(&profile, 48000).unwrap());
        let mut seed = 0x1234_5678_u32;
        for i in 0..96000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = seed as f64 / u32::MAX as f64 * 2.0 - 1.0;
            let sine = (std::f64::consts::TAU * 997.0 * i as f64 / 48000.0).sin();
            let x = ((noise * 0.5 + sine * 0.75).clamp(-1.0, 1.0)) as f32;
            for sample in processor.process([x, -x * 0.93]) {
                assert!(sample.is_finite());
                assert!(sample.abs() <= 0.891252);
            }
        }
    }
}

#[test]
fn applying_a_tone_curve_preserves_independent_global_controls() {
    let current = maris::profile::Profile {
        crossfeed: 0.12,
        stereo_width: 1.35,
        bypass: true,
        ..maris::profile::Profile::default()
    };
    let applied = presets::apply_tone_curve(&current, "warm", 48_000).unwrap();
    assert_eq!(applied.crossfeed, current.crossfeed);
    assert_eq!(applied.stereo_width, current.stereo_width);
    assert_eq!(applied.bypass, current.bypass);
    assert_eq!(applied.name, "warm");
    assert_ne!(applied.bands, current.bands);

    let eqmac = presets::apply_tone_curve(&current, "eqmac:acoustic", 48_000).unwrap();
    assert_eq!(eqmac.crossfeed, current.crossfeed);
    assert_eq!(eqmac.stereo_width, current.stereo_width);
    assert_eq!(eqmac.bypass, current.bypass);
}

#[test]
fn tone_curve_matching_ignores_independent_global_controls() {
    let mut current =
        presets::apply_tone_curve(&maris::profile::Profile::default(), "warm", 48_000).unwrap();
    current.crossfeed = 0.17;
    current.stereo_width = 1.25;
    current.bypass = true;
    assert!(presets::tone_curve_matches(&current, "warm", 48_000));
    assert!(!presets::tone_curve_matches(&current, "vocal", 48_000));
}
