//! Local model inventory and worker-only inference backends.
//! Runtime model downloads are intentionally excluded from the normal product path.
#[cfg(feature = "neural")]
use anyhow::ensure;
use anyhow::Result;
use serde_json::{json, Value};

pub(crate) mod musicnn;
pub(crate) mod musicnn_preprocess;

include!(concat!(env!("OUT_DIR"), "/bundled_models.rs"));

pub fn models() -> Value {
    let mut registry = json!({"models":[
        {"id":"rnnoise","kind":"neural_model","backend":"nnnoiseless 0.5.2","compiled":cfg!(feature="neural"),"bundled":cfg!(feature="neural"),"downloaded":cfg!(feature="neural"),"local":true,"status":"integrated","purpose":"speech noise suppression and voice probability","activation":"explicit_speech_only","sample_rate":48000,"frame_samples":480,"license":"BSD-3-Clause","weights":"built into the pinned dependency","music_safe_default":false,"audio_callback_thread":false},
        {"id":"signal-context","kind":"signal_analysis","compiled":true,"bundled":true,"downloaded":false,"local":true,"status":"integrated","role":"music signal analysis","purpose":"measured evidence for local deterministic tuning; not a neural model","semantic":false,"music_safe_default":true,"raw_audio_uploads":false,"audio_callback_thread":false},
        {"id":"musicnn","kind":"semantic_model","backend":"native Candle CPU","compiled":MUSICNN_BUNDLED,"bundled":MUSICNN_BUNDLED,"downloaded":MUSICNN_BUNDLED,"local":true,"status":"integrated","purpose":"local music tagging for bounded context-aware tuning","activation":"background_context_worker","sample_rate":16000,"fft_size":512,"hop_size":256,"mel_bands":96,"license":"Apache-2.0 model-card metadata; original architecture code ISC","weights_sha256":MUSICNN_SOURCE_SHA256,"source_revision":MUSICNN_SOURCE_REVISION,"music_safe_default":true,"raw_audio_uploads":false,"audio_callback_thread":false}
    ],"scope":"shipped_backends","music_requires_model_download":false,"automatic_downloads":false,"raw_audio_uploads":false,"research_notes":"docs/development/models.md","policy":"Only implemented backends appear here. Small reviewed resources are bundled at build time; runtime download is reserved for explicitly optional large models."});
    if let Some(models) = registry["models"].as_array_mut() {
        for model in models {
            let compiled = model["compiled"] == true;
            if model["status"] == "integrated" && !compiled {
                model["status"] = json!("unavailable");
            }
            model["usable"] = json!(compiled && model["status"] == "integrated");
            model["inference_available"] = json!(
                matches!(
                    model["kind"].as_str(),
                    Some("neural_model" | "semantic_model")
                ) && model["usable"] == true
            );
            if model["downloaded"].is_null() {
                model["downloaded"] = json!(false);
            }
            model["downloadable"] = json!(false);
            model["download_blocked_reason"] = if compiled {
                json!("Backend is bundled; no separate download is needed")
            } else {
                json!("Backend is not compiled in this build")
            };
        }
    }
    registry
}

pub fn models_at(_store: &crate::control::store::Store) -> Result<Value> {
    Ok(models())
}

/// Developer provenance for the semantic backend. It never downloads or mutates model state.
pub fn research_models_at(_store: &crate::control::store::Store) -> Result<Value> {
    Ok(json!({
        "scope":"model_provenance",
        "research_notes":"docs/development/models.md",
        "automatic_downloads":false,
        "raw_audio_uploads":false,
        "models":[{
            "id":"musicnn",
            "status":if MUSICNN_BUNDLED {"integrated"} else {"build_resource_absent"},
            "bundled":MUSICNN_BUNDLED,
            "inference_adapter":"native_candle_cpu",
            "inference_available":MUSICNN_BUNDLED,
            "downloadable":false,
            "repository_revision":MUSICNN_SOURCE_REVISION,
            "weights_sha256":MUSICNN_SOURCE_SHA256,
            "bytes":musicnn_weights().map_or(0, |bytes| bytes.len()),
            "parameters":792000,
            "sample_rate":16000,
            "fft_size":512,
            "hop_size":256,
            "mel_bands":96,
            "log_compression":"log10(10000*x+1)",
            "recommended_window_seconds":3.0,
            "audio_callback_thread":false
        }]
    }))
}

pub(crate) fn musicnn_weights() -> Option<&'static [u8]> {
    MUSICNN_BUNDLED.then_some(MUSICNN_MODEL_BYTES)
}

pub(crate) fn music_context_backend(
) -> Result<Option<Box<dyn crate::analysis::context::SemanticBackend>>> {
    musicnn_weights()
        .map(musicnn::MusicNn::load)
        .transpose()
        .map(|model| {
            model.map(|model| Box::new(model) as Box<dyn crate::analysis::context::SemanticBackend>)
        })
}

#[cfg(test)]
#[path = "../../tests/unit/model_cache.rs"]
mod bundle_tests;

#[cfg(feature = "neural")]
pub struct VoiceModel {
    left: Box<nnnoiseless::DenoiseState<'static>>,
    right: Box<nnnoiseless::DenoiseState<'static>>,
    input: [[f32; 480]; 2],
    output: [[f32; 480]; 2],
    first: bool,
}
#[cfg(feature = "neural")]
impl Default for VoiceModel {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(feature = "neural")]
impl VoiceModel {
    pub fn new() -> Self {
        Self {
            left: nnnoiseless::DenoiseState::new(),
            right: nnnoiseless::DenoiseState::new(),
            input: [[0.0; 480]; 2],
            output: [[0.0; 480]; 2],
            first: true,
        }
    }
    pub fn process(&mut self, frames: &[[f32; 2]; 480], result: &mut [[f32; 2]; 480]) -> f32 {
        for (i, frame) in frames.iter().enumerate() {
            for (c, value) in frame.iter().enumerate() {
                self.input[c][i] = if value.is_finite() {
                    (*value * 32768.0).clamp(-32768.0, 32767.0)
                } else {
                    0.0
                };
            }
        }
        let l = self.left.process_frame(&mut self.output[0], &self.input[0]);
        let r = self
            .right
            .process_frame(&mut self.output[1], &self.input[1]);
        for (i, frame) in result.iter_mut().enumerate() {
            for (c, value) in frame.iter_mut().enumerate() {
                let sample = self.output[c][i] / 32768.0;
                *value = if !self.first && sample.is_finite() {
                    sample.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        self.first = false;
        ((l + r) * 0.5).clamp(0.0, 1.0)
    }
}

pub fn enhance(input: &std::path::Path, output: &std::path::Path) -> Result<Value> {
    #[cfg(not(feature = "neural"))]
    {
        let _ = (input, output);
        anyhow::bail!("Rebuild with the neural feature to enable RNNoise");
    }
    #[cfg(feature = "neural")]
    {
        ensure!(!output.exists(), "Output exists; choose a new file");
        let audio = crate::audio::offline::read_wav(input)?;
        ensure!(
            audio.rate == 48000,
            "RNNoise requires 48 kHz; resample the speech WAV first"
        );
        ensure!(!audio.frames.is_empty(), "WAV contains no audio");
        let mut model = VoiceModel::new();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output)?;
        let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)?;
        let mut block = [[0.0_f32; 2]; 480];
        let mut denoised = block;
        let block_count = audio.frames.len().div_ceil(480);
        let mut written = 0;
        let mut vad = 0.0;
        for i in 0..=block_count {
            block.fill([0.0; 2]);
            if i < block_count {
                let chunk = &audio.frames[i * 480..audio.frames.len().min((i + 1) * 480)];
                block[..chunk.len()].copy_from_slice(chunk);
            }
            vad += model.process(&block, &mut denoised) as f64;
            if i == 0 {
                continue;
            }
            for frame in denoised
                .iter()
                .take((audio.frames.len() - written).min(480))
            {
                for sample in frame {
                    writer.write_sample(*sample)?;
                }
                written += 1;
            }
        }
        writer.finalize()?;
        Ok(json!({
            "model":"rnnoise",
            "backend":"nnnoiseless",
            "local":true,
            "sample_rate":48000,
            "frames":written,
            "mean_voice_probability":vad/(block_count+1) as f64,
            "warning":"Speech enhancement only. Musical instruments can be attenuated; no quality improvement is guaranteed."
        }))
    }
}
