use super::*;

#[test]
fn retune_resets_downstream_outer_eq_state_after_upstream_change() {
    let mut old_profile = Profile::default();
    old_profile.bands[0].gain_db = 6.0;
    old_profile.bands[1].gain_db = 6.0;
    let mut new_profile = old_profile.clone();
    new_profile.bands[0].gain_db = -6.0;

    let old = Settings::compile(&old_profile, 48_000).unwrap();
    let new = Settings::compile(&new_profile, 48_000).unwrap();
    let mut trained = Chain::new(old);
    let _ = trained.frame([0.5, 0.5]);

    let mut retuned = trained.retune(new);
    let mut fresh = Chain::new(new);
    let actual = retuned.frame([0.0, 0.0]);
    let expected = fresh.frame([0.0, 0.0]);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-12,
            "outer downstream biquad leaked stale state after upstream EQ change: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}

#[test]
fn retune_resets_crossfeed_lowpass_state_when_width_changes_its_input() {
    let old_profile = Profile {
        crossfeed: 0.2,
        ..Profile::default()
    };
    let mut new_profile = old_profile.clone();
    new_profile.stereo_width = 1.25;

    let old = Settings::compile(&old_profile, 48_000).unwrap();
    let new = Settings::compile(&new_profile, 48_000).unwrap();
    let mut trained = Chain::new(old);
    let _ = trained.frame([0.5, -0.25]);

    let mut retuned = trained.retune(new);
    let mut fresh = Chain::new(new);
    let actual = retuned.frame([0.0, 0.0]);
    let expected = fresh.frame([0.0, 0.0]);
    for channel in 0..2 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1e-12,
            "crossfeed lowpass leaked stale state after width changed its feed: actual={} fresh={}",
            actual[channel],
            expected[channel]
        );
    }
}
