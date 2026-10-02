//! Original listening-scene preferences. These are not measurements or neural models.
//! No scene changes system volume, routing, correction, channel balance or A/B policy.
use crate::{devices::capability::Capability, dsp::music::MusicProfile};
use anyhow::{ensure, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Scene {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub bass_db: f64,
    pub presence_db: f64,
    pub air_db: f64,
    pub softness: f64,
    pub adaptive_strength: f64,
    pub width: f64,
    pub compression: bool,
    pub virtual_bass: f64,
}

pub const SCENES: [Scene; 10] = [
    Scene {
        id: "focus", name: "Focus", category: "work",
        description: "Subtle bass and treble cuts for background listening; no compression.",
        bass_db: -0.5, presence_db: -0.25, air_db: -0.5, softness: 0.3,
        adaptive_strength: 0.35, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "long-listening", name: "Long listening", category: "work",
        description: "A softer high end and bounded dynamic cuts; not a hearing-protection mode.",
        bass_db: 0.0, presence_db: -0.5, air_db: -0.75, softness: 0.5,
        adaptive_strength: 0.55, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "dialogue", name: "Dialogue", category: "speech",
        description: "Less bass and a small presence lift for dialogue and podcasts; no denoising.",
        bass_db: -1.0, presence_db: 1.0, air_db: -0.25, softness: 0.2,
        adaptive_strength: 0.4, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "cinema", name: "Cinema", category: "film",
        description: "Modest weight and dialogue presence with original stereo dynamics; not surround sound.",
        bass_db: 0.75, presence_db: 0.5, air_db: 0.0, softness: 0.2,
        adaptive_strength: 0.45, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "night-dialogue", name: "Night dialogue", category: "film",
        description: "Reduces bass and loud peaks for late-night dialogue; enables compression without makeup gain.",
        bass_db: -1.5, presence_db: 0.75, air_db: -0.5, softness: 0.45,
        adaptive_strength: 0.5, width: 1.0, compression: true, virtual_bass: 0.0,
    },
    Scene {
        id: "acoustic-listening", name: "Acoustic listening", category: "music",
        description: "A restrained presence and air lift for acoustic recordings; keeps compression off.",
        bass_db: 0.0, presence_db: 0.25, air_db: 0.5, softness: 0.1,
        adaptive_strength: 0.25, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "orchestral", name: "Orchestral", category: "music",
        description: "Neutral static tone and very light dynamic control; preserves stereo width and dynamics.",
        bass_db: 0.0, presence_db: 0.0, air_db: 0.0, softness: 0.0,
        adaptive_strength: 0.15, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "tight-bass", name: "Tight bass", category: "music",
        description: "Cuts excess bass preference and strengthens bounded dynamic control; never adds a bass boost.",
        bass_db: -1.0, presence_db: 0.0, air_db: 0.0, softness: 0.2,
        adaptive_strength: 0.7, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "game-clarity", name: "Game clarity", category: "games",
        description: "Less bass masking with a small presence lift; no positional-audio or surround claim.",
        bass_db: -0.75, presence_db: 0.75, air_db: 0.0, softness: 0.25,
        adaptive_strength: 0.5, width: 1.0, compression: false, virtual_bass: 0.0,
    },
    Scene {
        id: "small-speakers", name: "Small speakers", category: "device",
        description: "No sub-bass boost; mild virtual bass only when device capability and bass limits are known.",
        bass_db: 0.0, presence_db: 0.25, air_db: -0.25, softness: 0.25,
        adaptive_strength: 0.55, width: 1.0, compression: false, virtual_bass: 0.18,
    },
];

pub fn find(id: &str) -> Option<&'static Scene> {
    let id = id.strip_prefix("scene:").unwrap_or(id);
    SCENES.iter().find(|scene| scene.id == id)
}

/// Requested parameters only; prepare() applies device constraints and retained correction.
pub fn profile(id: &str) -> Result<MusicProfile> {
    let scene = find(id).ok_or_else(|| anyhow::anyhow!("Unknown listening scene: {id}"))?;
    let mut profile = MusicProfile {
        bass_db: scene.bass_db,
        presence_db: scene.presence_db,
        air_db: scene.air_db,
        softness: scene.softness,
        width: scene.width,
        ..MusicProfile::default()
    };
    profile.adaptive.strength = scene.adaptive_strength;
    profile.adaptive.enabled = scene.adaptive_strength > 0.0;
    profile.compressor.enabled = scene.compression;
    if scene.compression {
        profile.compressor.threshold_db = -24.0;
        profile.compressor.ratio = 2.0;
        profile.compressor.attack_ms = 15.0;
        profile.compressor.release_ms = 250.0;
    }
    profile.bass_assist.enabled = scene.virtual_bass > 0.0;
    profile.bass_assist.amount = scene.virtual_bass;
    profile.validate()?;
    Ok(profile)
}

#[derive(Clone, Debug, Serialize)]
pub struct Preview {
    pub id: String,
    pub profile: MusicProfile,
    pub changes: Vec<String>,
    pub device_constraints_applied: bool,
    pub compression_enabled: bool,
    pub correction_preserved: bool,
}

/// Shared pure preview for CLI, TUI and MCP. No persistence, capture or inference.
pub fn prepare(before: &MusicProfile, capability: &Capability, id: &str) -> Result<Preview> {
    before.validate()?;
    capability.validate()?;
    let mut requested = MusicProfile::preset(id.strip_prefix("scene:").unwrap_or(id))?;
    requested.correction = before.correction.clone();
    requested.correction_preamp_db = before.correction_preamp_db;
    requested.correction_source = before.correction_source.clone();
    requested.highpass_hz = before.highpass_hz;
    requested.balance = before.balance;
    requested.level_match = before.level_match;
    let mut profile = crate::devices::capability::apply_constraints(&requested, capability);
    if find(id).is_some() && profile.bass_assist.enabled {
        profile.bass_assist.enabled = capability.virtual_bass_allowed
            && capability.device_class.contains("speaker")
            && capability.bass_floor_hz.is_some_and(|hz| hz >= 70.0);
    }
    profile.validate()?;
    ensure!(
        profile.correction == before.correction
            && profile.correction_source == before.correction_source
            && profile.correction_preamp_db == before.correction_preamp_db
            && profile.highpass_hz == before.highpass_hz,
        "A listening scene must preserve device correction"
    );
    Ok(Preview {
        id: id.to_owned(),
        changes: crate::tuning::planner::describe_changes(before, &profile),
        device_constraints_applied: profile != requested,
        compression_enabled: profile.compressor.enabled,
        correction_preserved: true,
        profile,
    })
}
