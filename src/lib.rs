//! Maris library. New code imports modules through their owning domain.

pub mod analysis;
pub mod audio;
pub mod cli;
pub mod control;
pub mod devices;
pub mod dsp;
pub mod i18n;
pub mod mixer;
pub mod models;
pub mod presets;
pub mod tuning;
pub mod ui;

// Compatibility exports retain existing callers without duplicate implementations.
#[doc(hidden)]
pub use analysis::context as music_context;
#[doc(hidden)]
pub use analysis::spectrum as visualizer;
#[doc(hidden)]
pub use audio::offline;
#[doc(hidden)]
pub use audio::platform;
#[doc(hidden)]
pub use cli::mixer as mixer_cli;
#[doc(hidden)]
pub use cli::sound as sound_cli;
#[doc(hidden)]
pub use control::mcp;
#[doc(hidden)]
pub use control::store;
#[doc(hidden)]
pub use devices::autoeq;
#[doc(hidden)]
pub use devices::capability as device_profile;
#[doc(hidden)]
pub use devices::identity as device_identity;
#[doc(hidden)]
pub use dsp::music;
#[doc(hidden)]
pub use dsp::profile;
#[doc(hidden)]
pub use dsp::resample;
#[doc(hidden)]
pub use dsp::tone;
#[doc(hidden)]
pub use models as neural;
#[doc(hidden)]
pub use presets::scenes;
#[doc(hidden)]
pub use tuning::eq as smart;
#[doc(hidden)]
pub use tuning::planner as music_tuning;
#[doc(hidden)]
pub use tuning::preferences as listening;
#[doc(hidden)]
pub use ui::desktop;
#[doc(hidden)]
pub use ui::desktop::controls as desktop_controls;
#[doc(hidden)]
pub use ui::theme;
#[doc(hidden)]
pub use ui::tui;
#[doc(hidden)]
pub use ui::tui::dashboard;
#[doc(hidden)]
pub use ui::tui::input as control_panel;
#[doc(hidden)]
pub use ui::tui::inspector;
#[doc(hidden)]
pub use ui::tui::monitor as monitor_view;
#[doc(hidden)]
pub use ui::tui::music as music_view;
#[doc(hidden)]
pub use ui::tui::presets as preset_view;
#[doc(hidden)]
pub use ui::tui::settings as configuration;
#[doc(hidden)]
pub use ui::tui::studio as studio_view;
#[doc(hidden)]
pub use ui::tui::studio::controls as studio_controls;
#[doc(hidden)]
pub use ui::tui::view as tui_view;
