//! Stable WASAPI endpoint IDs paired with the pinned CPAL 0.15 WASAPI enumerator.
//! Both enumerate active eAll endpoints in collection order; reject snapshot churn.
use anyhow::{ensure, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device,
};
use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    Media::Audio::{eAll, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE},
    System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    },
};

struct Apartment(bool);
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}
fn ids() -> Result<Vec<String>> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    ensure!(
        initialized.is_ok() || initialized == RPC_E_CHANGED_MODE,
        "Initialize endpoint COM apartment: {initialized:?}"
    );
    let _apartment = Apartment(initialized.is_ok());
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
    let endpoints = unsafe { enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE) }?;
    let count = unsafe { endpoints.GetCount() }?;
    ensure!(count <= 256, "Too many active Windows endpoints");
    (0..count)
        .map(|index| {
            let endpoint = unsafe { endpoints.Item(index) }?;
            let value = unsafe { endpoint.GetId() }?;
            let text = unsafe { value.to_string() };
            unsafe { CoTaskMemFree(Some(value.0.cast())) };
            let text = text?;
            ensure!(
                !text.is_empty() && text.len() <= 509 && !text.chars().any(char::is_control),
                "Invalid Windows endpoint ID"
            );
            Ok(text)
        })
        .collect()
}
fn paired() -> Result<Vec<(String, Device)>> {
    let before = ids()?;
    let devices: Vec<_> = cpal::default_host().devices()?.collect();
    ensure!(
        before == ids()? && before.len() == devices.len(),
        "Windows endpoints changed during enumeration; retry selection"
    );
    Ok(before.into_iter().zip(devices).collect())
}
#[allow(unreachable_patterns)]
fn same(a: &Device, b: &Device) -> bool {
    match (a.as_inner(), b.as_inner()) {
        (cpal::platform::DeviceInner::Wasapi(a), cpal::platform::DeviceInner::Wasapi(b)) => a == b,
        _ => false,
    }
}
fn supports(device: &Device, input: bool) -> bool {
    if input {
        device
            .supported_input_configs()
            .is_ok_and(|mut c| c.next().is_some())
    } else {
        device
            .supported_output_configs()
            .is_ok_and(|mut c| c.next().is_some())
    }
}
pub(super) fn identity(device: &Device) -> Result<String> {
    paired()?
        .into_iter()
        .find(|(_, candidate)| same(device, candidate))
        .map(|(id, _)| id)
        .context("Selected Windows endpoint disappeared")
}
pub(super) fn select(selector: &str, input: bool) -> Result<Device> {
    let id = selector
        .strip_prefix("wasapi:")
        .filter(|id| !id.is_empty())
        .context("Invalid WASAPI selector")?;
    paired()?
        .into_iter()
        .find(|(candidate, device)| candidate == id && supports(device, input))
        .map(|(_, device)| device)
        .context("Selected Windows endpoint is unavailable in this direction")
}
pub(super) fn devices() -> Result<Vec<super::DeviceInfo>> {
    let host = cpal::default_host();
    let input = host.default_input_device();
    let output = host.default_output_device();
    let mut result = Vec::new();
    for (id, device) in paired()? {
        let name = device.name()?;
        for (direction, is_input, default) in [
            ("input", true, input.as_ref()),
            ("output", false, output.as_ref()),
        ] {
            if supports(&device, is_input) {
                result.push(super::DeviceInfo {
                    id: format!("wasapi:{id}"),
                    name: name.clone(),
                    direction: direction.into(),
                    is_default: default.is_some_and(|default| same(default, &device)),
                });
            }
        }
    }
    Ok(result)
}
