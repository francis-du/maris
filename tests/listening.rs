use maris::{
    listening,
    music::MusicProfile,
    sound_cli::{self, Action},
    store::Store,
};

#[test]
fn device_preferences_do_not_leak_into_unknown_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let library = listening::edit(&store, Some(0), Some("USB headphones"), |p| {
        p.bass_db = 2.0;
        Ok(())
    })
    .unwrap();
    assert_eq!(library.effective("USB headphones").bass_db, 2.0);
    assert_eq!(library.effective("Speakers").bass_db, 0.0);
    assert_eq!(listening::load(&store).unwrap().revision, 1);
}
#[test]
fn stale_and_invalid_preferences_do_not_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    listening::edit(&store, Some(0), None, |p| {
        p.air_db = 1.0;
        Ok(())
    })
    .unwrap();
    assert!(listening::edit(&store, Some(0), None, |p| {
        p.air_db = 2.0;
        Ok(())
    })
    .is_err());
    assert!(listening::edit(&store, Some(1), None, |p| {
        p.air_db = 100.0;
        Ok(())
    })
    .is_err());
    assert_eq!(listening::load(&store).unwrap().default.air_db, 1.0);
}
#[test]
fn strict_peq_import_supports_peaks_and_shelves() {
    let p=listening::import_peq("Preamp: -4.5 dB\nFilter 1: ON LSC Fc 105 Hz Gain 3 dB Q 0.707\nFilter 2: ON PK Fc 2000 Hz Gain -2 dB Q 1.2\nFilter 3: ON HSC Fc 9000 Hz Gain -1 dB Q 0.707\n","User supplied measurement export").unwrap();
    assert_eq!(p.correction.len(), 3);
    assert_eq!(p.correction_preamp_db, -4.5);
    p.compile(48000).unwrap();
}
#[test]
fn unsupported_and_tampered_peq_fails_closed() {
    for text in [
        "Preamp: 10 dB\nFilter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1",
        "Include: other.txt",
        "Filter 1: ON PK Fc 1000 Hz Gain NaN dB Q 1",
        "Filter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1\nFilter 1: ON PK Fc 2000 Hz Gain 1 dB Q 1",
    ] {
        assert!(listening::import_peq(text, "test").is_err());
    }
}
#[test]
fn tonal_preset_preserves_device_correction() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path());
    let correction = listening::import_peq(
        "Preamp: -3 dB\nFilter 1: ON PK Fc 3000 Hz Gain -2 dB Q 2",
        "test source",
    )
    .unwrap();
    listening::edit(&store, Some(0), Some("Headphones"), |p| {
        *p = correction.clone();
        Ok(())
    })
    .unwrap();
    sound_cli::run(
        &store,
        Some("Headphones".into()),
        Some(1),
        Action::Preset {
            name: "warm".into(),
        },
    )
    .unwrap();
    let library = listening::load(&store).unwrap();
    assert_eq!(
        library.effective("Headphones").correction,
        correction.correction
    );
    assert!(library.effective("Headphones").bass_db > 0.0);
}
#[test]
fn baseline_profile_remains_backward_compatible() {
    let p: MusicProfile = serde_json::from_str("{}").unwrap();
    assert_eq!(p, MusicProfile::default());
    assert!(serde_json::from_str::<MusicProfile>("{\"unknown\":1}").is_err());
}
