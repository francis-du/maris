//! Deterministic multi-strip / dual-bus mixer core and persistent control state.
//! Device stream ownership lives in audio::mix; state edits never start capture.
pub mod engine;
use crate::control::store::{self, Store, MAX_JSON_BYTES};
use anyhow::{ensure, Context, Result};
pub use engine::Processor;
use fs2::FileExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
};

pub const MAX_STRIPS: usize = 16;
pub const BUS_COUNT: usize = 2;
const CEILING: f64 = 0.8912509381337456;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MixerStrip {
    pub id: String,
    pub name: String,
    pub source_device: Option<String>,
    pub gain_db: f64,
    pub mute: bool,
    pub solo: bool,
    pub pan: f64,
    pub sends: [f64; BUS_COUNT],
    pub voice_trigger: bool,
    pub duck_target: bool,
    /// Speech-only RNNoise worker. Never enable this for music strips.
    pub speech_denoise: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eq: Option<crate::dsp::profile::Profile>,
    #[serde(default)]
    pub compressor: crate::dsp::music::Compressor,
}
impl Default for MixerStrip {
    fn default() -> Self {
        Self {
            id: "channel".into(),
            name: "Channel".into(),
            source_device: None,
            gain_db: 0.0,
            mute: false,
            solo: false,
            pan: 0.0,
            sends: [1.0, 0.0],
            voice_trigger: false,
            duck_target: true,
            speech_denoise: false,
            eq: None,
            compressor: crate::dsp::music::Compressor::default(),
        }
    }
}
impl MixerStrip {
    fn validate(&self) -> Result<()> {
        validate_label(&self.id, 64, "strip id")?;
        validate_label(&self.name, 128, "strip name")?;
        if let Some(device) = &self.source_device {
            validate_label(device, 256, "source device")?;
        }
        if let Some(eq) = &self.eq {
            eq.validate()?;
        }
        ensure!(
            !self.speech_denoise || self.source_device.is_some(),
            "Speech denoise requires an assigned mixer source"
        );
        self.compressor.validate()?;
        bounded(self.gain_db, -60.0, 12.0, "strip gain_db")?;
        bounded(self.pan, -1.0, 1.0, "strip pan")?;
        for send in self.sends {
            bounded(send, 0.0, 1.0, "bus send")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct OutputBus {
    pub id: String,
    pub name: String,
    pub output_device: Option<String>,
    pub gain_db: f64,
    pub mute: bool,
    /// Additional output delay for aligning independent buses.
    pub delay_ms: f64,
}
impl Default for OutputBus {
    fn default() -> Self {
        Self {
            id: "a".into(),
            name: "Bus A".into(),
            output_device: None,
            gain_db: 0.0,
            mute: false,
            delay_ms: 0.0,
        }
    }
}
impl OutputBus {
    fn validate(&self) -> Result<()> {
        validate_label(&self.id, 32, "bus id")?;
        validate_label(&self.name, 128, "bus name")?;
        if let Some(device) = &self.output_device {
            validate_label(device, 256, "output device")?;
        }
        bounded(self.gain_db, -60.0, 6.0, "bus gain_db")?;
        bounded(self.delay_ms, 0.0, 500.0, "bus delay_ms")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Ducking {
    pub enabled: bool,
    pub threshold_dbfs: f64,
    pub attenuation_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
}
impl Default for Ducking {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_dbfs: -32.0,
            attenuation_db: 9.0,
            attack_ms: 25.0,
            release_ms: 350.0,
        }
    }
}
impl Ducking {
    fn validate(self) -> Result<()> {
        bounded(self.threshold_dbfs, -80.0, 0.0, "duck threshold_dbfs")?;
        bounded(self.attenuation_db, 0.0, 30.0, "duck attenuation_db")?;
        bounded(self.attack_ms, 1.0, 500.0, "duck attack_ms")?;
        bounded(self.release_ms, 20.0, 5000.0, "duck release_ms")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MixerConfig {
    pub strips: Vec<MixerStrip>,
    pub buses: [OutputBus; BUS_COUNT],
    pub ducking: Ducking,
}
impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            strips: Vec::new(),
            buses: [
                OutputBus::default(),
                OutputBus {
                    id: "b".into(),
                    name: "Bus B".into(),
                    ..OutputBus::default()
                },
            ],
            ducking: Ducking::default(),
        }
    }
}
impl MixerConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.strips.len() <= MAX_STRIPS,
            "At most {MAX_STRIPS} mixer strips are supported"
        );
        let mut ids = BTreeSet::new();
        for strip in &self.strips {
            strip.validate()?;
            ensure!(ids.insert(strip.id.as_str()), "Duplicate mixer strip id");
        }
        ensure!(
            self.buses[0].id != self.buses[1].id,
            "Output bus ids must be distinct"
        );
        for bus in &self.buses {
            bus.validate()?;
        }
        self.ducking.validate()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MixerState {
    pub schema_version: u32,
    pub revision: u64,
    pub config: MixerConfig,
    pub previous: Option<MixerConfig>,
    pub scenes: BTreeMap<String, MixerConfig>,
}
impl Default for MixerState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            config: MixerConfig::default(),
            previous: None,
            scenes: BTreeMap::new(),
        }
    }
}
impl MixerState {
    fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "Unsupported mixer schema version");
        self.config.validate()?;
        if let Some(previous) = &self.previous {
            previous.validate()?;
        }
        ensure!(
            self.scenes.len() <= 32,
            "At most 32 mixer scenes are supported"
        );
        for (name, scene) in &self.scenes {
            validate_label(name, 80, "scene name")?;
            scene.validate()?;
        }
        Ok(())
    }
}

pub fn load(store: &Store) -> Result<MixerState> {
    let path = store.directory.join("mixer.json");
    if !path.exists() {
        return Ok(MixerState::default());
    }
    let state: MixerState = store::read_json(&path)?;
    state.validate()?;
    Ok(state)
}

pub fn edit(
    store: &Store,
    expected: Option<u64>,
    change: impl FnOnce(&mut MixerConfig) -> Result<()>,
) -> Result<MixerState> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("mixer.lock"))?;
    lock.lock_exclusive()?;
    let mut state = load(store)?;
    if let Some(expected) = expected {
        ensure!(
            state.revision == expected,
            "Mixer revision conflict: expected {expected}, current {}",
            state.revision
        );
    }
    let previous = state.config.clone();
    change(&mut state.config)?;
    state.config.validate()?;
    if state.config == previous {
        return Ok(state);
    }
    state.previous = Some(previous);
    state.revision = state
        .revision
        .checked_add(1)
        .context("Mixer revision overflow")?;
    ensure!(
        serde_json::to_vec_pretty(&state)?.len() as u64 <= MAX_JSON_BYTES,
        "Mixer state exceeds 64 KiB"
    );
    store.write_json("mixer.json", &state)?;
    Ok(state)
}

pub fn undo(store: &Store, expected: Option<u64>) -> Result<MixerState> {
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("mixer.lock"))?;
    lock.lock_exclusive()?;
    let mut state = load(store)?;
    if let Some(expected) = expected {
        ensure!(state.revision == expected, "Mixer revision conflict");
    }
    let previous = state
        .previous
        .take()
        .context("Nothing to restore in mixer")?;
    state.previous = Some(std::mem::replace(&mut state.config, previous));
    state.revision = state
        .revision
        .checked_add(1)
        .context("Mixer revision overflow")?;
    state.validate()?;
    store.write_json("mixer.json", &state)?;
    Ok(state)
}

pub fn save_scene(store: &Store, name: &str) -> Result<MixerState> {
    save_scene_checked(store, None, name)
}

pub fn save_scene_checked(store: &Store, expected: Option<u64>, name: &str) -> Result<MixerState> {
    validate_label(name, 80, "scene name")?;
    std::fs::create_dir_all(&store.directory)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.directory.join("mixer.lock"))?;
    lock.lock_exclusive()?;
    let mut state = load(store)?;
    if let Some(expected) = expected {
        ensure!(state.revision == expected, "Mixer revision conflict");
    }
    if state.scenes.get(name) == Some(&state.config) {
        return Ok(state);
    }
    state.scenes.insert(name.to_owned(), state.config.clone());
    // A scene edit invalidates stale writers without replacing the last audio undo.
    state.revision = state
        .revision
        .checked_add(1)
        .context("Mixer revision overflow")?;
    state.validate()?;
    ensure!(
        serde_json::to_vec_pretty(&state)?.len() as u64 <= MAX_JSON_BYTES,
        "Mixer state exceeds 64 KiB"
    );
    store.write_json("mixer.json", &state)?;
    Ok(state)
}

pub fn restore_scene(store: &Store, name: &str) -> Result<MixerState> {
    restore_scene_checked(store, None, name)
}

pub fn restore_scene_checked(
    store: &Store,
    expected: Option<u64>,
    name: &str,
) -> Result<MixerState> {
    let state = load(store)?;
    if let Some(expected) = expected {
        ensure!(state.revision == expected, "Mixer revision conflict");
    }
    let scene = state
        .scenes
        .get(name)
        .cloned()
        .context("Unknown mixer scene")?;
    edit(store, Some(state.revision), |config| {
        *config = scene;
        Ok(())
    })
}

pub fn capabilities() -> Value {
    let application_audio = cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    ));
    json!({
        "matrix_engine": true,
        "strip_gain_mute_solo_pan": true,
        "dual_bus_routing_engine": true,
        "voice_trigger_ducking_engine": true,
        "scene_persistence": true,
        "simultaneous_hardware_inputs": true,
        "simultaneous_hardware_outputs": true,
        "selected_application_capture": application_audio,
        "selected_application_shared_route": application_audio,
        "independent_application_strips": application_audio,
        "per_strip_eq": true,
        "per_strip_compression": true,
        "per_strip_neural_denoise": cfg!(feature = "neural"),
        "application_volume": application_audio,
        "latency_compensation": true,
        "cross_device_clock_sync": true
    })
}

fn bounded(value: f64, min: f64, max: f64, name: &str) -> Result<()> {
    ensure!(
        value.is_finite() && (min..=max).contains(&value),
        "{name} must be finite and in [{min}, {max}]"
    );
    Ok(())
}
fn validate_label(value: &str, max: usize, name: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control),
        "Invalid {name}"
    );
    Ok(())
}
