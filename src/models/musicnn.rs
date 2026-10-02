//! Native CPU MusicNN MTT inference. Runs only on the background context worker.
use crate::analysis::context::{MusicContext, SemanticBackend, Tag, Window};
use anyhow::{ensure, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::{
    batch_norm, BatchNorm, BatchNormConfig, Conv2d, Conv2dConfig, Linear, Module, ModuleT,
    VarBuilder,
};
const MELS: usize = 96;
const PATCH_FRAMES: usize = 187;
const LABELS: [&str; 50] = [
    "guitar",
    "classical",
    "slow",
    "techno",
    "strings",
    "drums",
    "electronic",
    "rock",
    "fast",
    "piano",
    "ambient",
    "beat",
    "violin",
    "vocal",
    "synth",
    "female",
    "indian",
    "opera",
    "male",
    "singing",
    "vocals",
    "no vocals",
    "harpsichord",
    "loud",
    "quiet",
    "flute",
    "woman",
    "male vocal",
    "no vocal",
    "pop",
    "soft",
    "sitar",
    "solo",
    "man",
    "classic",
    "choir",
    "voice",
    "new age",
    "dance",
    "male voice",
    "female vocal",
    "beats",
    "harp",
    "cello",
    "no voice",
    "weird",
    "country",
    "metal",
    "female voice",
    "choral",
];

struct ConvBn {
    conv: Conv2d,
    bn: BatchNorm,
}
impl ConvBn {
    fn load(
        vb: VarBuilder<'_>,
        in_channels: usize,
        out_channels: usize,
        kernel: (usize, usize),
    ) -> candle_core::Result<Self> {
        let weight = vb.get(
            (out_channels, in_channels, kernel.0, kernel.1),
            "conv.weight",
        )?;
        let bias = vb.get(out_channels, "conv.bias")?;
        let bn = batch_norm(
            out_channels,
            BatchNormConfig {
                eps: 0.001,
                momentum: 0.01,
                ..Default::default()
            },
            vb.pp("bn"),
        )?;
        Ok(Self {
            conv: Conv2d::new(weight, Some(bias), Conv2dConfig::default()),
            bn,
        })
    }
    fn forward(&self, input: &Tensor) -> candle_core::Result<Tensor> {
        self.bn.forward_t(&self.conv.forward(input)?.relu()?, false)
    }
}

struct MidEnd {
    c1: ConvBn,
    c2: ConvBn,
    c3: ConvBn,
}
impl MidEnd {
    fn load(vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        // Keep explicit paths because these names are part of the reviewed checkpoint contract.
        let c1 = {
            let weight = vb.pp("c1_conv").get((64, 1, 7, 561), "weight")?;
            let bias = vb.pp("c1_conv").get(64, "bias")?;
            ConvBn {
                conv: Conv2d::new(weight, Some(bias), Conv2dConfig::default()),
                bn: batch_norm(
                    64,
                    BatchNormConfig {
                        eps: 0.001,
                        momentum: 0.01,
                        ..Default::default()
                    },
                    vb.pp("c1_bn"),
                )?,
            }
        };
        let c2 = {
            let weight = vb.pp("c2_conv").get((64, 1, 7, 64), "weight")?;
            let bias = vb.pp("c2_conv").get(64, "bias")?;
            ConvBn {
                conv: Conv2d::new(weight, Some(bias), Conv2dConfig::default()),
                bn: batch_norm(
                    64,
                    BatchNormConfig {
                        eps: 0.001,
                        momentum: 0.01,
                        ..Default::default()
                    },
                    vb.pp("c2_bn"),
                )?,
            }
        };
        let c3 = {
            let weight = vb.pp("c3_conv").get((64, 1, 7, 64), "weight")?;
            let bias = vb.pp("c3_conv").get(64, "bias")?;
            ConvBn {
                conv: Conv2d::new(weight, Some(bias), Conv2dConfig::default()),
                bn: batch_norm(
                    64,
                    BatchNormConfig {
                        eps: 0.001,
                        momentum: 0.01,
                        ..Default::default()
                    },
                    vb.pp("c3_bn"),
                )?,
            }
        };
        Ok(Self { c1, c2, c3 })
    }

    fn forward(&self, frontend: &Tensor) -> candle_core::Result<Tensor> {
        let x1 = self
            .c1
            .forward(&frontend.unsqueeze(1)?.pad_with_zeros(2, 3, 3)?)?
            .squeeze(3)?
            .transpose(1, 2)?;
        let x2 = self
            .c2
            .forward(&x1.unsqueeze(1)?.pad_with_zeros(2, 3, 3)?)?
            .squeeze(3)?
            .transpose(1, 2)?;
        let res2 = (&x2 + &x1)?;
        let x3 = self
            .c3
            .forward(&res2.unsqueeze(1)?.pad_with_zeros(2, 3, 3)?)?
            .squeeze(3)?
            .transpose(1, 2)?;
        let res3 = (&x3 + &res2)?;
        Tensor::cat(&[frontend, &x1, &res2, &res3], 2)
    }
}

pub struct MusicNn {
    device: Device,
    bn_input: BatchNorm,
    timbral_1: ConvBn,
    timbral_2: ConvBn,
    temp_1: ConvBn,
    temp_2: ConvBn,
    temp_3: ConvBn,
    midend: MidEnd,
    bn_backend: BatchNorm,
    fc1: Linear,
    bn_fc1: BatchNorm,
    fc2: Linear,
}
impl MusicNn {
    pub fn load(bytes: &'static [u8]) -> Result<Self> {
        ensure!(!bytes.is_empty(), "MusicNN weights are not bundled");
        let device = Device::Cpu;
        let vb = VarBuilder::from_slice_safetensors(bytes, DType::F32, &device)
            .context("Read bundled MusicNN safetensors")?;
        let bn = |count, path: &str| -> candle_core::Result<BatchNorm> {
            batch_norm(
                count,
                BatchNormConfig {
                    eps: 0.001,
                    momentum: 0.01,
                    ..Default::default()
                },
                vb.pp(path),
            )
        };
        let timbral_1 = ConvBn::load(vb.pp("timbral_1.conv_block"), 1, 204, (7, 38))
            .context("Load MusicNN timbral_1")?;
        let timbral_2 = ConvBn::load(vb.pp("timbral_2.conv_block"), 1, 204, (7, 67))
            .context("Load MusicNN timbral_2")?;
        let temp_1 = ConvBn::load(vb.pp("temp_1.conv_block"), 1, 51, (128, 1))
            .context("Load MusicNN temp_1")?;
        let temp_2 = ConvBn::load(vb.pp("temp_2.conv_block"), 1, 51, (64, 1))
            .context("Load MusicNN temp_2")?;
        let temp_3 = ConvBn::load(vb.pp("temp_3.conv_block"), 1, 51, (32, 1))
            .context("Load MusicNN temp_3")?;
        Ok(Self {
            device,
            bn_input: bn(1, "bn_input").context("Load MusicNN input batch norm")?,
            timbral_1,
            timbral_2,
            temp_1,
            temp_2,
            temp_3,
            midend: MidEnd::load(vb.pp("midend")).context("Load MusicNN midend")?,
            bn_backend: bn(1506, "backend.bn_in").context("Load MusicNN backend batch norm")?,
            fc1: candle_nn::linear(1506, 200, vb.pp("backend.fc1")).context("Load MusicNN fc1")?,
            bn_fc1: bn(200, "backend.bn_fc1").context("Load MusicNN fc1 batch norm")?,
            fc2: candle_nn::linear(200, 50, vb.pp("backend.fc2")).context("Load MusicNN fc2")?,
        })
    }

    fn frontend_block(
        input: &Tensor,
        block: &ConvBn,
        kernel_time: usize,
        timbral: bool,
    ) -> candle_core::Result<Tensor> {
        let padded = if timbral {
            input.pad_with_zeros(2, 3, 3)?
        } else {
            let total = kernel_time - 1;
            let left = total / 2;
            input.pad_with_zeros(2, left, total - left)?
        };
        block.forward(&padded)?.max(3)?.transpose(1, 2)
    }

    fn forward(&self, mel: Vec<f32>) -> Result<Vec<f32>> {
        ensure!(
            mel.len() == PATCH_FRAMES * MELS,
            "Invalid MusicNN mel patch"
        );
        let input = Tensor::from_vec(mel, (1, PATCH_FRAMES, MELS), &self.device)?.unsqueeze(1)?;
        let input = self.bn_input.forward_t(&input, false)?;
        let f1 = Self::frontend_block(&input, &self.timbral_1, 7, true)?;
        let f2 = Self::frontend_block(&input, &self.timbral_2, 7, true)?;
        let t1 = Self::frontend_block(&input, &self.temp_1, 128, false)?;
        let t2 = Self::frontend_block(&input, &self.temp_2, 64, false)?;
        let t3 = Self::frontend_block(&input, &self.temp_3, 32, false)?;
        let frontend = Tensor::cat(&[&f1, &f2, &t1, &t2, &t3], 2)?;
        let encoded = self.midend.forward(&frontend)?;
        let max = encoded.max(1)?;
        let mean = encoded.mean(1)?;
        let pooled = Tensor::stack(&[max, mean], 2)?.flatten_from(1)?;
        let hidden = self.bn_backend.forward_t(&pooled, false)?;
        let hidden = self.fc1.forward(&hidden)?.relu()?;
        let hidden = self.bn_fc1.forward_t(&hidden, false)?;
        let logits = self.fc2.forward(&hidden)?;
        let probabilities = candle_nn::ops::sigmoid(&logits)?
            .flatten_all()?
            .to_vec1::<f32>()?;
        ensure!(
            probabilities.len() == LABELS.len(),
            "Invalid MusicNN output"
        );
        Ok(probabilities)
    }
}

impl SemanticBackend for MusicNn {
    fn id(&self) -> &'static str {
        "musicnn-mtt"
    }

    fn infer(&mut self, input: &Window) -> Result<MusicContext> {
        let mel = prepare_window(input)?;
        let probabilities = self.forward(mel)?;
        context_from_probabilities(&probabilities, input)
    }
}

fn prepare_window(input: &Window) -> Result<Vec<f32>> {
    input.analysis.validate()?;
    ensure!(
        input.rate == input.analysis.sample_rate
            && input.analysis.frames == input.frames.len() as u64
            && input.analysis.dropped_frames == 0
            && input.frames.len() == input.rate as usize * crate::analysis::MUSIC_WINDOW_SECONDS,
        "Music recognition requires three complete seconds of matching audio and measurements"
    );
    ensure!(
        input
            .frames
            .iter()
            .flatten()
            .all(|sample| sample.is_finite() && sample.abs() <= 16.0),
        "Music recognition input contains invalid samples"
    );
    let mono = super::musicnn_preprocess::resample_mono(&input.frames, input.rate)?;
    super::musicnn_preprocess::mel_patch(&mono)
}

fn context_from_probabilities(probabilities: &[f32], input: &Window) -> Result<MusicContext> {
    ensure!(
        probabilities.len() == LABELS.len(),
        "Invalid MusicNN tag count"
    );
    ensure!(
        probabilities
            .iter()
            .all(|score| score.is_finite() && (0.0..=1.0).contains(score)),
        "MusicNN returned invalid probabilities"
    );
    let mut ranked: Vec<_> = LABELS
        .iter()
        .zip(probabilities)
        .map(|(label, score)| (*label, *score as f64))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let score = |label: &str| {
        ranked
            .iter()
            .find(|(name, _)| *name == label)
            .map_or(0.0, |(_, value)| *value)
    };
    const GENRES: &[&str] = &[
        "classical",
        "classic",
        "techno",
        "electronic",
        "rock",
        "ambient",
        "indian",
        "opera",
        "pop",
        "new age",
        "dance",
        "country",
        "metal",
    ];
    const INSTRUMENTS: &[&str] = &[
        "guitar",
        "strings",
        "drums",
        "piano",
        "violin",
        "synth",
        "harpsichord",
        "flute",
        "sitar",
        "harp",
        "cello",
    ];
    const MOODS: &[&str] = &["slow", "fast", "loud", "quiet", "soft", "weird"];
    let tags = |allowed: &[&str]| -> Vec<Tag> {
        ranked
            .iter()
            .filter(|(label, value)| *value >= 0.35 && allowed.contains(label))
            .take(5)
            .map(|(label, confidence)| Tag {
                label: (*label).into(),
                confidence: *confidence,
            })
            .collect()
    };
    let vocal = [
        "vocal",
        "female",
        "male",
        "singing",
        "vocals",
        "woman",
        "male vocal",
        "man",
        "choir",
        "voice",
        "male voice",
        "female vocal",
        "female voice",
        "choral",
    ]
    .into_iter()
    .map(score)
    .fold(0.0_f64, f64::max);
    let instrumental = ["no vocals", "no vocal", "no voice"]
        .into_iter()
        .map(score)
        .fold(0.0_f64, f64::max);
    let signal = MusicContext::signal_only(&input.analysis).signal;
    let confidence = ranked.first().map_or(0.0, |(_, score)| *score);
    let result = MusicContext {
        genre: tags(GENRES),
        instruments: tags(INSTRUMENTS),
        vocal_probability: Some(vocal),
        instrumental_probability: Some(instrumental),
        mood: tags(MOODS),
        confidence,
        model: "musicnn-mtt".into(),
        mode: "semantic".into(),
        updated_at_ms: input.analysis.updated_at_ms,
        inference_ms: 0.0,
        signal,
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/unit/musicnn.rs"]
mod tests;
