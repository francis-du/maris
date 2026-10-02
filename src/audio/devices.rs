use anyhow::{bail, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, SampleFormat, SupportedStreamConfig,
};
use serde::Serialize;

#[cfg(target_os = "macos")]
use super::tap_ffi;

#[derive(Serialize, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub direction: String,
    pub is_default: bool,
}

#[cfg(target_os = "windows")]
pub fn devices() -> Result<Vec<DeviceInfo>> {
    super::windows_devices::devices()
}

#[cfg(not(target_os = "windows"))]
pub fn devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_in = host
        .default_input_device()
        .and_then(|d| d.description().ok().map(|v| v.name().to_owned()));
    let default_out = host
        .default_output_device()
        .and_then(|d| d.description().ok().map(|v| v.name().to_owned()));
    #[cfg(target_os = "macos")]
    let default_out_id = tap_ffi::default_output().ok();
    let mut result = Vec::new();
    for (direction, list, default) in [
        (
            "input",
            host.input_devices()?.collect::<Vec<_>>(),
            default_in,
        ),
        (
            "output",
            host.output_devices()?.collect::<Vec<_>>(),
            default_out,
        ),
    ] {
        for (index, device) in list.into_iter().enumerate() {
            let name = device
                .description()
                .map(|v| v.name().to_owned())
                .unwrap_or_else(|_| "Unnamed device".into());
            #[cfg(target_os = "macos")]
            let coreaudio_id = (direction == "output")
                .then(|| tap_ffi::output_device_id_at(index).ok())
                .flatten();
            #[cfg(target_os = "macos")]
            let stable_uid = coreaudio_id.and_then(|id| tap_ffi::device_uid(id).ok());
            #[cfg(target_os = "macos")]
            let id = stable_uid.as_ref().map_or_else(
                || format!("{direction}:{index}"),
                |uid| format!("uid:{uid}"),
            );
            #[cfg(not(target_os = "macos"))]
            let id = format!("{direction}:{index}");
            #[cfg(target_os = "macos")]
            let is_default = if direction == "output" {
                coreaudio_id.zip(default_out_id).map_or_else(
                    || default.as_deref() == Some(&name),
                    |(id, default)| id == default,
                )
            } else {
                default.as_deref() == Some(&name)
            };
            #[cfg(not(target_os = "macos"))]
            let is_default = default.as_deref() == Some(&name);
            result.push(DeviceInfo {
                id,
                is_default,
                name,
                direction: direction.into(),
            });
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(outputs) = super::pulse::devices() {
        // Keep explicit CPAL endpoint IDs available for run/play; append server routing IDs.
        result.extend(outputs);
    }
    Ok(result)
}

pub fn select(selector: Option<&str>, input: bool) -> Result<Device> {
    let host = cpal::default_host();
    let Some(selector) = selector else {
        return if input {
            host.default_input_device()
        } else {
            host.default_output_device()
        }
        .context("No default audio device");
    };
    #[cfg(target_os = "windows")]
    if selector.starts_with("wasapi:") {
        return super::windows_devices::select(selector, input);
    }
    let devices = if input {
        host.input_devices()?.collect::<Vec<_>>()
    } else {
        host.output_devices()?.collect::<Vec<_>>()
    };
    #[cfg(target_os = "macos")]
    if !input {
        if let Some(uid) = selector.strip_prefix("uid:") {
            let index = tap_ffi::output_index_for_uid(uid)?;
            return devices
                .into_iter()
                .nth(index)
                .context("Stable output device is no longer available");
        }
    }
    let prefix = if input { "input:" } else { "output:" };
    if let Some(index) = selector
        .strip_prefix(prefix)
        .and_then(|v| v.parse::<usize>().ok())
    {
        return devices
            .into_iter()
            .nth(index)
            .context("Device index does not exist; run 'maris devices' again");
    }
    let mut matches = devices
        .into_iter()
        .filter(|d| d.description().is_ok_and(|v| v.name() == selector));
    let first = matches.next().with_context(|| {
        format!(
            "No {} device named '{selector}'; run 'maris devices'",
            if input { "input" } else { "output" }
        )
    })?;
    if matches.next().is_some() {
        bail!("Device name is ambiguous; use its input:N or output:N identifier");
    }
    Ok(first)
}

pub fn config(device: &Device, input: bool, rate: u32) -> Result<SupportedStreamConfig> {
    let ranges = if input {
        device.supported_input_configs()?.collect::<Vec<_>>()
    } else {
        device.supported_output_configs()?.collect::<Vec<_>>()
    };
    let mut candidates: Vec<_> = ranges
        .into_iter()
        .filter(|r| {
            (1..=2).contains(&r.channels())
                && r.min_sample_rate() <= rate
                && rate <= r.max_sample_rate()
                && matches!(
                    r.sample_format(),
                    SampleFormat::F32 | SampleFormat::I16 | SampleFormat::U16 | SampleFormat::I32
                )
        })
        .collect();
    candidates.sort_by_key(|r| (r.channels() != 2, r.sample_format() != SampleFormat::F32));
    candidates
        .into_iter()
        .next()
        .map(|r| r.with_sample_rate(rate))
        .with_context(|| {
            format!(
                "Device '{}' does not support mono/stereo PCM at {rate} Hz",
                device
                    .description()
                    .map(|v| v.name().to_owned())
                    .unwrap_or_default()
            )
        })
}

/// Prefer the session rate, then a supported native rate. The caller must resample explicitly.
pub(super) fn config_near(
    device: &Device,
    input: bool,
    preferred: u32,
) -> Result<SupportedStreamConfig> {
    if let Ok(config) = config(device, input, preferred) {
        return Ok(config);
    }
    let default = if input {
        device.default_input_config()
    } else {
        device.default_output_config()
    }?;
    let rate = default.sample_rate();
    if (44100..=192000).contains(&rate) {
        if let Ok(config) = config(device, input, rate) {
            return Ok(config);
        }
    }
    for rate in [48000, 44100, 96000, 88200, 192000, 176400] {
        if let Ok(config) = config(device, input, rate) {
            return Ok(config);
        }
    }
    anyhow::bail!("No supported PCM format for mixer device")
}

/// Stable pins survive reordering. A vanished pin follows the unique physical
/// default, never an arbitrary same-named endpoint or a silently guessed index.
#[cfg(any(target_os = "windows", test))]
pub(super) fn follow_stable_output<'a>(
    inventory: &'a [DeviceInfo],
    pin: Option<&str>,
) -> Result<&'a DeviceInfo> {
    if let Some(pin) = pin {
        anyhow::ensure!(
            pin.strip_prefix("wasapi:").is_some_and(|id| !id.is_empty())
                && pin.len() <= 516
                && !pin.chars().any(char::is_control),
            "Invalid stable Windows output pin"
        );
        let mut matches = inventory
            .iter()
            .filter(|device| device.direction == "output" && device.id == pin);
        if let Some(device) = matches.next() {
            anyhow::ensure!(
                matches.next().is_none(),
                "Ambiguous Windows output identity"
            );
            anyhow::ensure!(!is_virtual(&device.name), "Choose a physical output");
            return Ok(device);
        }
    }
    let mut defaults = inventory
        .iter()
        .filter(|device| device.direction == "output" && device.is_default);
    let device = defaults.next().context("No current default output")?;
    anyhow::ensure!(
        defaults.next().is_none() && !is_virtual(&device.name),
        "No unique physical default output"
    );
    anyhow::ensure!(
        device.id.starts_with("wasapi:") && device.id.len() > 7,
        "Missing stable Windows output identity"
    );
    Ok(device)
}

pub fn is_virtual(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "blackhole",
        "cable input",
        "cable output",
        "loopback",
        "maris",
    ]
    .iter()
    .any(|term| lower.contains(term))
}
