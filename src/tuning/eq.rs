//! Conservative, explainable recommendations. The local EQ planner is not a neural model.
use crate::{
    analysis::{self, Analysis},
    control::store::{Snapshot, Store},
    dsp::profile::Profile,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

pub const GOALS: [&str; 4] = ["balanced", "warm", "clear", "soft"];
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub algorithm: String,
    pub goal: String,
    pub baseline_revision: u64,
    pub created_at_ms: u64,
    pub evidence: Analysis,
    pub profile: Profile,
    pub changes: Vec<String>,
    pub limitations: Vec<String>,
}

pub fn propose(state: &Snapshot, evidence: &Analysis, goal: &str) -> Result<Proposal> {
    evidence.validate()?;
    state.profile.validate()?;
    ensure!(
        GOALS.contains(&goal),
        "Goal must be balanced, warm, clear, or soft"
    );
    ensure!(
        evidence.frames >= evidence.sample_rate as u64 * 2,
        "At least two seconds of audio are needed"
    );
    ensure!(
        evidence.rms_dbfs > -60.0,
        "Signal is too quiet for a reliable tonal proposal"
    );
    ensure!(
        evidence.dropped_frames == 0,
        "Analysis lost samples; wait for a complete window"
    );
    ensure!(
        !state.profile.bypass,
        "Turn off bypass before requesting a tonal proposal"
    );
    let bass: f64 = evidence.band_energy_share[..3].iter().sum();
    let mids: f64 = evidence.band_energy_share[3..7].iter().sum();
    let highs: f64 = evidence.band_energy_share[7..].iter().sum();
    let mut target = state.profile.clone();
    let mut changes = Vec::new();
    let mut deltas = [0.0_f64; 10];
    match goal {
        "warm" => {
            if bass < 0.55 {
                deltas[1] = 0.75;
                deltas[2] = 0.5;
            }
            if highs > 0.08 {
                deltas[7] = -0.5;
                deltas[8] = -0.75;
            }
        }
        "clear" => {
            if bass > 0.3 {
                deltas[2] = -0.5;
                deltas[3] = -0.75;
            }
            if mids < 0.8 {
                deltas[5] = 0.5;
                deltas[6] = 0.75;
            }
        }
        "soft" => {
            if highs > 0.04 {
                deltas[7] = -0.75;
                deltas[8] = -1.0;
                deltas[9] = -0.5;
            }
        }
        "balanced" => {
            if bass > 0.7 {
                deltas[1] = -0.5;
                deltas[2] = -0.75;
            }
            if highs > 0.35 {
                deltas[7] = -0.5;
                deltas[8] = -0.75;
            }
        }
        _ => unreachable!(),
    }
    for (index, band) in target.bands.iter_mut().enumerate() {
        let region = crate::dsp::profile::FREQUENCIES
            .windows(2)
            .position(|pair| band.frequency_hz < (pair[0] * pair[1]).sqrt())
            .unwrap_or(9);
        let delta = deltas[region];
        let old = band.gain_db;
        band.gain_db = (old + delta).clamp(-12.0, 12.0);
        if (old - band.gain_db).abs() > 1e-9 {
            changes.push(format!("Band {} ({:.0} Hz): {:+.2} -> {:+.2} dB; goal={}, measured bass/mid/high shares={:.2}/{:.2}/{:.2}", index + 1, band.frequency_hz, old, band.gain_db, goal, bass, mids, highs));
        }
    }
    if !changes.is_empty() {
        target.name = format!("smart-{goal}");
    }
    // No automatic loudness increase, stereo change, or unbounded flattening of musical spectra.
    target.preamp_db = target.preamp_db.min(state.profile.effective_preamp_db());
    if target.preamp_db < -30.0 {
        target.preamp_db = -30.0;
    }
    if (target.preamp_db - state.profile.preamp_db).abs() > 1e-9 {
        changes.push(format!(
            "Preamp: {:.2} -> {:.2} dB to retain conservative headroom",
            state.profile.preamp_db, target.preamp_db
        ));
    }
    target.validate()?;
    Ok(Proposal { algorithm: "maris-tonal-heuristic-v1".into(), goal: goal.into(), baseline_revision: state.revision,
        created_at_ms: analysis::now_ms(), evidence: evidence.clone(), profile: target, changes,
        limitations: vec!["This is a bounded heuristic, not a trained mastering model or a quality guarantee.".into(),
            "Input spectral balance is not headphone frequency response or acoustic SPL.".into(),
            "One proposal changes a band by at most 1 dB. Listen and undo before requesting another.".into()] })
}
pub fn from_live(store: &Store, goal: &str) -> Result<Proposal> {
    let runtime = crate::audio::runtime_status(store);
    ensure!(
        runtime["active"].as_bool() == Some(true),
        "No active audio session; start playback or provide --file"
    );
    let evidence: Analysis = serde_json::from_value(
        runtime
            .get("analysis")
            .cloned()
            .context("Analysis is still warming up")?,
    )?;
    ensure!(
        analysis::now_ms().saturating_sub(evidence.updated_at_ms) < 6000,
        "Analysis is stale; wait for a new window"
    );
    propose(&store.load()?, &evidence, goal)
}
pub fn apply(store: &Store, proposal: &Proposal) -> Result<Snapshot> {
    ensure!(
        proposal.algorithm == "maris-tonal-heuristic-v1",
        "Unsupported planner"
    );
    ensure!(
        analysis::now_ms().saturating_sub(proposal.created_at_ms) < 120000,
        "Proposal expired; analyze again"
    );
    let current = store.load()?;
    ensure!(
        current.revision == proposal.baseline_revision,
        "Profile changed after preview; create a new proposal"
    );
    // Recompute from evidence instead of trusting an externally modified proposal profile.
    let expected = propose(&current, &proposal.evidence, &proposal.goal)?;
    ensure!(
        expected.profile == proposal.profile,
        "Proposal was modified; use validated profile apply instead"
    );
    if expected.profile == current.profile {
        return Ok(current);
    }
    store.edit(Some(proposal.baseline_revision), |profile| {
        *profile = proposal.profile.clone();
        Ok(())
    })
}
