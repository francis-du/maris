use maris::{
    analysis::{self, Analysis},
    smart,
    store::{atomic_json_new, Snapshot, Store},
};
use serde_json::json;

fn tone(hz: f64, opposite: bool) -> Vec<[f32; 2]> {
    (0..96000)
        .map(|i| {
            let sample = (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin() as f32 * 0.1;
            [sample, if opposite { -sample } else { sample }]
        })
        .collect()
}
fn evidence() -> Analysis {
    analysis::measure(&tone(8000.0, false), 48000).unwrap()
}

#[test]
fn spectral_evidence_locates_bass_and_treble() {
    for (hz, band) in [(125.0, 2), (8000.0, 8)] {
        let a = analysis::measure(&tone(hz, false), 48000).unwrap();
        a.validate().unwrap();
        assert!(a.band_energy_share[band] > 0.98);
        assert!((a.rms_dbfs + 23.0103).abs() < 0.1);
        // An 8 kHz sine sampled at 48 kHz never lands on its analog crest.
        let expected_peak = if hz == 8000.0 {
            20.0 * (0.1 * 3.0_f64.sqrt() / 2.0).log10()
        } else {
            -20.0
        };
        assert!((a.peak_dbfs - expected_peak).abs() < 0.01);
    }
}
#[test]
fn bs1770_metrics_are_separate_from_sample_peak_and_rms() {
    let frames: Vec<[f32; 2]> = (0..192000)
        .map(|i| {
            let sample =
                (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin() as f32 * 0.1;
            [sample, sample]
        })
        .collect();
    let a = analysis::measure(&frames, 48000).unwrap();
    assert!(a.momentary_lufs.is_some_and(f64::is_finite));
    assert!(a.shortterm_lufs.is_some_and(f64::is_finite));
    assert!(a.integrated_lufs.is_some_and(f64::is_finite));
    let true_peak = a.true_peak_dbtp.unwrap();
    assert!(true_peak.is_finite());
    assert!(true_peak + 0.05 >= a.peak_dbfs);
    assert!(true_peak < -18.0);
}

#[test]
fn opposite_phase_channels_do_not_cancel_analysis() {
    let a = analysis::measure(&tone(1000.0, true), 48000).unwrap();
    assert!(a.band_energy_share[5] > 0.98);
    assert!(a.stereo_correlation < -0.999);
}
#[test]
fn invalid_and_missing_evidence_fails_closed() {
    assert!(analysis::measure(&[[f32::NAN, 0.0]], 48000).is_err());
    assert!(analysis::measure(&[], 48000).is_err());
    let silence = analysis::measure(&vec![[0.0; 2]; 96000], 48000).unwrap();
    assert!(smart::propose(&Snapshot::default(), &silence, "warm").is_err());
    let mut a = evidence();
    a.frames = 480;
    assert!(smart::propose(&Snapshot::default(), &a, "soft").is_err());
    a = evidence();
    a.dropped_frames = 1;
    assert!(smart::propose(&Snapshot::default(), &a, "soft").is_err());
}
#[test]
fn proposal_is_bounded_and_does_not_modify_state() {
    let state = Snapshot::default();
    let a = evidence();
    let p = smart::propose(&state, &a, "soft").unwrap();
    assert!(!p.changes.is_empty());
    for (before, after) in state.profile.bands.iter().zip(&p.profile.bands) {
        assert!((before.gain_db - after.gain_db).abs() <= 1.0);
        assert_eq!(before.frequency_hz, after.frequency_hz);
    }
    assert!(p.profile.preamp_db <= state.profile.preamp_db);
    assert_eq!(p.algorithm, "maris-tonal-heuristic-v1");
    assert_eq!(state.revision, 0);
}
#[test]
fn manual_edits_invalidate_smart_preview() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let p = smart::propose(&store.load().unwrap(), &evidence(), "soft").unwrap();
    store
        .edit(Some(0), |profile| {
            profile.bands[0].gain_db = -2.0;
            Ok(())
        })
        .unwrap();
    assert!(smart::apply(&store, &p).is_err());
    assert_eq!(store.load().unwrap().profile.bands[0].gain_db, -2.0);
}
#[test]
fn tampered_and_expired_proposals_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let p = smart::propose(&store.load().unwrap(), &evidence(), "soft").unwrap();
    let mut modified = p.clone();
    modified.profile.bands[0].gain_db = 12.0;
    assert!(smart::apply(&store, &modified).is_err());
    modified = p;
    modified.created_at_ms = 0;
    assert!(smart::apply(&store, &modified).is_err());
    assert_eq!(store.load().unwrap().revision, 0);
}
#[test]
fn applying_and_undoing_a_proposal_round_trips() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path());
    let before = store.load().unwrap();
    let p = smart::propose(&before, &evidence(), "soft").unwrap();
    let after = smart::apply(&store, &p).unwrap();
    assert_eq!(after.revision, 1);
    assert_eq!(store.undo(Some(1)).unwrap().profile, before.profile);
}
#[test]
fn export_never_overwrites_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("proposal.json");
    atomic_json_new(&file, &json!({"original":true})).unwrap();
    assert!(atomic_json_new(&file, &json!({"replacement":true})).is_err());
    assert!(std::fs::read_to_string(&file).unwrap().contains("original"));
}

#[test]
fn spectral_band_boundaries_follow_the_geometric_midpoints() {
    // At 48 kHz the analysis FFT is 8192 bins wide, so bin centres are
    // 5.859375 Hz apart. Each band boundary is the geometric mean of two
    // adjacent profile frequencies, so the bin just below it and the next bin
    // up must land in adjacent bands.
    let bin = 48000.0 / 8192.0;
    let dominant = |share: [f64; analysis::BINS]| {
        share
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0
    };
    for (lower_bin, upper_bin, lower_band, upper_band) in [(30, 31, 2, 3), (965, 966, 7, 8)] {
        let midpoint = (maris::dsp::profile::FREQUENCIES[lower_band]
            * maris::dsp::profile::FREQUENCIES[upper_band])
            .sqrt();
        let below = bin * lower_bin as f64;
        let above = bin * upper_bin as f64;
        assert!(
            below < midpoint && midpoint < above,
            "bins {below}/{above} Hz must straddle the {midpoint} Hz boundary"
        );
        let lower = analysis::measure(&tone(below, false), 48000).unwrap();
        lower.validate().unwrap();
        let upper = analysis::measure(&tone(above, false), 48000).unwrap();
        upper.validate().unwrap();
        assert_eq!(
            dominant(lower.band_energy_share),
            lower_band,
            "{below} Hz should be dominated by band {lower_band}"
        );
        assert_eq!(
            dominant(upper.band_energy_share),
            upper_band,
            "{above} Hz should be dominated by band {upper_band}"
        );
    }
    // Everything at or above the last geometric midpoint falls through to the
    // top band rather than being dropped.
    let top = analysis::measure(&tone(16000.0, false), 48000).unwrap();
    top.validate().unwrap();
    assert_eq!(dominant(top.band_energy_share), 9);
}
