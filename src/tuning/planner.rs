//! Context-aware, explainable tuning proposals for device-specific music preferences.
//! Proposals never edit measurement correction and apply through the normal listening revision path.

use crate::{
    analysis::context::{MusicContext, Tag},
    analysis::{self, Analysis},
    control::store::Store,
    devices::capability::Capability,
    dsp::music::MusicProfile,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

pub const GOALS: [&str; 4] = ["balanced", "warm", "clear", "soft"];
pub const GOAL_LABELS: [&str; 4] = ["Balanced", "Warm", "Clear", "Soft"];

/// The UI and live planner share the same evidence gate; an active process is not
/// sufficient evidence for a useful recommendation.
pub fn preview_evidence(runtime: &serde_json::Value, now: u64) -> Result<Analysis> {
    ensure!(runtime["active"] == true, "Waiting for signal");
    ensure!(
        runtime["updated_at_ms"]
            .as_u64()
            .is_some_and(|time| now.checked_sub(time).is_some_and(|age| age < 1000)),
        "Awaiting telemetry"
    );
    let evidence: Analysis =
        serde_json::from_value(runtime["analysis"].clone()).context("Analysis is warming up")?;
    evidence.validate()?;
    ensure!(
        now.checked_sub(evidence.updated_at_ms)
            .is_some_and(|age| age < 6000),
        "Analysis is stale or from the future"
    );
    ensure!(
        runtime["sample_rate"].as_u64() == Some(u64::from(evidence.sample_rate)),
        "Analysis sample rate does not match the active output"
    );
    ensure!(
        evidence.frames >= u64::from(evidence.sample_rate) * 2,
        "At least two seconds of evidence are required"
    );
    ensure!(
        evidence.dropped_frames == 0 && evidence.rms_dbfs > -60.0,
        "Signal evidence is incomplete or too quiet"
    );
    Ok(evidence)
}

/// Semantic context must come from an available backend and a recent observation,
/// not a downloaded artifact or a stale object left by a previous session.
pub fn live_context(runtime: &serde_json::Value, evidence: &Analysis, now: u64) -> MusicContext {
    serde_json::from_value::<MusicContext>(runtime["music_context"].clone())
        .ok()
        .filter(|context| {
            runtime["music_context_status"]["semantic_backend_available"] == true
                && context.mode == "semantic"
                && context.confidence >= 0.5
                && context.validate().is_ok()
                && now
                    .checked_sub(context.updated_at_ms)
                    .is_some_and(|age| age <= 10_000)
        })
        .unwrap_or_else(|| MusicContext::signal_only(evidence))
}

/// Derive the displayed delta from the final constrained profile, never from a
/// pre-constraint intent. Boolean activation changes are audible changes too.
pub fn describe_changes(before: &MusicProfile, after: &MusicProfile) -> Vec<String> {
    let mut result = Vec::new();
    for (label, a, b) in [
        ("Music processing", before.enabled, after.enabled),
        ("Reference A", before.reference, after.reference),
        (
            "Dynamic EQ",
            before.adaptive.enabled,
            after.adaptive.enabled,
        ),
        (
            "Bass Assist",
            before.bass_assist.enabled,
            after.bass_assist.enabled,
        ),
        (
            "Compressor",
            before.compressor.enabled,
            after.compressor.enabled,
        ),
        ("Level match", before.level_match, after.level_match),
    ] {
        if a != b {
            result.push(format!(
                "{label}: {} -> {}",
                if a { "ON" } else { "OFF" },
                if b { "ON" } else { "OFF" }
            ));
        }
    }
    for (label, a, b, scale, unit) in [
        ("Bass", before.bass_db, after.bass_db, 1.0, " dB"),
        (
            "Presence",
            before.presence_db,
            after.presence_db,
            1.0,
            " dB",
        ),
        ("Air", before.air_db, after.air_db, 1.0, " dB"),
        ("Softness", before.softness, after.softness, 100.0, "%"),
        ("Intensity", before.intensity, after.intensity, 100.0, "%"),
        (
            "Dynamic EQ strength",
            before.adaptive.strength,
            after.adaptive.strength,
            100.0,
            "%",
        ),
        (
            "Bass Assist amount",
            before.bass_assist.amount,
            after.bass_assist.amount,
            100.0,
            "%",
        ),
        ("Stereo width", before.width, after.width, 100.0, "%"),
        ("Balance", before.balance, after.balance, 1.0, ""),
    ] {
        if (a - b).abs() > 1e-9 {
            result.push(format!(
                "{label}: {:+.2}{unit} -> {:+.2}{unit}",
                a * scale,
                b * scale
            ));
        }
    }
    result
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiveGuard {
    pub session_id: String,
    pub stable_id: Option<String>,
    pub device_binding_revision: Option<u64>,
    pub rebind_count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub algorithm: String,
    pub goal: String,
    pub device: String,
    pub profile_key: String,
    pub baseline_listening_revision: u64,
    pub created_at_ms: u64,
    pub evidence: Analysis,
    pub context: MusicContext,
    pub capability: Capability,
    pub effective_preamp_db: f64,
    pub before: MusicProfile,
    pub profile: MusicProfile,
    pub changes: Vec<String>,
    pub reasons: Vec<String>,
    pub limitations: Vec<String>,
    #[serde(default)]
    pub live_guard: Option<LiveGuard>,
}

fn bounded_add(value: f64, delta: f64, min: f64, max: f64) -> f64 {
    (value + delta).clamp(min, max)
}

fn semantic_family(context: &MusicContext) -> Option<String> {
    context
        .top_genre()
        .filter(|_| context.confidence >= 0.5)
        .map(|value| value.to_ascii_lowercase())
}

fn semantic_tagged(tags: &[Tag], terms: &[&str], minimum_confidence: f64) -> bool {
    tags.iter().any(|tag| {
        tag.confidence >= minimum_confidence
            && terms
                .iter()
                .any(|term| tag.label.to_ascii_lowercase().contains(term))
    })
}

pub struct TuningInput<'a> {
    pub device: &'a str,
    pub profile_key: &'a str,
    pub listening_revision: u64,
    pub before: &'a MusicProfile,
    pub evidence: &'a Analysis,
    pub context: &'a MusicContext,
    pub capability: &'a Capability,
    pub goal: &'a str,
    pub effective_preamp_db: f64,
}

pub fn propose_from(input: TuningInput<'_>) -> Result<Proposal> {
    let TuningInput {
        device,
        profile_key,
        listening_revision,
        before,
        evidence,
        context,
        capability,
        goal,
        effective_preamp_db,
    } = input;
    ensure!(GOALS.contains(&goal), "Unsupported tuning goal");
    ensure!(
        !device.is_empty() && device.len() <= 256 && !device.chars().any(char::is_control),
        "Invalid output device"
    );
    ensure!(
        !profile_key.is_empty()
            && profile_key.len() <= 256
            && !profile_key.chars().any(char::is_control),
        "Invalid output profile key"
    );
    before.validate()?;
    evidence.validate()?;
    context.validate()?;
    capability.validate()?;
    ensure!(
        evidence.frames >= evidence.sample_rate as u64 * 2,
        "At least two seconds of evidence are required"
    );
    ensure!(
        evidence.dropped_frames == 0 && evidence.rms_dbfs > -60.0,
        "Signal evidence is incomplete or too quiet"
    );
    ensure!(
        effective_preamp_db.is_finite() && (-60.0..=6.0).contains(&effective_preamp_db),
        "Invalid effective headroom"
    );

    let bass = evidence.band_energy_share[..3].iter().sum::<f64>();
    let presence = evidence.band_energy_share[5..7].iter().sum::<f64>();
    let treble = evidence.band_energy_share[7..].iter().sum::<f64>();
    let loud = evidence.momentary_lufs.is_some_and(|value| value > -13.0);
    let semantic = semantic_family(context);
    let semantic_is = |terms: &[&str]| {
        semantic
            .as_deref()
            .is_some_and(|value| terms.iter().any(|term| value.contains(term)))
    };
    let semantic_available = context.mode == "semantic" && context.confidence >= 0.5;
    let vocal_context = semantic_available
        && (context.vocal_probability.is_some_and(|value| value >= 0.65)
            || semantic_is(&["pop", "vocal", "r&b", "soul"])
            || semantic_tagged(&context.instruments, &["vocal", "voice", "singer"], 0.55));
    let bright_instrumentation = semantic_available
        && semantic_tagged(
            &context.instruments,
            &["cymbal", "hi-hat", "violin", "strings", "brass"],
            0.55,
        );
    let soft_mood = semantic_available
        && semantic_tagged(&context.mood, &["calm", "soft", "mellow", "ambient"], 0.60);
    let small_speaker = capability.device_class.contains("speaker");
    let headphone = capability.device_class.contains("headphone");
    let has_measurement = before.correction_source.is_some() && !before.correction.is_empty();
    let headroom_available = effective_preamp_db <= -0.75;

    let mut target = before.clone();
    let mut changes = Vec::new();
    let mut reasons = Vec::new();

    if target.reference {
        target.reference = false;
        changes.push("A/B: Reference A -> Enhanced B".into());
        reasons.push("Applying a tuning proposal must make the changed branch audible.".into());
    }
    if !target.enabled {
        target.enabled = true;
        changes.push("Music enhancement: disabled -> enabled".into());
    }

    if bass > 0.55 {
        if target.bass_db > 0.0 {
            let prior = target.bass_db;
            target.bass_db = bounded_add(target.bass_db, -0.5, -6.0, 6.0).max(0.0);
            changes.push(format!("Bass: {prior:+.2} -> {:+.2} dB", target.bass_db));
        }
        let prior = target.adaptive.strength;
        target.adaptive.enabled = true;
        target.adaptive.strength = bounded_add(prior, 0.10, 0.0, 1.0);
        if (prior - target.adaptive.strength).abs() > 1e-9 {
            changes.push(format!(
                "Dynamic EQ strength: {:.0}% -> {:.0}%",
                prior * 100.0,
                target.adaptive.strength * 100.0
            ));
        }
        reasons.push(format!(
            "Measured low-band energy is high ({bass:.2}); avoid adding a bass shelf and favor bounded dynamic control."
        ));
    } else {
        let wants_weight =
            goal == "warm" || semantic_is(&["electronic", "dance", "techno", "hip", "bass", "r&b"]);
        if wants_weight && bass < 0.30 {
            if small_speaker
                && capability.virtual_bass_allowed
                && capability.bass_floor_hz.is_some_and(|floor| floor >= 70.0)
            {
                let prior = target.bass_assist.amount;
                target.bass_assist.enabled = true;
                target.bass_assist.amount = bounded_add(prior, 0.10, 0.0, 1.0);
                changes.push(format!(
                    "Bass Assist: {:.0}% -> {:.0}%",
                    prior * 100.0,
                    target.bass_assist.amount * 100.0
                ));
                reasons.push(
                    "Known limited low-frequency extension favors bounded virtual-bass harmonics over a large sub-bass boost."
                        .into(),
                );
            } else if (headphone || !small_speaker) && has_measurement && headroom_available {
                let prior = target.bass_db;
                target.bass_db = bounded_add(target.bass_db, 0.5, -6.0, 6.0)
                    .min(capability.max_preference_boost_db);
                if (prior - target.bass_db).abs() > 1e-9 {
                    changes.push(format!("Bass: {prior:+.2} -> {:+.2} dB", target.bass_db));
                    reasons.push(
                        "Low measured bass energy, a bound correction profile, and available digital headroom permit a small preference boost."
                            .into(),
                    );
                }
            } else {
                reasons.push(
                    "Bass boost withheld because device correction/headroom or physical low-frequency capability is not established."
                        .into(),
                );
            }
        }
    }

    let harsh = treble > 0.16
        || (bright_instrumentation && treble > 0.12)
        || (loud && evidence.crest_db < 7.0);
    let semantic_softness = soft_mood && treble > 0.10;
    if harsh || goal == "soft" || semantic_softness {
        let prior = target.softness;
        target.softness = bounded_add(prior, 0.10, 0.0, 1.0);
        if (prior - target.softness).abs() > 1e-9 {
            changes.push(format!(
                "Softness: {:.0}% -> {:.0}%",
                prior * 100.0,
                target.softness * 100.0
            ));
        }
        target.adaptive.enabled = true;
        let prior = target.adaptive.strength;
        target.adaptive.strength = bounded_add(prior, 0.10, 0.0, 1.0);
        if (prior - target.adaptive.strength).abs() > 1e-9 {
            changes.push(format!(
                "Dynamic EQ strength: {:.0}% -> {:.0}%",
                prior * 100.0,
                target.adaptive.strength * 100.0
            ));
        }
        reasons.push(
            if bright_instrumentation && treble > 0.12 {
                "Measured treble density plus bright-instrument semantic context favors bounded dynamic de-harshing rather than static high-frequency boost."
            } else if semantic_softness {
                "Measured treble energy plus a sufficiently confident soft/calm semantic mood favors a small bounded softness increase."
            } else {
                "Treble density/loudness evidence favors bounded dynamic de-harshing rather than static high-frequency boost."
            }
            .into(),
        );
    } else if goal == "clear" && treble < 0.08 && !loud && headroom_available {
        let prior = target.air_db;
        target.air_db = bounded_add(target.air_db, 0.25, -3.0, 3.0)
            .min(capability.max_preference_boost_db.min(3.0));
        if (prior - target.air_db).abs() > 1e-9 {
            changes.push(format!("Air: {prior:+.2} -> {:+.2} dB", target.air_db));
        }
    }

    if (goal == "clear" || vocal_context) && presence < 0.16 && !harsh && headroom_available {
        let prior = target.presence_db;
        target.presence_db = bounded_add(target.presence_db, 0.25, -3.0, 3.0)
            .min(capability.max_preference_boost_db.min(3.0));
        if (prior - target.presence_db).abs() > 1e-9 {
            changes.push(format!(
                "Presence: {prior:+.2} -> {:+.2} dB",
                target.presence_db
            ));
            if vocal_context && goal != "clear" {
                reasons.push(
                    "Confident vocal semantic context plus low measured presence supports a small bounded presence preference boost."
                        .into(),
                );
            }
        }
    }

    if evidence.stereo_correlation < 0.20 && target.width > 1.0 {
        let prior = target.width;
        target.width = (target.width - 0.05).max(1.0);
        changes.push(format!(
            "Stereo width: {:.0}% -> {:.0}%",
            prior * 100.0,
            target.width * 100.0
        ));
        reasons.push(
            "Low stereo correlation blocks additional widening and moves the preference toward unity width."
                .into(),
        );
    }

    if goal == "soft" && loud && evidence.crest_db < 6.0 && !target.compressor.enabled {
        reasons.push(
            "Already low-crest material does not justify enabling another compressor; retain the user's explicit compression choice."
                .into(),
        );
    }

    let constrained = crate::devices::capability::apply_constraints(&target, capability);
    if constrained != target {
        reasons.push(
            "Device capability limits reduced one or more requested preference boosts.".into(),
        );
        target = constrained;
    }

    ensure!(
        target.correction == before.correction
            && target.correction_preamp_db == before.correction_preamp_db
            && target.correction_source == before.correction_source,
        "Context tuning must not modify device correction"
    );
    target.validate()?;
    // Earlier decisions may be reduced by device limits. Only these final deltas
    // are shown, serialized, and verified when applying the preview.
    let changes = describe_changes(before, &target);

    Ok(Proposal {
        algorithm: "maris-context-tuning-preview-v1".into(),
        goal: goal.into(),
        device: device.into(),
        profile_key: profile_key.into(),
        baseline_listening_revision: listening_revision,
        created_at_ms: analysis::now_ms(),
        evidence: evidence.clone(),
        context: context.clone(),
        capability: capability.clone(),
        effective_preamp_db,
        before: before.clone(),
        profile: target,
        changes,
        reasons,
        limitations: vec![
            "Semantic tags are used only when produced by an integrated semantic backend with sufficient confidence.".into(),
            "Signal spectrum is source evidence, not a measurement of headphone/speaker frequency response.".into(),
            "A proposal changes preference controls only; measured correction remains unchanged.".into(),
            "Digital headroom and limiter telemetry are not acoustic SPL or hearing-protection guarantees.".into(),
        ],
        live_guard: None,
    })
}

pub fn from_live(store: &Store, goal: &str) -> Result<Proposal> {
    from_live_at(store, goal, analysis::now_ms())
}

#[doc(hidden)]
pub fn from_live_at(store: &Store, goal: &str, now: u64) -> Result<Proposal> {
    let runtime = crate::audio::runtime_status_at(store, now);
    ensure!(runtime["active"] == true, "No active audio session");
    let device = runtime["output"]
        .as_str()
        .context("Missing active output")?
        .to_owned();
    let evidence = preview_evidence(&runtime, now)?;
    let context = live_context(&runtime, &evidence, now);
    let profile_key = runtime["profile_key"]
        .as_str()
        .unwrap_or(&device)
        .to_owned();
    let capability = crate::devices::capability::effective(store, &profile_key)?;
    let listening = crate::tuning::preferences::load(store)?;
    let before = listening.effective(&profile_key).clone();
    let effective_preamp_db = runtime["effective_preamp_db"].as_f64().unwrap_or(0.0);
    let session_id = runtime["session_id"]
        .as_str()
        .context("Missing active session identity")?;
    ensure!(
        !session_id.is_empty()
            && session_id.len() <= 256
            && !session_id.chars().any(char::is_control),
        "Invalid active session identity"
    );
    let stable_id = runtime["device_identity"]["stable_id"]
        .as_str()
        .map(str::to_owned);
    if stable_id.is_some() {
        ensure!(
            runtime["device_binding_revision"].as_u64().is_some(),
            "Missing device binding revision"
        );
    }
    let mut proposal = propose_from(TuningInput {
        device: &device,
        profile_key: &profile_key,
        listening_revision: listening.revision,
        before: &before,
        evidence: &evidence,
        context: &context,
        capability: &capability,
        goal,
        effective_preamp_db,
    })?;
    proposal.algorithm = "maris-context-tuning-live-v2".into();
    proposal.live_guard = Some(LiveGuard {
        session_id: session_id.to_owned(),
        stable_id,
        device_binding_revision: runtime["device_binding_revision"].as_u64(),
        rebind_count: runtime["rebind_count"].as_u64(),
    });
    Ok(proposal)
}

pub fn apply(store: &Store, proposal: &Proposal) -> Result<crate::tuning::preferences::Library> {
    ensure!(
        proposal.algorithm == "maris-context-tuning-live-v2",
        "Only a live Maris tuning proposal can be applied"
    );
    let guard = proposal
        .live_guard
        .as_ref()
        .context("Tuning proposal is not bound to a live audio session")?;
    let now = analysis::now_ms();
    ensure!(
        now.checked_sub(proposal.created_at_ms)
            .is_some_and(|age| age < 120_000),
        "Tuning proposal expired or has a future timestamp"
    );
    let runtime = crate::audio::runtime_status(store);
    ensure!(runtime["active"] == true, "No active audio session");
    ensure!(
        runtime["session_id"].as_str() == Some(guard.session_id.as_str()),
        "Audio session changed after preview"
    );
    ensure!(
        runtime["device_identity"]["stable_id"].as_str() == guard.stable_id.as_deref(),
        "Active device identity changed after preview"
    );
    ensure!(
        runtime["device_binding_revision"].as_u64() == guard.device_binding_revision,
        "Device binding changed after preview"
    );
    ensure!(
        runtime["rebind_count"].as_u64() == guard.rebind_count,
        "Audio device was rebound after preview"
    );
    ensure!(
        runtime["output"].as_str() == Some(proposal.device.as_str()),
        "Active output changed after preview"
    );
    ensure!(
        runtime["profile_key"]
            .as_str()
            .or_else(|| runtime["output"].as_str())
            == Some(proposal.profile_key.as_str()),
        "Device profile binding changed after preview"
    );
    let library = crate::tuning::preferences::load(store)?;
    ensure!(
        library.revision == proposal.baseline_listening_revision,
        "Listening profile changed after preview"
    );
    ensure!(
        library.effective(&proposal.profile_key) == &proposal.before,
        "Device preference changed after preview"
    );
    let current_capability = crate::devices::capability::effective(store, &proposal.profile_key)?;
    ensure!(
        current_capability == proposal.capability,
        "Device capability changed after preview"
    );
    let expected = propose_from(TuningInput {
        device: &proposal.device,
        profile_key: &proposal.profile_key,
        listening_revision: proposal.baseline_listening_revision,
        before: &proposal.before,
        evidence: &proposal.evidence,
        context: &proposal.context,
        capability: &proposal.capability,
        goal: &proposal.goal,
        effective_preamp_db: proposal.effective_preamp_db,
    })?;
    ensure!(
        expected.profile == proposal.profile
            && expected.changes == proposal.changes
            && expected.reasons == proposal.reasons,
        "Tuning proposal was modified"
    );
    // A no-op must not erase the last useful undo snapshot or increment revision.
    if proposal.profile == proposal.before {
        return Ok(library);
    }
    crate::tuning::preferences::edit(
        store,
        Some(proposal.baseline_listening_revision),
        Some(&proposal.profile_key),
        |profile| {
            *profile = proposal.profile.clone();
            Ok(())
        },
    )
}
