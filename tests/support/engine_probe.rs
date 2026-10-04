//! Offline cost probe for the actual shipped DSP and embedded speech network.
//! Generated tones/noise are not speech-quality or hardware-latency evidence.
mod ablation;
use anyhow::{ensure, Result};
use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    neural::VoiceModel,
    profile::Profile,
};
use serde_json::{json, Value};
use std::{hint::black_box, time::Instant};

const RATE: u32 = 48_000;
const BLOCK: usize = 480;
const BLOCKS: usize = 800;

fn report(id: &str, mut times: Vec<f64>, peak: f32, checksum: f64) -> Value {
    let total: f64 = times.iter().sum();
    times.sort_by(f64::total_cmp);
    let audio_seconds = BLOCKS as f64 * BLOCK as f64 / f64::from(RATE);
    json!({
        "id":id, "sample_rate":RATE, "channels":2,
        "audio_seconds":audio_seconds, "blocks":BLOCKS,
        "processing_wall_ms":total,
        "realtime_budget_percent":total / (audio_seconds * 1000.0) * 100.0,
        "p50_block_ms":times[times.len()/2],
        "p95_block_ms":times[times.len()*95/100],
        "max_block_ms":times[times.len()-1],
        "output_peak":peak, "finite_output":checksum.is_finite(),
        "quality_evaluated":false
    })
}

fn probe_dsp(id: &str, frames: &[[f32; 2]], music: &MusicProfile) -> Result<Value> {
    let settings = Settings::compile(&Profile::default(), RATE)?.with_music(music, RATE)?;
    let mut dsp = Processor::new(settings);
    for frame in frames.iter().take(RATE as usize / 2) {
        black_box(dsp.process(*frame));
    }
    let mut times = Vec::with_capacity(BLOCKS);
    let mut peak = 0.0_f32;
    let mut checksum = 0.0_f64;
    for block in frames.as_chunks::<BLOCK>().0 {
        let began = Instant::now();
        let mut output = [[0.0_f32; 2]; BLOCK];
        for (source, target) in block.iter().zip(output.iter_mut()) {
            *target = black_box(dsp.process(*source));
        }
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        for value in output.iter().flatten() {
            ensure!(value.is_finite(), "DSP produced a nonfinite sample");
            peak = peak.max(value.abs());
            checksum += f64::from(*value);
        }
    }
    Ok(report(id, times, peak, checksum))
}

fn main() -> Result<()> {
    ensure!(
        !cfg!(debug_assertions),
        "Run this probe with --release; debug timings are not a product budget"
    );
    if std::env::args().any(|argument| argument == "--ablation") {
        let report = ablation::report(&[44_100, 48_000, 96_000, 192_000])?;
        std::fs::create_dir_all(".maris-review")?;
        let text = serde_json::to_string_pretty(&report)?;
        std::fs::write(".maris-review/feature-ablation.json", &text)?;
        println!("{text}");
        return Ok(());
    }
    let frames: Vec<[f32; 2]> = (0..BLOCKS * BLOCK)
        .map(|i| {
            let phase = std::f64::consts::TAU * i as f64 / f64::from(RATE);
            let noise = ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8) as f64
                / 16777216.0
                - 0.5;
            [
                (0.08 * (phase * 220.0).sin() + 0.04 * (phase * 440.0).sin() + noise * 0.015)
                    as f32,
                (0.08 * (phase * 220.0 + 0.1).sin() + 0.04 * (phase * 880.0).sin() - noise * 0.015)
                    as f32,
            ]
        })
        .collect();
    let native = probe_dsp("native-music-dsp", &frames, &MusicProfile::default())?;
    let no_level_match = MusicProfile {
        level_match: false,
        ..MusicProfile::default()
    };
    let native_without_level_match = probe_dsp(
        "native-music-dsp-without-level-match",
        &frames,
        &no_level_match,
    )?;
    let mut model = VoiceModel::new();
    let mut input = [[0.0_f32; 2]; BLOCK];
    let mut output = input;
    input.copy_from_slice(&frames[..BLOCK]);
    for _ in 0..50 {
        black_box(model.process(&input, &mut output));
    }
    let mut times = Vec::with_capacity(BLOCKS);
    let mut peak = 0.0_f32;
    let mut checksum = 0.0_f64;
    for block in frames.as_chunks::<BLOCK>().0 {
        input.copy_from_slice(block);
        let began = Instant::now();
        let probability = black_box(model.process(black_box(&input), black_box(&mut output)));
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        ensure!(
            (0.0..=1.0).contains(&probability),
            "Invalid voice probability"
        );
        for value in output.iter().flatten() {
            ensure!(
                value.is_finite(),
                "Speech backend produced a nonfinite sample"
            );
            peak = peak.max(value.abs());
            checksum += f64::from(*value);
        }
    }
    let result = json!({
        "scope":"offline_generated_signal_cost_only", "architecture":std::env::consts::ARCH,
        "platform":std::env::consts::OS, "build":"release", "audio_capture_started":false,
        "downloads":false, "music_model_added":false,
        "results":[native,native_without_level_match,report("rnnoise-speech-only",times,peak,checksum)],
        "limitations":"Single offline run, not callback CPU, device latency, speech intelligibility or a listening-quality comparison. RNNoise remains speech-only."
    });
    std::fs::create_dir_all(".maris-review")?;
    std::fs::write(
        ".maris-review/engine-cost.json",
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
