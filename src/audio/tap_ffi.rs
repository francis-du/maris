//! Minimal macOS CoreAudio ABI used by the native system-audio backend.
//! See Apple's AudioHardware.h and AudioHardwareTapping.h. New tap symbols are runtime-resolved.
use anyhow::{bail, ensure, Context, Result};
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::{ffi::c_void, mem::size_of, ptr};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Address {
    pub selector: u32,
    pub scope: u32,
    pub element: u32,
}
pub fn address(selector: &[u8; 4]) -> Address {
    address_scope(selector, b"glob")
}
pub fn address_scope(selector: &[u8; 4], scope: &[u8; 4]) -> Address {
    Address {
        selector: u32::from_be_bytes(*selector),
        scope: u32::from_be_bytes(*scope),
        element: 0,
    }
}
#[repr(C)]
#[derive(Default, Copy, Clone, Debug, PartialEq)]
pub struct Format {
    pub rate: f64,
    pub id: u32,
    pub flags: u32,
    pub bytes_packet: u32,
    pub frames_packet: u32,
    pub bytes_frame: u32,
    pub channels: u32,
    pub bits: u32,
    pub reserved: u32,
}
// AudioTimeStamp from CoreAudioTypes.h: SMPTETime occupies 24 bytes;
// kAudioTimeStampSampleTimeValid is bit zero. No timestamp is invented when absent.
#[repr(C)]
#[derive(Default)]
pub struct TimeStamp {
    pub sample_time: f64,
    pub host_time: u64,
    pub rate_scalar: f64,
    pub word_clock_time: u64,
    pub smpte_time: [u8; 24],
    pub flags: u32,
    pub reserved: u32,
}
pub type PropertyListener = unsafe extern "C" fn(u32, u32, *const Address, *mut c_void) -> i32;
pub const SYSTEM_OBJECT: u32 = 1;
#[repr(C)]
pub struct Buffer {
    pub channels: u32,
    pub bytes: u32,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct BufferList {
    pub count: u32,
    pub buffers: [Buffer; 1],
}
pub type IoProc = unsafe extern "C" fn(
    u32,
    *const c_void,
    *const BufferList,
    *const c_void,
    *mut BufferList,
    *const c_void,
    *mut c_void,
) -> i32;
pub type IoId = *mut c_void;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    pub fn AudioObjectGetPropertyDataSize(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    pub fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    pub fn AudioObjectSetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
    pub fn AudioObjectAddPropertyListener(
        object: u32,
        address: *const Address,
        listener: PropertyListener,
        context: *mut c_void,
    ) -> i32;
    pub fn AudioObjectRemovePropertyListener(
        object: u32,
        address: *const Address,
        listener: PropertyListener,
        context: *mut c_void,
    ) -> i32;
    pub fn AudioHardwareCreateAggregateDevice(description: *const c_void, id: *mut u32) -> i32;
    pub fn AudioHardwareDestroyAggregateDevice(id: u32) -> i32;
    pub fn AudioDeviceCreateIOProcID(
        device: u32,
        proc: IoProc,
        client: *mut c_void,
        id: *mut IoId,
    ) -> i32;
    pub fn AudioDeviceDestroyIOProcID(device: u32, id: IoId) -> i32;
    pub fn AudioDeviceStart(device: u32, id: IoId) -> i32;
    pub fn AudioDeviceStop(device: u32, id: IoId) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

pub fn check(status: i32, operation: &str) -> Result<()> {
    ensure!(status == 0, "{operation} failed (CoreAudio {status}). System audio remains managed by macOS; no virtual driver is required");
    Ok(())
}
// Only used with fixed-width numeric/ASBD property types declared in the Apple headers.
pub unsafe fn property<T: Default>(object: u32, selector: &[u8; 4]) -> Result<T> {
    let mut result = T::default();
    let mut bytes = size_of::<T>() as u32;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(selector),
                0,
                ptr::null(),
                &mut bytes,
                (&mut result as *mut T).cast(),
            )
        },
        "Read audio property",
    )?;
    ensure!(
        bytes as usize == size_of::<T>(),
        "Unexpected CoreAudio property size"
    );
    Ok(result)
}
unsafe fn property_vec_u32(object: u32, selector: &[u8; 4]) -> Result<Vec<u32>> {
    unsafe { property_vec_u32_scope(object, selector, b"glob") }
}
unsafe fn property_vec_u32_scope(
    object: u32,
    selector: &[u8; 4],
    scope: &[u8; 4],
) -> Result<Vec<u32>> {
    let mut bytes = 0_u32;
    check(
        unsafe {
            AudioObjectGetPropertyDataSize(
                object,
                &address_scope(selector, scope),
                0,
                ptr::null(),
                &mut bytes,
            )
        },
        "Read audio property size",
    )?;
    ensure!(
        bytes <= 256 * 1024 && bytes.is_multiple_of(size_of::<u32>() as u32),
        "Unexpected CoreAudio list property size"
    );
    let mut values = vec![0_u32; bytes as usize / size_of::<u32>()];
    if bytes == 0 {
        return Ok(values);
    }
    let mut actual = bytes;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                object,
                &address_scope(selector, scope),
                0,
                ptr::null(),
                &mut actual,
                values.as_mut_ptr().cast(),
            )
        },
        "Read audio list property",
    )?;
    ensure!(
        actual <= bytes && actual.is_multiple_of(size_of::<u32>() as u32),
        "Unexpected CoreAudio list result size"
    );
    values.truncate(actual as usize / size_of::<u32>());
    Ok(values)
}

fn string_property(object: u32, selector: &[u8; 4]) -> Result<String> {
    let mut value: *const NSString = ptr::null();
    let mut bytes = size_of::<*const NSString>() as u32;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                object,
                &address(selector),
                0,
                ptr::null(),
                &mut bytes,
                (&mut value as *mut *const NSString).cast(),
            )
        },
        "Read audio string property",
    )?;
    ensure!(
        !value.is_null() && bytes as usize == size_of::<*const NSString>(),
        "Audio string property is unavailable"
    );
    let text = unsafe { (&*value).to_string() };
    unsafe { CFRelease(value.cast()) };
    Ok(text)
}

pub fn default_output() -> Result<u32> {
    unsafe { property(1, b"dOut") }
}
pub fn device_ids() -> Result<Vec<u32>> {
    unsafe { property_vec_u32(1, b"dev#") }
}
/// Disable the physical clock device's inputs and all aggregate outputs before
/// starting the tap. Unused streams arrive as NULL buffers (AudioHardware.h).
pub fn isolate_tap_io(aggregate: u32, io: IoId, clock_uid: &str) -> Result<u32> {
    let clock = output_device_id_for_uid(clock_uid)?;
    let physical_inputs = unsafe { property_vec_u32_scope(clock, b"stm#", b"inp ") }?.len();
    let input_streams = unsafe { property_vec_u32_scope(aggregate, b"stm#", b"inp ") }?;
    let inputs = input_streams.len();
    ensure!(
        inputs > physical_inputs && inputs - physical_inputs <= 2,
        "Unexpected aggregate input layout; refusing physical input capture"
    );
    // The physical sub-device precedes the appended sub-tap streams.
    // Use their actual aggregate virtual format, which may be converted to the
    // output clock rate by CoreAudio, rather than the standalone tap's rate.
    let mut rate = None;
    let mut channels = 0;
    for stream in &input_streams[physical_inputs..] {
        let format: Format = unsafe { property(*stream, b"sfmt")? };
        ensure!(
            format.id == u32::from_be_bytes(*b"lpcm")
                && format.flags & 1 != 0
                && format.flags & 2 == 0
                && format.bits == 32
                && (1..=2).contains(&format.channels)
                && format.rate.is_finite()
                && (44100.0..=192000.0).contains(&format.rate),
            "Unsupported aggregate tap PCM format"
        );
        ensure!(
            rate.is_none_or(|value| value == format.rate),
            "Aggregate tap streams have different rates"
        );
        rate = Some(format.rate);
        channels += format.channels;
    }
    ensure!(
        (1..=2).contains(&channels),
        "Unsupported aggregate tap channel layout"
    );
    set_stream_usage(aggregate, io, b"inp ", inputs, physical_inputs)?;
    let outputs = output_stream_ids(aggregate)?.len();
    set_stream_usage(aggregate, io, b"outp", outputs, outputs)?;
    Ok(rate.context("Aggregate tap has no input stream")?.round() as u32)
}

fn set_stream_usage(
    device: u32,
    io: IoId,
    scope: &[u8; 4],
    streams: usize,
    disabled_prefix: usize,
) -> Result<()> {
    ensure!(
        streams <= 128 && disabled_prefix <= streams,
        "Unsupported aggregate stream count"
    );
    if streams == 0 {
        return Ok(());
    }
    // AudioHardwareIOProcStreamUsage: pointer, UInt32 count, UInt32 flags.
    // Word storage provides pointer alignment and is allocated on the control thread.
    let header = size_of::<IoId>() + size_of::<u32>();
    let bytes = header + streams * size_of::<u32>();
    let mut words = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
    let data = words.as_mut_ptr().cast::<u8>();
    unsafe {
        data.cast::<IoId>().write(io);
        data.add(size_of::<IoId>())
            .cast::<u32>()
            .write(streams as u32);
        let flags = data.add(header).cast::<u32>();
        for index in disabled_prefix..streams {
            flags.add(index).write(1);
        }
        check(
            AudioObjectSetPropertyData(
                device,
                &address_scope(b"suse", scope),
                0,
                ptr::null(),
                bytes as u32,
                data.cast(),
            ),
            "Isolate system tap from physical inputs and outputs",
        )?;
        let mut actual = bytes as u32;
        check(
            AudioObjectGetPropertyData(
                device,
                &address_scope(b"suse", scope),
                0,
                ptr::null(),
                &mut actual,
                data.cast(),
            ),
            "Verify isolated system tap stream usage",
        )?;
        ensure!(
            actual as usize == bytes,
            "Unexpected stream usage property size"
        );
        ensure!(
            data.cast::<IoId>().read() == io
                && data.add(size_of::<IoId>()).cast::<u32>().read() as usize == streams,
            "Stream usage no longer matches the tap callback"
        );
        for index in 0..streams {
            ensure!(
                (flags.add(index).read() != 0) == (index >= disabled_prefix),
                "CoreAudio did not isolate the system tap streams"
            );
        }
        Ok(())
    }
}

pub fn output_stream_ids(device: u32) -> Result<Vec<u32>> {
    unsafe { property_vec_u32_scope(device, b"stm#", b"outp") }
}
fn scoped_output_stream_supported(stream_count: usize, channels: u32) -> bool {
    stream_count == 1 && (1..=2).contains(&channels)
}

pub fn single_stereo_output_device_uid(device: u32) -> Result<Option<String>> {
    let streams = output_stream_ids(device)?;
    if streams.len() != 1 {
        return Ok(None);
    }
    let format: Format = unsafe { property(streams[0], b"sfmt")? };
    if !scoped_output_stream_supported(streams.len(), format.channels) {
        return Ok(None);
    }
    Ok(Some(device_uid(device)?))
}
pub fn output_device_ids() -> Result<Vec<u32>> {
    let mut result = Vec::new();
    for device in device_ids()? {
        let mut bytes = 0_u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                device,
                &address_scope(b"stm#", b"outp"),
                0,
                ptr::null(),
                &mut bytes,
            )
        };
        if status == 0 && bytes >= size_of::<u32>() as u32 {
            result.push(device);
        }
    }
    Ok(result)
}
pub fn output_device_id_at(index: usize) -> Result<u32> {
    output_device_ids()?
        .into_iter()
        .nth(index)
        .context("CoreAudio output index is no longer available")
}
pub fn output_device_id_for_uid(uid: &str) -> Result<u32> {
    ensure!(
        !uid.is_empty() && uid.len() <= 512 && !uid.chars().any(char::is_control),
        "Invalid CoreAudio output UID"
    );
    let mut found = None;
    for device in output_device_ids()? {
        if device_uid(device)
            .as_deref()
            .is_ok_and(|value| value == uid)
        {
            ensure!(found.is_none(), "CoreAudio output UID is ambiguous");
            found = Some(device);
        }
    }
    found.context("Pinned CoreAudio output is no longer available")
}
pub fn output_index_for_uid(uid: &str) -> Result<usize> {
    let target = output_device_id_for_uid(uid)?;
    output_device_ids()?
        .into_iter()
        .position(|device| device == target)
        .context("Pinned CoreAudio output is no longer available")
}
pub fn output_device_id_for_selector(selector: Option<&str>, expected_name: &str) -> Result<u32> {
    let device = if let Some(uid) = selector.and_then(|selector| selector.strip_prefix("uid:")) {
        output_device_id_for_uid(uid)?
    } else if let Some(index) = selector
        .and_then(|selector| selector.strip_prefix("output:"))
        .and_then(|index| index.parse::<usize>().ok())
    {
        output_device_id_at(index)?
    } else if selector.is_none() {
        default_output()?
    } else {
        device_id_named(expected_name)?
    };
    ensure!(
        device_name(device).is_ok_and(|name| name == expected_name),
        "CoreAudio output identity no longer matches the selected endpoint"
    );
    Ok(device)
}
pub fn output_selector_for_uid(uid: &str) -> Result<String> {
    let _ = output_device_id_for_uid(uid)?;
    Ok(format!("uid:{uid}"))
}
pub fn process_ids() -> Result<Vec<u32>> {
    unsafe { property_vec_u32(1, b"prs#") }
}
pub fn sample_rate(device: u32) -> Result<u32> {
    let rate: f64 = unsafe { property(device, b"nsrt")? };
    ensure!(
        rate.is_finite() && (44100.0..=192000.0).contains(&rate),
        "Unsupported current output sample rate"
    );
    Ok(rate.round() as u32)
}
pub fn device_name(device: u32) -> Result<String> {
    string_property(device, b"lnam").context("Read output name")
}
pub fn device_uid(device: u32) -> Result<String> {
    string_property(device, b"uid ").context("Read output device UID")
}
pub fn device_model_uid(device: u32) -> Result<String> {
    string_property(device, b"muid").context("Read output model UID")
}
pub fn device_id_named(name: &str) -> Result<u32> {
    let mut matches = device_ids()?
        .into_iter()
        .filter(|device| device_name(*device).is_ok_and(|candidate| candidate == name));
    let first = matches
        .next()
        .with_context(|| format!("CoreAudio device '{name}' is unavailable"))?;
    if matches.next().is_some() {
        bail!("CoreAudio device name '{name}' is ambiguous");
    }
    Ok(first)
}
pub fn output_buffer_frames(device: u32) -> Result<u32> {
    let frames: u32 = unsafe { property(device, b"fsiz")? };
    ensure!(
        frames > 0 && frames <= 65_536,
        "Invalid output buffer frame size"
    );
    Ok(frames)
}
pub fn output_latency_frames(device: u32) -> Result<u32> {
    let mut result = 0_u32;
    let mut bytes = size_of::<u32>() as u32;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                device,
                &address_scope(b"ltnc", b"outp"),
                0,
                ptr::null(),
                &mut bytes,
                (&mut result as *mut u32).cast(),
            )
        },
        "Read output latency",
    )?;
    ensure!(
        bytes as usize == size_of::<u32>(),
        "Unexpected output latency size"
    );
    Ok(result)
}
pub fn output_safety_offset_frames(device: u32) -> Result<u32> {
    let mut result = 0_u32;
    let mut bytes = size_of::<u32>() as u32;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                device,
                &address_scope(b"saft", b"outp"),
                0,
                ptr::null(),
                &mut bytes,
                (&mut result as *mut u32).cast(),
            )
        },
        "Read output safety offset",
    )?;
    ensure!(
        bytes as usize == size_of::<u32>(),
        "Unexpected output safety offset size"
    );
    Ok(result)
}
pub fn applications() -> Result<Vec<super::ApplicationInfo>> {
    let own = own_process().ok();
    let mut applications = Vec::new();
    for object_id in process_ids()? {
        let pid = process_pid(object_id).unwrap_or(0);
        let bundle_id = string_property(object_id, b"pbid")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let maris_bundle = bundle_id.as_deref() == Some("audio.maris.app");
        let running_output = unsafe { property::<u32>(object_id, b"piro") }
            .map(|value| value != 0)
            .unwrap_or(false);
        let devices = unsafe { property_vec_u32_scope(object_id, b"pdv#", b"outp") }
            .unwrap_or_default()
            .into_iter()
            .filter_map(|device| device_name(device).ok())
            .collect();
        applications.push(super::ApplicationInfo {
            object_id,
            pid,
            bundle_id,
            running_output,
            devices,
            is_maris: own == Some(object_id) || pid == std::process::id() as i32 || maris_bundle,
        });
    }
    applications.sort_by(|a, b| {
        b.running_output
            .cmp(&a.running_output)
            .then_with(|| a.bundle_id.cmp(&b.bundle_id))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    Ok(applications)
}

pub fn process_pid(object_id: u32) -> Result<i32> {
    ensure!(object_id != 0, "Invalid audio process object");
    let pid: i32 = unsafe { property(object_id, b"ppid")? };
    ensure!(pid > 0, "Audio process PID is unavailable");
    Ok(pid)
}

pub fn process_object_for_pid(pid: i32) -> Result<u32> {
    ensure!(pid > 0, "Process PID must be positive");
    let mut id = 0_u32;
    let mut bytes = 4;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                1,
                &address(b"id2p"),
                4,
                (&pid as *const i32).cast(),
                &mut bytes,
                (&mut id as *mut u32).cast(),
            )
        },
        "Resolve audio process",
    )?;
    ensure!(id != 0 && bytes == 4, "Audio process is unavailable");
    Ok(id)
}

pub fn own_process() -> Result<u32> {
    process_object_for_pid(std::process::id() as i32)
        .context("Cannot exclude Maris from system capture; refusing a feedback-prone route")
}
pub type CreateTap = unsafe extern "C" fn(*const AnyObject, *mut u32) -> i32;
pub type DestroyTap = unsafe extern "C" fn(u32) -> i32;
#[derive(Clone, Copy)]
pub struct TapApi {
    pub create: CreateTap,
    pub destroy: DestroyTap,
}
impl TapApi {
    pub fn load() -> Result<Self> {
        // SAFETY: symbols are resolved from the linked CoreAudio framework and cast to its documented ABI.
        unsafe {
            let create = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"AudioHardwareCreateProcessTap".as_ptr(),
            );
            let destroy = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"AudioHardwareDestroyProcessTap".as_ptr(),
            );
            ensure!(
                !create.is_null() && !destroy.is_null(),
                "Native system audio requires macOS 14.2 or newer"
            );
            Ok(Self {
                create: std::mem::transmute::<*mut c_void, CreateTap>(create),
                destroy: std::mem::transmute::<*mut c_void, DestroyTap>(destroy),
            })
        }
    }
}
pub fn object(class: &std::ffi::CStr) -> Result<Retained<AnyObject>> {
    let class = objc2::runtime::AnyClass::get(class)
        .context("Required macOS audio class is unavailable")?;
    // SAFETY: Foundation mutable containers support new and return retained objects.
    Ok(unsafe { msg_send![class, new] })
}
pub fn set(dictionary: &AnyObject, key: &str, value: &AnyObject) {
    // SAFETY: callers supply NSMutableDictionary, an NSString key, and live NSObject values.
    unsafe {
        let _: () = msg_send![dictionary, setObject: value, forKey: &*NSString::from_str(key)];
    }
}
pub fn append(array: &AnyObject, value: &AnyObject) {
    // SAFETY: callers supply NSMutableArray and a live object; the array retains it.
    unsafe {
        let _: () = msg_send![array, addObject: value];
    }
}

#[cfg(test)]
mod scoped_output_tests {
    use super::scoped_output_stream_supported;

    #[test]
    fn scoped_output_requires_exactly_one_mono_or_stereo_stream() {
        for channels in [1, 2] {
            assert!(scoped_output_stream_supported(1, channels));
        }
        for stream_count in [0, 2, 3] {
            for channels in [1, 2, 6, 8] {
                assert!(!scoped_output_stream_supported(stream_count, channels));
            }
        }
        for channels in [0, 3, 4, 6, 8, 16] {
            assert!(!scoped_output_stream_supported(1, channels));
        }
    }
}
