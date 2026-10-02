use crate::{
    dsp::profile::Profile,
    dsp::{Processor, Settings},
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{fs, path::Path};

pub struct AudioFile {
    pub rate: u32,
    pub frames: Vec<[f32; 2]>,
}
#[derive(Serialize)]
pub struct RenderReport {
    pub sample_rate: u32,
    pub frames: usize,
    pub peak_dbfs: f64,
    pub effective_preamp_db: f64,
}

pub fn read_wav(path: &Path) -> Result<AudioFile> {
    ensure!(
        fs::metadata(path)?.len() <= 128 * 1024 * 1024,
        "WAV files larger than 128 MiB are not supported in this version"
    );
    let mut reader = hound::WavReader::open(path).context("Open PCM WAV file")?;
    let spec = reader.spec();
    ensure!(
        (1..=2).contains(&spec.channels),
        "WAV must be mono or stereo"
    );
    ensure!(
        (44100..=192000).contains(&spec.sample_rate),
        "WAV sample rate must be 44100..192000 Hz"
    );
    ensure!(
        reader.len() <= 32 * 1024 * 1024,
        "Decoded WAV exceeds sample limit"
    );
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            ensure!(
                spec.bits_per_sample == 32,
                "Only 32-bit float WAV is supported"
            );
            reader
                .samples::<f32>()
                .collect::<std::result::Result<_, _>>()?
        }
        hound::SampleFormat::Int => {
            ensure!(
                (8..=32).contains(&spec.bits_per_sample),
                "Unsupported PCM depth"
            );
            let scale = 2.0_f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|v| v.map(|x| x as f32 / scale))
                .collect::<std::result::Result<_, _>>()?
        }
    };
    ensure!(
        samples.len().is_multiple_of(spec.channels as usize),
        "Truncated interleaved WAV frame"
    );
    let frames = samples
        .chunks_exact(spec.channels as usize)
        .map(|c| [c[0], if c.len() == 1 { c[0] } else { c[1] }])
        .collect();
    Ok(AudioFile {
        rate: spec.sample_rate,
        frames,
    })
}

pub fn render(input: &Path, output: &Path, profile: &Profile) -> Result<RenderReport> {
    ensure!(
        !output.exists(),
        "Output already exists; choose a new output file"
    );
    let audio = read_wav(input)?;
    let mut processor = Processor::new(Settings::compile(profile, audio.rate)?);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: audio.rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut writer = hound::WavWriter::new(std::io::BufWriter::new(file), spec)?;
    let mut peak = 0.0_f32;
    for frame in &audio.frames {
        for sample in processor.process(*frame) {
            peak = peak.max(sample.abs());
            writer.write_sample(sample)?;
        }
    }
    writer.finalize()?;
    Ok(RenderReport {
        sample_rate: audio.rate,
        frames: audio.frames.len(),
        peak_dbfs: 20.0 * (peak.max(1e-12) as f64).log10(),
        effective_preamp_db: profile.effective_preamp_db(),
    })
}
