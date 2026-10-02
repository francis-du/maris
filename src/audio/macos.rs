use super::RoutePlan;
use crate::{
    audio::devices,
    control::store::{read_json, Store},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{c_char, c_void, CStr},
    mem::size_of,
    ptr,
};

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}
const SYSTEM: u32 = 1;
const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const DEVICES: u32 = u32::from_be_bytes(*b"dev#");
const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
const NAME: u32 = u32::from_be_bytes(*b"lnam");
const UID: u32 = u32::from_be_bytes(*b"uid ");

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut c_char,
        capacity: isize,
        encoding: u32,
    ) -> u8;
    fn CFRelease(value: *const c_void);
}
fn address(selector: u32) -> Address {
    Address {
        selector,
        scope: GLOBAL,
        element: 0,
    }
}
fn check(status: i32) -> Result<()> {
    ensure!(status == 0, "CoreAudio returned OSStatus {status}");
    Ok(())
}
fn default_output() -> Result<u32> {
    let mut id = 0_u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: the property has a UInt32 value; both output pointers reference live storage.
    unsafe {
        check(AudioObjectGetPropertyData(
            SYSTEM,
            &address(DEFAULT_OUTPUT),
            0,
            ptr::null(),
            &mut size,
            (&mut id as *mut u32).cast(),
        ))?;
    }
    ensure!(size == 4 && id != 0, "No default CoreAudio output device");
    Ok(id)
}
fn set_output(id: u32) -> Result<()> {
    // SAFETY: id is a live AudioDeviceID obtained from the CoreAudio device list.
    unsafe {
        check(AudioObjectSetPropertyData(
            SYSTEM,
            &address(DEFAULT_OUTPUT),
            0,
            ptr::null(),
            4,
            (&id as *const u32).cast(),
        ))
    }
}
fn ids() -> Result<Vec<u32>> {
    let mut size = 0_u32;
    // SAFETY: address and size point to valid storage; no qualifier is required.
    unsafe {
        check(AudioObjectGetPropertyDataSize(
            SYSTEM,
            &address(DEVICES),
            0,
            ptr::null(),
            &mut size,
        ))?;
    }
    ensure!(
        size <= 65536 && size.is_multiple_of(4),
        "Invalid CoreAudio device list size"
    );
    let mut result = vec![0_u32; size as usize / 4];
    // SAFETY: the vector has the exact capacity and initialized length reported by CoreAudio.
    unsafe {
        check(AudioObjectGetPropertyData(
            SYSTEM,
            &address(DEVICES),
            0,
            ptr::null(),
            &mut size,
            result.as_mut_ptr().cast(),
        ))?;
    }
    result.truncate(size as usize / 4);
    Ok(result)
}
fn string(id: u32, selector: u32) -> Result<String> {
    let mut value: *const c_void = ptr::null();
    let mut size = size_of::<*const c_void>() as u32;
    // SAFETY: name and UID properties return a CFStringRef; the returned reference is released below.
    unsafe {
        check(AudioObjectGetPropertyData(
            id,
            &address(selector),
            0,
            ptr::null(),
            &mut size,
            (&mut value as *mut *const c_void).cast(),
        ))?;
    }
    ensure!(
        !value.is_null(),
        "CoreAudio returned an empty string reference"
    );
    let mut buffer = [0 as c_char; 4096];
    // SAFETY: value is a valid CFStringRef and buffer is writable for the supplied length.
    let ok = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            0x08000100,
        )
    };
    unsafe {
        CFRelease(value);
    }
    ensure!(
        ok != 0,
        "CoreAudio device string is too long or invalid UTF-8"
    );
    // SAFETY: successful CFStringGetCString guarantees a NUL-terminated buffer.
    Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()?
        .to_owned())
}
fn find(selector: u32, value: &str) -> Result<u32> {
    let matches: Vec<_> = ids()?
        .into_iter()
        .filter(|id| string(*id, selector).is_ok_and(|s| s == value))
        .collect();
    ensure!(
        matches.len() == 1,
        "CoreAudio device is absent or ambiguous: {value}"
    );
    Ok(matches[0])
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    original_uid: String,
    target_uid: String,
}
pub struct RouteGuard {
    store: Store,
}

pub fn plan(output: Option<&str>) -> Result<RoutePlan> {
    let input = devices()?.into_iter().find(|d| d.direction == "input" && d.name == "BlackHole 2ch")
        .context("BlackHole 2ch is not installed. Install it separately, then reopen Maris. WAV playback works without it")?.name;
    let output = match output {
        Some(name) => name.to_owned(),
        None => string(default_output()?, NAME)?,
    };
    ensure!(
        !crate::audio::devices::is_virtual(&output),
        "Choose physical headphones with --output; the current default output is virtual"
    );
    Ok(RoutePlan { input, output })
}
pub fn activate(store: &Store, plan: &RoutePlan) -> Result<RouteGuard> {
    ensure!(
        !store.directory.join("route.json").exists(),
        "A previous routing journal exists; run 'maris restore' first"
    );
    let original = default_output()?;
    let target = find(NAME, &plan.input)?;
    ensure!(
        original != target,
        "System output is already BlackHole. Use 'maris run' for a manually configured route"
    );
    let journal = Journal {
        original_uid: string(original, UID)?,
        target_uid: string(target, UID)?,
    };
    store.write_json("route.json", &journal)?;
    let guard = RouteGuard {
        store: store.clone(),
    };
    set_output(target)?;
    ensure!(
        default_output()? == target,
        "System output did not switch to BlackHole"
    );
    Ok(guard)
}
pub fn restore(store: &Store) -> Result<()> {
    let path = store.directory.join("route.json");
    if !path.exists() {
        return Ok(());
    }
    let journal: Journal = read_json(&path)?;
    // Respect a device selection the user made while Maris was running.
    if string(default_output()?, UID)? == journal.target_uid {
        set_output(find(UID, &journal.original_uid)?)?;
    }
    std::fs::remove_file(path)?;
    Ok(())
}
impl Drop for RouteGuard {
    fn drop(&mut self) {
        if let Err(error) = restore(&self.store) {
            eprintln!("Maris could not restore the audio route: {error:#}. Run 'maris restore'.");
        }
    }
}
