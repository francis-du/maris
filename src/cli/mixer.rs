//! Mixer configuration and explicitly authorized multi-device sessions.
use crate::{control::store::Store, mixer};
use anyhow::{ensure, Result};
use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum Action {
    /// Show the saved mixer topology and current capabilities.
    Status,
    /// Show platform mixer and routing capabilities without changing state.
    Capabilities,
    /// Start the configured inputs and output buses; never changes OS volume or defaults.
    Run {
        #[arg(long)]
        /// Authorize temporary audio capture and routing.
        accept_routing: bool,
        #[arg(long, default_value_t = 48000)]
        /// Mixer processing sample rate in Hz.
        rate: u32,
        #[arg(long)]
        /// Stop automatically after this many seconds.
        seconds: Option<u64>,
    },
    /// Apply an EQ preset to one strip without changing device correction.
    Eq {
        /// Mixer input strip ID.
        id: String,
        /// Included EQ preset name or stable ID.
        preset: String,
    },
    /// Adjust one strip EQ band (1 through 10).
    Band {
        /// Mixer input strip ID.
        id: String,
        /// EQ band number, from 1 through 10.
        index: usize,
        #[arg(allow_hyphen_values = true)]
        /// Gain in dB.
        gain: f64,
    },
    /// Configure the optional compressor on a single strip; there is no makeup gain.
    Compressor {
        /// Mixer input strip ID.
        id: String,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Enable or disable this processor.
        enabled: Option<bool>,
        #[arg(long, allow_hyphen_values = true)]
        /// Detection threshold in dB.
        threshold: Option<f64>,
        #[arg(long)]
        /// Compression ratio.
        ratio: Option<f64>,
    },
    /// Add a mixer input strip from a device or application source.
    Add {
        /// Mixer input strip ID.
        id: String,
        #[arg(long)]
        /// Display name for the input strip.
        name: Option<String>,
        #[arg(long)]
        /// Input device or application source selector.
        device: Option<String>,
    },
    /// Remove one mixer input strip.
    Remove {
        /// Mixer input strip ID.
        id: String,
    },
    /// Change gain, pan, sends, mute/solo, ducking, or speech cleanup on one strip.
    Set {
        /// Mixer input strip ID.
        id: String,
        #[arg(long, allow_hyphen_values = true)]
        /// Gain in dB.
        gain: Option<f64>,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Mute this strip or bus: true or false.
        mute: Option<bool>,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Solo this strip: true or false.
        solo: Option<bool>,
        #[arg(long, allow_hyphen_values = true)]
        /// Stereo pan; negative favors left, positive favors right.
        pan: Option<f64>,
        #[arg(long)]
        /// Linear send amount to output bus A.
        send_a: Option<f64>,
        #[arg(long)]
        /// Linear send amount to output bus B.
        send_b: Option<f64>,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Use this strip to trigger voice ducking.
        voice_trigger: Option<bool>,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Attenuate this strip when voice ducking triggers.
        duck_target: Option<bool>,
        #[arg(long, action = clap::ArgAction::Set, help = "Enable speech-only RNNoise for this strip")]
        /// Enable speech-only RNNoise for this strip.
        speech_denoise: Option<bool>,
    },
    /// Configure one output bus, including device, gain, mute, and delay.
    Bus {
        /// Output bus number: 1 for A, 2 for B.
        index: usize,
        #[arg(long)]
        /// Device name or stable device identifier.
        device: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        /// Gain in dB.
        gain: Option<f64>,
        #[arg(long, action = clap::ArgAction::Set)]
        /// Mute this strip or bus: true or false.
        mute: Option<bool>,
        #[arg(long, help = "Additional bus delay in milliseconds (0..500)")]
        /// Additional bus delay in milliseconds.
        delay_ms: Option<f64>,
    },
    /// Configure voice-triggered ducking for the mixer.
    Duck {
        #[arg(long, action = clap::ArgAction::Set)]
        /// Enable or disable this processor.
        enabled: Option<bool>,
        #[arg(long, allow_hyphen_values = true)]
        /// Detection threshold in dB.
        threshold: Option<f64>,
        #[arg(long)]
        /// Ducking attenuation in dB.
        attenuation: Option<f64>,
    },
    /// Save the current mixer topology and strip settings as a named scene.
    SceneSave {
        /// Mixer scene name.
        name: String,
    },
    /// Restore a previously saved mixer scene.
    SceneRestore {
        /// Mixer scene name.
        name: String,
    },
    /// Undo the most recent mixer configuration change.
    Undo,
}

pub fn run(store: &Store, expected: Option<u64>, action: Action) -> Result<Value> {
    match action {
        Action::Status => Ok(json!({
            "mixer": mixer::load(store)?,
            "capabilities": mixer::capabilities()
        })),
        Action::Capabilities => Ok(mixer::capabilities()),
        Action::Run {
            accept_routing,
            rate,
            seconds,
        } => {
            ensure!(accept_routing, "Mixer capture requires --accept-routing");
            if let Some(expected) = expected {
                ensure!(
                    mixer::load(store)?.revision == expected,
                    "Mixer revision conflict"
                );
            }
            crate::audio::mix::run(store.clone(), rate, true, seconds)
        }
        Action::Eq { id, preset } => Ok(serde_json::to_value(mixer::edit(
            store,
            expected,
            |config| {
                let strip = config
                    .strips
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Unknown mixer strip"))?;
                let current = strip.eq.clone().unwrap_or_default();
                strip.eq = Some(crate::presets::apply_tone_curve(&current, &preset, 48_000)?);
                Ok(())
            },
        )?)?),
        Action::Band { id, index, gain } => {
            ensure!((1..=10).contains(&index), "Band index must be 1..10");
            Ok(serde_json::to_value(mixer::edit(
                store,
                expected,
                |config| {
                    let strip = config
                        .strips
                        .iter_mut()
                        .find(|s| s.id == id)
                        .ok_or_else(|| anyhow::anyhow!("Unknown mixer strip"))?;
                    let profile = strip
                        .eq
                        .get_or_insert_with(crate::dsp::profile::Profile::default);
                    profile.bands[index - 1].gain_db = gain;
                    profile.name = "custom".into();
                    Ok(())
                },
            )?)?)
        }
        Action::Compressor {
            id,
            enabled,
            threshold,
            ratio,
        } => {
            ensure!(
                enabled.is_some() || threshold.is_some() || ratio.is_some(),
                "Specify a compressor change"
            );
            Ok(serde_json::to_value(mixer::edit(
                store,
                expected,
                |config| {
                    let strip = config
                        .strips
                        .iter_mut()
                        .find(|s| s.id == id)
                        .ok_or_else(|| anyhow::anyhow!("Unknown mixer strip"))?;
                    if let Some(value) = enabled {
                        strip.compressor.enabled = value;
                    }
                    if let Some(value) = threshold {
                        strip.compressor.threshold_db = value;
                    }
                    if let Some(value) = ratio {
                        strip.compressor.ratio = value;
                    }
                    Ok(())
                },
            )?)?)
        }
        Action::Add { id, name, device } => Ok(serde_json::to_value(mixer::edit(
            store,
            expected,
            |config| {
                ensure!(
                    !config.strips.iter().any(|strip| strip.id == id),
                    "Mixer strip id already exists"
                );
                config.strips.push(mixer::MixerStrip {
                    id: id.clone(),
                    name: name.clone().unwrap_or_else(|| id.clone()),
                    source_device: device.clone(),
                    ..mixer::MixerStrip::default()
                });
                Ok(())
            },
        )?)?),
        Action::Remove { id } => Ok(serde_json::to_value(mixer::edit(
            store,
            expected,
            |config| {
                let before = config.strips.len();
                config.strips.retain(|strip| strip.id != id);
                ensure!(config.strips.len() != before, "Unknown mixer strip");
                Ok(())
            },
        )?)?),
        Action::Set {
            id,
            gain,
            mute,
            solo,
            pan,
            send_a,
            send_b,
            voice_trigger,
            duck_target,
            speech_denoise,
        } => Ok(serde_json::to_value(mixer::edit(
            store,
            expected,
            |config| {
                let strip = config
                    .strips
                    .iter_mut()
                    .find(|strip| strip.id == id)
                    .ok_or_else(|| anyhow::anyhow!("Unknown mixer strip"))?;
                if let Some(value) = gain {
                    strip.gain_db = value;
                }
                if let Some(value) = mute {
                    strip.mute = value;
                }
                if let Some(value) = solo {
                    strip.solo = value;
                }
                if let Some(value) = pan {
                    strip.pan = value;
                }
                if let Some(value) = send_a {
                    strip.sends[0] = value;
                }
                if let Some(value) = send_b {
                    strip.sends[1] = value;
                }
                if let Some(value) = voice_trigger {
                    strip.voice_trigger = value;
                }
                if let Some(value) = duck_target {
                    strip.duck_target = value;
                }
                if let Some(value) = speech_denoise {
                    strip.speech_denoise = value;
                }
                Ok(())
            },
        )?)?),
        Action::Bus {
            index,
            device,
            gain,
            mute,
            delay_ms,
        } => {
            ensure!(
                (1..=mixer::BUS_COUNT).contains(&index),
                "Bus index must be 1 or 2"
            );
            Ok(serde_json::to_value(mixer::edit(
                store,
                expected,
                |config| {
                    let bus = &mut config.buses[index - 1];
                    if let Some(value) = &device {
                        bus.output_device = if value == "none" {
                            None
                        } else {
                            Some(value.clone())
                        };
                    }
                    if let Some(value) = gain {
                        bus.gain_db = value;
                    }
                    if let Some(value) = mute {
                        bus.mute = value;
                    }
                    if let Some(value) = delay_ms {
                        bus.delay_ms = value;
                    }
                    Ok(())
                },
            )?)?)
        }
        Action::Duck {
            enabled,
            threshold,
            attenuation,
        } => Ok(serde_json::to_value(mixer::edit(
            store,
            expected,
            |config| {
                if let Some(value) = enabled {
                    config.ducking.enabled = value;
                }
                if let Some(value) = threshold {
                    config.ducking.threshold_dbfs = value;
                }
                if let Some(value) = attenuation {
                    config.ducking.attenuation_db = value;
                }
                Ok(())
            },
        )?)?),
        Action::SceneSave { name } => Ok(serde_json::to_value(mixer::save_scene_checked(
            store, expected, &name,
        )?)?),
        Action::SceneRestore { name } => Ok(serde_json::to_value(mixer::restore_scene_checked(
            store, expected, &name,
        )?)?),
        Action::Undo => Ok(serde_json::to_value(mixer::undo(store, expected)?)?),
    }
}
