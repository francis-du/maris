use maris::{
    dsp::{music::MusicProfile, profile::Profile, Processor, Settings},
    mixer::{MixerConfig, MixerStrip},
};
use std::any::TypeId;

#[test]
fn old_and_domain_paths_reference_the_same_public_types() {
    assert_eq!(
        TypeId::of::<Profile>(),
        TypeId::of::<maris::profile::Profile>()
    );
    assert_eq!(
        TypeId::of::<MusicProfile>(),
        TypeId::of::<maris::music::MusicProfile>()
    );
    assert_eq!(
        TypeId::of::<maris::control::store::Store>(),
        TypeId::of::<maris::store::Store>()
    );
    assert_eq!(
        TypeId::of::<maris::devices::identity::Identity>(),
        TypeId::of::<maris::device_identity::Identity>()
    );
    assert_eq!(
        TypeId::of::<maris::devices::capability::Capability>(),
        TypeId::of::<maris::device_profile::Capability>()
    );
    assert_eq!(
        TypeId::of::<maris::analysis::context::MusicContext>(),
        TypeId::of::<maris::music_context::MusicContext>()
    );
    assert_eq!(
        TypeId::of::<maris::tuning::preferences::Library>(),
        TypeId::of::<maris::listening::Library>()
    );
    assert_eq!(
        TypeId::of::<maris::ui::tui::settings::Editor>(),
        TypeId::of::<maris::configuration::Editor>()
    );
    assert_eq!(
        TypeId::of::<maris::ui::desktop::controls::Controller>(),
        TypeId::of::<maris::desktop_controls::Controller>()
    );
}

#[test]
fn domain_imports_execute_the_existing_dsp_and_mixer() {
    let mut processor = Processor::new(Settings::compile(&Profile::default(), 48_000).unwrap());
    let frame = processor.process([0.2, -0.2]);
    assert!((frame[0] - 0.2).abs() < 1e-6);
    assert!((frame[1] + 0.2).abs() < 1e-6);
    let mut mixer = maris::mixer::engine::Processor::new(
        MixerConfig {
            strips: vec![MixerStrip::default()],
            ..MixerConfig::default()
        },
        48_000,
    )
    .unwrap();
    let mixed = mixer.process(&[frame]);
    assert!((mixed[0][0] - 0.2).abs() < 1e-6);
    assert_eq!(mixed[1], [0.0; 2]);
}
