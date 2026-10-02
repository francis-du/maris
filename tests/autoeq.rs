use maris::{autoeq, device_profile, music::MusicProfile};

const INDEX: &str = r#"
- [Sony WH-1000XM5](./oratory1990/over-ear/Sony%20WH-1000XM5)
- [Sony WH-1000XM5 (ANC On)](./Example/over-ear/Sony%20WH-1000XM5%20(ANC%20On))
- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650)
- [Apple AirPods Pro 2](./Example/in-ear/Apple%20AirPods%20Pro%202)
"#;

#[test]
fn parses_autoeq_recommended_index() {
    let entries = autoeq::parse_index(INDEX);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].source, "oratory1990");
    assert_eq!(entries[0].form_factor, "over-ear");
}

#[test]
fn exact_unique_device_name_can_auto_match() {
    let entries = autoeq::parse_index(INDEX);
    let matched = autoeq::match_device("Sennheiser HD 650", &entries);
    assert!(matched.auto_apply);
    assert_eq!(matched.candidate.unwrap().name, "Sennheiser HD 650");
}

#[test]
fn generic_or_ambiguous_device_name_never_auto_applies() {
    let entries = autoeq::parse_index(INDEX);
    assert!(!autoeq::match_device("Headphones", &entries).auto_apply);
    let sony = autoeq::match_device("Sony WH-1000XM5", &entries);
    assert!(
        !sony.auto_apply,
        "variant ambiguity must require confirmation"
    );
}

#[test]
fn bundled_autoeq_index_is_large_and_contains_known_models() {
    let entries = autoeq::catalog().unwrap();
    assert!(
        entries.len() > 5000,
        "bundled recommended AutoEq index unexpectedly small"
    );
    let matched = autoeq::match_device("Sennheiser HD 650", &entries);
    let entry = matched
        .candidate
        .expect("HD 650 should exist in bundled AutoEq");
    assert_eq!(entry.name, "Sennheiser HD 650");
    assert_eq!(
        autoeq::AUTOEQ_COMMIT,
        "7ae0f56d53074872b028649617a22bbb4232feb7"
    );
}

#[test]
fn bundled_profile_pack_contains_known_profile() {
    if std::env::var("MARIS_BUNDLE_AUTOEQ_PROFILES").as_deref() != Ok("1") {
        return;
    }
    assert!(autoeq::bundled_profiles_available());
    let entry = autoeq::catalog()
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == "Sennheiser HD 650")
        .expect("bundled AutoEq catalog contains HD 650");
    let profile = autoeq::bundled_profile(&entry).unwrap();
    assert!(profile.correction.len() >= 5);
    assert!(profile.correction_preamp_db <= 0.0);
    assert!(profile
        .correction_source
        .as_deref()
        .is_some_and(|source| source.contains("AutoEq@")));
}

#[test]
fn cached_profile_can_be_reused_offline_and_bound_manually() {
    let temp = tempfile::tempdir().unwrap();
    let store = maris::store::Store::at(temp.path());
    let entry = autoeq::Entry {
        name: "Fixture Headphone".into(),
        path: "./fixture/over-ear/Fixture Headphone".into(),
        source: "fixture".into(),
        form_factor: "over-ear".into(),
    };
    let mut profile = MusicProfile {
        correction_source: Some("fixture measurement".into()),
        correction_preamp_db: -3.0,
        ..MusicProfile::default()
    };
    profile.correction.push(maris::tone::Filter {
        kind: maris::tone::Kind::Peak,
        frequency_hz: 120.0,
        gain_db: 2.0,
        q: 1.0,
    });
    autoeq::cache_profile(&store, &entry, &profile).unwrap();
    let cached = autoeq::load_cached_profile(&store, &entry).unwrap();
    assert_eq!(cached.correction_source, profile.correction_source);
    let library = autoeq::bind_entry(&store, "USB Headphones", &entry).unwrap();
    assert_eq!(
        library.effective("USB Headphones").correction_source,
        profile.correction_source
    );
    let capability = device_profile::effective(&store, "USB Headphones").unwrap();
    assert_eq!(capability.model.as_deref(), Some("Fixture Headphone"));
    assert_eq!(capability.correction_source, profile.correction_source);
    assert!(capability.match_confirmed);
}

#[test]
fn candidate_match_does_not_claim_correction_before_apply() {
    let temp = tempfile::tempdir().unwrap();
    let store = maris::store::Store::at(temp.path());
    let entries = autoeq::parse_index(INDEX);
    let matched = autoeq::match_device("Sennheiser HD 650", &entries);
    device_profile::remember_match(&store, "Sennheiser HD 650", &matched, false).unwrap();
    let capability = device_profile::effective(&store, "Sennheiser HD 650").unwrap();
    assert_eq!(capability.model.as_deref(), Some("Sennheiser HD 650"));
    assert!(capability.correction_source.is_none());
}

#[test]
fn capability_constraints_do_not_modify_measurement_correction() {
    let mut profile = MusicProfile {
        bass_db: 6.0,
        air_db: 3.0,
        ..MusicProfile::default()
    };
    profile.correction.push(maris::tone::Filter {
        kind: maris::tone::Kind::Peak,
        frequency_hz: 100.0,
        gain_db: 8.0,
        q: 1.0,
    });
    let capability = device_profile::Capability {
        max_preference_boost_db: 1.5,
        virtual_bass_allowed: false,
        ..device_profile::Capability::default()
    };
    let effective = device_profile::apply_constraints(&profile, &capability);
    assert_eq!(effective.bass_db, 1.5);
    assert_eq!(effective.air_db, 1.5);
    assert_eq!(effective.correction[0].gain_db, 8.0);
}
