//! Controlled offline feature ablation through the production Processor.
//! Output difference proves contribution, not subjective benefit. Nothing is played or captured.
use anyhow::{ensure, Result};
use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
    tone::{Filter, Kind},
};
use serde_json::{json, Value};
use std::time::Instant;

fn input(rate: u32, fixture: usize) -> Vec<[f32; 2]> {
    // An eighth second still covers every fixture's detector/pulse behavior while keeping
    // this numerical matrix bounded on the slowest native release runner.
    (0..rate as usize / 8)
        .map(|i| {
            let phase = std::f64::consts::TAU * i as f64 / f64::from(rate);
            let pulse = if i % (rate as usize / 5) < rate as usize / 50 {
                1.0
            } else {
                0.35
            };
            let (left, right) = match fixture {
                0 => {
                    let x = 0.65 * (phase * 70.0).sin() + 0.025 * (phase * 1000.0).sin();
                    (x, x)
                }
                1 => {
                    let x = 0.6 * (phase * 5600.0).sin() + 0.04 * (phase * 800.0).sin();
                    (x * pulse, x * pulse)
                }
                _ => (
                    (0.4 * (phase * 110.0).sin() + 0.25 * (phase * 1500.0).sin()) * pulse,
                    (0.32 * (phase * 165.0 + 0.2).sin() + 0.22 * (phase * 6400.0).sin()) * pulse,
                ),
            };
            [left as f32, right as f32]
        })
        .collect()
}

fn render(
    eq: &Profile,
    music: &MusicProfile,
    rate: u32,
    frames: &[[f32; 2]],
) -> Result<(Vec<[f32; 2]>, f64)> {
    let mut processor = Processor::new(Settings::compile(eq, rate)?.with_music(music, rate)?);
    // Warm stateful detectors on the beginning of one continuous program, then measure
    // only the continuation. Replaying the input from frame zero after warm-up creates an
    // artificial phase/envelope discontinuity that can exaggerate stateful-effect deltas.
    let settle = (rate as usize / 32).min(frames.len());
    for frame in &frames[..settle] {
        std::hint::black_box(processor.process(*frame));
    }
    let began = Instant::now();
    let result: Vec<_> = frames[settle..]
        .iter()
        .map(|frame| processor.process(*frame))
        .collect();
    let elapsed_ms = began.elapsed().as_secs_f64() * 1000.0;
    ensure!(
        result
            .iter()
            .flatten()
            .all(|v| v.is_finite() && v.abs() <= 0.891252),
        "Ablation produced invalid or unbounded samples"
    );
    Ok((result, elapsed_ms))
}

fn rms(frames: &[[f32; 2]]) -> f64 {
    (frames
        .iter()
        .flatten()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        / (frames.len() * 2) as f64)
        .sqrt()
}
fn db(value: f64) -> f64 {
    20.0 * value.max(1e-12).log10()
}

pub fn report(rates: &[u32]) -> Result<Value> {
    let mut base_eq = Profile::default();
    base_eq.bands[3].gain_db = -1.25;
    base_eq.crossfeed = 0.12;
    let mut base_music = MusicProfile {
        bass_db: 1.0,
        presence_db: 0.75,
        air_db: -0.5,
        softness: 0.35,
        width: 1.25,
        virtual_surround: 0.65,
        stereo_focus: 0.35,
        level_match: false,
        correction: vec![Filter {
            kind: Kind::Peak,
            frequency_hz: 3000.0,
            gain_db: -2.0,
            q: 1.0,
        }],
        correction_preamp_db: -1.0,
        correction_source: Some("Synthetic ablation correction, not a device measurement".into()),
        ..MusicProfile::default()
    };
    base_music.adaptive.strength = 1.0;
    base_music.bass_assist.enabled = true;
    base_music.bass_assist.amount = 0.25;
    base_music.compressor.enabled = true;
    base_music.compressor.threshold_db = -24.0;
    let names = [
        "reference",
        "without_correction",
        "without_preference_tone",
        "without_dynamic_eq",
        "without_virtual_bass",
        "without_compression",
        "without_stereo_width",
        "without_virtual_surround",
        "without_stereo_focus",
        "without_profile_eq",
        "without_crossfeed",
    ];
    let mut results = Vec::new();
    for &rate in rates {
        for (fixture, name) in ["bass_dominant_mono", "treble_pulses_mono", "mixed_stereo"]
            .iter()
            .enumerate()
        {
            let frames = input(rate, fixture);
            let (reference, reference_elapsed) = render(&base_eq, &base_music, rate, &frames)?;
            let reference_rms = rms(&reference);
            for removed in names {
                let mut eq = base_eq.clone();
                let mut music = base_music.clone();
                match removed {
                    "without_correction" => {
                        music.correction.clear();
                        music.correction_preamp_db = 0.0;
                        music.correction_source = None;
                    }
                    "without_preference_tone" => {
                        music.bass_db = 0.0;
                        music.presence_db = 0.0;
                        music.air_db = 0.0;
                        music.softness = 0.0;
                    }
                    "without_dynamic_eq" => music.adaptive.enabled = false,
                    "without_virtual_bass" => music.bass_assist.enabled = false,
                    "without_compression" => music.compressor.enabled = false,
                    "without_stereo_width" => music.width = 1.0,
                    "without_virtual_surround" => music.virtual_surround = 0.0,
                    "without_stereo_focus" => music.stereo_focus = 0.0,
                    "without_profile_eq" => {
                        for band in &mut eq.bands {
                            band.gain_db = 0.0;
                        }
                    }
                    "without_crossfeed" => eq.crossfeed = 0.0,
                    _ => {}
                }
                let rendered = if removed == "reference" {
                    None
                } else {
                    Some(render(&eq, &music, rate, &frames)?)
                };
                let (output, elapsed) = match rendered.as_ref() {
                    Some((output, elapsed)) => (output, *elapsed),
                    None => (&reference, reference_elapsed),
                };
                let output_rms = rms(output);
                // Normalize only the reported numerical difference, never a played audio path.
                let scale = reference_rms / output_rms.max(1e-12);
                let difference = (reference
                    .iter()
                    .flatten()
                    .zip(output.iter().flatten())
                    .map(|(a, b)| (f64::from(*a) - f64::from(*b) * scale).powi(2))
                    .sum::<f64>()
                    / (reference.len() * 2) as f64)
                    .sqrt();
                let peak = output
                    .iter()
                    .flatten()
                    .fold(0.0_f32, |max, v| max.max(v.abs()));
                results.push(json!({"fixture":name,"sample_rate":rate,"variant":removed,
                    "rms_dbfs":db(output_rms),"peak_dbfs":db(f64::from(peak)),
                    "crest_db":db(f64::from(peak)) - db(output_rms),
                    "level_matched_difference_rms":difference,"processing_wall_ms":elapsed,
                    "output_finite_and_limited":true}));
            }
        }
    }
    Ok(
        json!({"scope":"offline_numerical_feature_ablation", "results":results,
        "reference":"All tested effects deliberately enabled for contribution testing; not a shipped preset",
        "audio_started":false,"subjective_quality_validated":false,
        "timing_is_release_build":!cfg!(debug_assertions),
        "limitations":"A nonzero difference is not proof of preferred sound. Zero effect on mono or inactive detector conditions is expected. Limiter and safety headroom are never disabled. No device response is inferred."}),
    )
}
