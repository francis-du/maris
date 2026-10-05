//! Driver-free system capture. Private process tap, self-exclusion, and mute-only-while-tapped.
//! No default-output switch, microphone capture, user-created aggregate, or installed driver.
use super::{tap_ffi as ca, Metrics};
use anyhow::{ensure, Context, Result};
use crossbeam_queue::ArrayQueue;
use objc2::{
    msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
    sel,
};
use objc2_foundation::{NSNumber, NSString};
use std::{
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

struct Capture {
    queue: Arc<ArrayQueue<[f32; 2]>>,
    metrics: Arc<Metrics>,
    next_sample: AtomicU64,
}

fn device_list_changed(object: u32, addresses: &[ca::Address]) -> bool {
    object == ca::SYSTEM_OBJECT
        && addresses
            .iter()
            .any(|address| address.selector == u32::from_be_bytes(*b"dev#"))
}

unsafe extern "C" fn configuration_event(
    object: u32,
    count: u32,
    addresses: *const ca::Address,
    context: *mut c_void,
) -> i32 {
    if !context.is_null() {
        // Listener state shares the capture's owned lifetime; no native queries,
        // locks, allocations or profile writes occur on this callback.
        let capture = unsafe { &*context.cast::<Capture>() };
        capture
            .metrics
            .configuration_events
            .fetch_add(1, Ordering::Release);
        if object == ca::SYSTEM_OBJECT && !addresses.is_null() {
            let addresses = unsafe { std::slice::from_raw_parts(addresses, count as usize) };
            if device_list_changed(object, addresses) {
                capture
                    .metrics
                    .device_list_events
                    .fetch_add(1, Ordering::Release);
            }
        }
    }
    0
}

#[derive(Clone, Debug, PartialEq)]
struct OutputConfiguration {
    rate: u32,
    buffer: Option<u32>,
    latency: Option<u32>,
    safety: Option<u32>,
    alive: Option<u32>,
}
impl OutputConfiguration {
    fn read(device: u32) -> Result<Self> {
        Ok(Self {
            rate: ca::sample_rate(device)?,
            buffer: ca::output_buffer_frames(device).ok(),
            latency: ca::output_latency_frames(device).ok(),
            safety: ca::output_safety_offset_frames(device).ok(),
            alive: unsafe { ca::property(device, b"livn") }.ok(),
        })
    }
}

fn default_output_changed(expected: Option<u32>, current: Option<u32>) -> bool {
    expected.is_some() && expected != current
}

fn validate_process_set(processes: &[u32], available: &[u32], own: u32) -> Result<()> {
    ensure!(
        !processes.is_empty() && processes.len() <= 64,
        "Select between 1 and 64 audio process objects"
    );
    for (index, process) in processes.iter().copied().enumerate() {
        ensure!(
            process != 0 && process != own,
            "Invalid audio process object {process}"
        );
        ensure!(
            !processes[..index].contains(&process),
            "Duplicate audio process object {process}"
        );
        ensure!(
            available.contains(&process),
            "Audio process object {process} is no longer available"
        );
    }
    Ok(())
}

fn validate_processes(processes: &[u32]) -> Result<()> {
    let available = ca::process_ids()?;
    let own = ca::own_process()?;
    validate_process_set(processes, &available, own)
}

pub(super) struct TapCapture {
    api: ca::TapApi,
    tap: u32,
    aggregate: u32,
    io: ca::IoId,
    context: Option<Box<Capture>>,
    started: bool,
    format: ca::Format,
    aggregate_rate: u32,
    aggregate_buffer: Option<u32>,
    output_configuration: Option<(u32, OutputConfiguration)>,
    expected_default_output: Option<u32>,
    listeners: Vec<(u32, ca::Address)>,
    scoped_output: bool,
    pub rate: u32,
}
fn tap_mode(scoped_output: bool) -> &'static str {
    if scoped_output {
        "device_stream"
    } else {
        "global_mixdown"
    }
}
fn scoped_initializer_supported(class: &AnyClass, selected: bool) -> bool {
    if selected {
        class.responds_to(sel!(initWithProcesses:andDeviceUID:withStream:))
    } else {
        class.responds_to(sel!(initExcludingProcesses:andDeviceUID:withStream:))
    }
}

impl TapCapture {
    pub fn prepare(queue: Arc<ArrayQueue<[f32; 2]>>, metrics: Arc<Metrics>) -> Result<Self> {
        Self::prepare_scope(queue, metrics, None, None)
    }

    pub fn prepare_for_output(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
        device_uid: &str,
    ) -> Result<Self> {
        Self::prepare_scope(queue, metrics, None, Some((device_uid, 0)))
    }

    pub fn prepare_processes(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
        processes: &[u32],
    ) -> Result<Self> {
        validate_processes(processes)?;
        Self::prepare_scope(queue, metrics, Some(processes), None)
    }

    pub fn prepare_processes_for_output(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
        processes: &[u32],
        device_uid: &str,
    ) -> Result<Self> {
        validate_processes(processes)?;
        Self::prepare_scope(queue, metrics, Some(processes), Some((device_uid, 0)))
    }

    pub fn capture_mode(&self) -> &'static str {
        tap_mode(self.scoped_output)
    }

    fn prepare_scope(
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
        processes: Option<&[u32]>,
        output: Option<(&str, usize)>,
    ) -> Result<Self> {
        let api = ca::TapApi::load()?;
        let own = ca::own_process()?;
        let class =
            AnyClass::get(c"CATapDescription").context("Native audio taps require macOS 14.2+")?;
        let listed = ca::object(c"NSMutableArray")?;
        let selected = if let Some(processes) = processes {
            for process in processes {
                ca::append(&listed, &NSNumber::new_u32(*process));
            }
            true
        } else {
            ca::append(&listed, &NSNumber::new_u32(own));
            false
        };
        // Apple's device/stream-scoped initializers were publicly usable only from
        // macOS 14.5 despite earlier SDK availability annotations. Probe the Objective-C
        // runtime so the supported 14.2-14.4 range safely retains the global-tap fallback.
        let output = output.filter(|_| scoped_initializer_supported(class, selected));
        let global_description = || -> Result<Retained<AnyObject>> {
            // SAFETY: the global initializers and argument types follow CATapDescription.h.
            unsafe {
                let allocated: Allocated<AnyObject> = msg_send![class, alloc];
                let description: Option<Retained<AnyObject>> = if selected {
                    msg_send![allocated, initStereoMixdownOfProcesses: &*listed]
                } else {
                    msg_send![allocated, initStereoGlobalTapButExcludeProcesses: &*listed]
                };
                description.context("macOS did not create a global audio tap description")
            }
        };
        // SAFETY: selectors and NSInteger/BOOL types follow CATapDescription.h and the
        // scoped selectors are sent only after the runtime confirms their availability.
        // When one unambiguous stereo output stream is known, bind capture to that
        // stream instead of asking CoreAudio to perform a global stereo mixdown.
        let (mut description, scoped_output): (Retained<AnyObject>, bool) = unsafe {
            let scoped: Option<Retained<AnyObject>> = if let Some((device_uid, stream)) = output {
                let allocated: Allocated<AnyObject> = msg_send![class, alloc];
                let device_uid = NSString::from_str(device_uid);
                if selected {
                    msg_send![allocated,
                        initWithProcesses: &*listed,
                        andDeviceUID: &*device_uid,
                        withStream: stream as isize]
                } else {
                    msg_send![allocated,
                        initExcludingProcesses: &*listed,
                        andDeviceUID: &*device_uid,
                        withStream: stream as isize]
                }
            } else {
                None
            };
            match scoped {
                Some(description) => (description, true),
                None => (global_description()?, false),
            }
        };
        let configure_description = |description: &Retained<AnyObject>| {
            // SAFETY: setters and argument types follow CATapDescription.h.
            unsafe {
                let name = if selected {
                    "Maris Application Audio"
                } else {
                    "Maris System Audio"
                };
                let _: () = msg_send![description, setName: &*NSString::from_str(name)];
                let _: () = msg_send![description, setPrivate: true];
                // Exclusive means all processes except those listed. Included-process taps keep it false.
                let _: () = msg_send![description, setExclusive: !selected];
                // CATapMutedWhenTapped = 2. Untapped applications regain normal playback automatically.
                let _: () = msg_send![description, setMuteBehavior: 2_isize];
            }
        };
        configure_description(&description);
        let mut result = Self {
            api,
            tap: 0,
            aggregate: 0,
            io: ptr::null_mut(),
            context: None,
            started: false,
            format: ca::Format::default(),
            aggregate_rate: 0,
            aggregate_buffer: None,
            output_configuration: None,
            expected_default_output: None,
            listeners: Vec::with_capacity(12),
            scoped_output,
            rate: 0,
        };
        let mut create_status = unsafe { (api.create)(&*description, &mut result.tap) };
        if create_status != 0 && scoped_output {
            // Some devices accept the scoped description but reject process-tap creation.
            // Keep the established global path as a compatibility fallback.
            if result.tap != 0 {
                let _ = unsafe { (api.destroy)(result.tap) };
                result.tap = 0;
            }
            description = global_description()?;
            configure_description(&description);
            result.scoped_output = false;
            create_status = unsafe { (api.create)(&*description, &mut result.tap) };
        }
        ca::check(create_status, "Request system-audio permission")?;
        ensure!(result.tap != 0, "macOS did not create an audio tap");
        let format: ca::Format = unsafe { ca::property(result.tap, b"tfmt")? };
        ensure!(
            format.id == u32::from_be_bytes(*b"lpcm")
                && format.flags & 1 != 0
                && format.flags & 2 == 0
                && format.bits == 32
                && (1..=2).contains(&format.channels),
            "macOS returned an unsupported tap format; keeping original audio"
        );
        ensure!(
            format.rate.is_finite() && (44100.0..=192000.0).contains(&format.rate),
            "Unsupported system audio rate"
        );
        result.rate = format.rate.round() as u32;
        result.format = format;
        let uuid: Retained<AnyObject> = unsafe { msg_send![&description, UUID] };
        let uid: Retained<NSString> = unsafe { msg_send![&uuid, UUIDString] };
        let sub = ca::object(c"NSMutableDictionary")?;
        ca::set(&sub, "uid", &uid);
        ca::set(&sub, "drift", &NSNumber::new_bool(true));
        let taps = ca::object(c"NSMutableArray")?;
        ca::append(&taps, &sub);
        let dictionary = ca::object(c"NSMutableDictionary")?;
        ca::set(
            &dictionary,
            "name",
            &NSString::from_str("Maris Private System Tap"),
        );
        ca::set(
            &dictionary,
            "uid",
            &NSString::from_str(&format!("audio.maris.tap.{}", uid)),
        );
        ca::set(&dictionary, "private", &NSNumber::new_bool(true));
        ca::set(&dictionary, "tapautostart", &NSNumber::new_bool(true));
        ca::set(&dictionary, "taps", &taps);
        // A tap-only private aggregate contains no physical microphone input.
        ca::check(
            unsafe {
                ca::AudioHardwareCreateAggregateDevice(
                    (&*dictionary as *const AnyObject).cast(),
                    &mut result.aggregate,
                )
            },
            "Create private audio connection",
        )?;
        ensure!(
            result.aggregate != 0,
            "macOS did not create the private audio connection"
        );
        result.aggregate_rate = ca::sample_rate(result.aggregate)?;
        result.aggregate_buffer = ca::output_buffer_frames(result.aggregate).ok();
        result.context = Some(Box::new(Capture {
            queue,
            metrics,
            next_sample: AtomicU64::new(f64::NAN.to_bits()),
        }));
        let context = (&**result
            .context
            .as_ref()
            .expect("capture context initialized") as *const Capture)
            .cast_mut();
        ca::check(
            unsafe {
                ca::AudioDeviceCreateIOProcID(
                    result.aggregate,
                    capture,
                    context.cast(),
                    &mut result.io,
                )
            },
            "Connect system-audio callback",
        )?;
        result.watch(result.tap, ca::address(b"tfmt"))?;
        result.watch(result.aggregate, ca::address(b"nsrt"))?;
        if result.aggregate_buffer.is_some() {
            result.watch(result.aggregate, ca::address(b"fsiz"))?;
        }
        Ok(result)
    }
    fn watch(&mut self, object: u32, address: ca::Address) -> Result<()> {
        ensure!(
            self.listeners.len() < 12,
            "Too many native configuration listeners"
        );
        // Existing listeners may already access this state through shared references.
        // Never form an exclusive Rust reference merely to obtain the FFI pointer.
        let context = (&**self.context.as_ref().context("Missing capture state")?
            as *const Capture)
            .cast_mut();
        ca::check(
            unsafe {
                ca::AudioObjectAddPropertyListener(
                    object,
                    &address,
                    configuration_event,
                    context.cast(),
                )
            },
            "Observe audio configuration",
        )?;
        self.listeners.push((object, address));
        Ok(())
    }
    pub fn watch_output(&mut self, device: u32, follow_default: bool) -> Result<()> {
        let configuration = OutputConfiguration::read(device)?;
        // Device insertion/removal can disturb the CoreAudio graph even when the
        // active output's own format does not change. Quarantine PCM immediately;
        // the control thread later decides whether a rebuild is actually required.
        self.watch(ca::SYSTEM_OBJECT, ca::address(b"dev#"))?;
        if follow_default {
            self.expected_default_output = Some(device);
            self.watch(ca::SYSTEM_OBJECT, ca::address(b"dOut"))?;
        }
        self.watch(device, ca::address(b"nsrt"))?;
        // A driver need not expose every optional latency property. Observe only
        // supported properties rather than making their absence prevent playback.
        for (selector, value) in [
            (b"fsiz", configuration.buffer),
            (b"livn", configuration.alive),
        ] {
            if value.is_some() {
                self.watch(device, ca::address(selector))?;
            }
        }
        for (selector, value) in [
            (b"ltnc", configuration.latency),
            (b"saft", configuration.safety),
        ] {
            if value.is_some() {
                self.watch(device, ca::address_scope(selector, b"outp"))?;
            }
        }
        self.output_configuration = Some((device, configuration));
        Ok(())
    }
    /// Called only by the control thread. A concurrent notification cannot be
    /// accidentally acknowledged by checking an earlier configuration generation.
    pub fn needs_rebuild(&self) -> Result<bool> {
        let metrics = &self
            .context
            .as_ref()
            .context("Missing capture state")?
            .metrics;
        let generation = metrics.configuration_events.load(Ordering::Acquire);
        let current: ca::Format = unsafe { ca::property(self.tap, b"tfmt")? };
        // A system device-list event means CoreAudio may have rebuilt the HAL
        // graph even when this output keeps the same ID/rate. Do not resume PCM
        // through that old graph; rebuild the quarantined pipeline instead.
        let topology_changed = metrics.device_list_events.load(Ordering::Acquire) > 0;
        let mut changed = topology_changed
            || current != self.format
            || ca::sample_rate(self.aggregate)? != self.aggregate_rate
            || ca::output_buffer_frames(self.aggregate).ok() != self.aggregate_buffer;
        if let Some((device, expected)) = &self.output_configuration {
            changed |= OutputConfiguration::read(*device)? != *expected;
        }
        changed |= self.expected_default_output.is_some_and(|expected| {
            default_output_changed(Some(expected), ca::default_output().ok())
        });
        if !changed {
            metrics
                .checked_configuration_events
                .store(generation, Ordering::Release);
        }
        Ok(changed)
    }
    pub fn configuration_pending(&self) -> bool {
        self.context.as_ref().is_some_and(|capture| {
            capture.metrics.configuration_events.load(Ordering::Acquire)
                != capture
                    .metrics
                    .checked_configuration_events
                    .load(Ordering::Acquire)
        })
    }

    pub fn start(&mut self) -> Result<()> {
        ensure!(!self.started, "Capture already started");
        // Close the read-before-listener-registration window before decoding any
        // live buffer. A changed initial format cannot silently start on stale DSP.
        ensure!(
            !self.needs_rebuild()?,
            "Audio configuration changed before capture startup; retry"
        );
        ca::check(
            unsafe { ca::AudioDeviceStart(self.aggregate, self.io) },
            "Start system-audio capture (allow Maris in the macOS permission dialog)",
        )?;
        self.started = true;
        Ok(())
    }
}
impl Drop for TapCapture {
    fn drop(&mut self) {
        // Stop before destroying the callback or freeing its context. No permanent output state was changed.
        unsafe {
            if self.started {
                let _ = ca::AudioDeviceStop(self.aggregate, self.io);
            }
            let mut removed = true;
            let context = self.context.as_ref().map_or(ptr::null_mut(), |capture| {
                (&**capture as *const Capture).cast_mut().cast()
            });
            for (object, address) in self.listeners.drain(..) {
                removed &= ca::AudioObjectRemovePropertyListener(
                    object,
                    &address,
                    configuration_event,
                    context,
                ) == 0;
            }
            removed &=
                self.io.is_null() || ca::AudioDeviceDestroyIOProcID(self.aggregate, self.io) == 0;
            if self.aggregate != 0 {
                let _ = ca::AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if self.tap != 0 {
                let _ = (self.api.destroy)(self.tap);
            }
            if !removed {
                // On a native teardown failure prefer one bounded leak to a callback use-after-free.
                if let Some(context) = self.context.take() {
                    let _ = Box::into_raw(context);
                }
            }
        }
    }
}

unsafe extern "C" fn capture(
    _device: u32,
    _now: *const c_void,
    input: *const ca::BufferList,
    input_time: *const c_void,
    _output: *mut ca::BufferList,
    _output_time: *const c_void,
    context: *mut c_void,
) -> i32 {
    if input.is_null() || context.is_null() {
        return 0;
    }
    // SAFETY: CoreAudio owns input buffers for this call; context remains boxed until IOProc destruction.
    let capture = unsafe { &*(context as *const Capture) };
    if capture.metrics.configuration_events.load(Ordering::Acquire)
        != capture
            .metrics
            .checked_configuration_events
            .load(Ordering::Acquire)
    {
        // Do not decode PCM using a format that the control thread has not checked.
        return 0;
    }
    let list = unsafe { &*input };
    if list.count == 0 {
        return 0;
    }
    if list.count > 2 {
        capture.metrics.errors.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let buffers = unsafe { std::slice::from_raw_parts(list.buffers.as_ptr(), list.count as usize) };
    let first = &buffers[0];
    let planar = buffers.len() == 2;
    if first.data.is_null() || first.bytes == 0 {
        return 0;
    }
    if first.channels == 0
        || first.channels > 2
        || !first.bytes.is_multiple_of(4 * first.channels)
        || !(first.data as usize).is_multiple_of(4)
    {
        capture.metrics.errors.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    if planar
        && (first.channels != 1
            || buffers[1].channels != 1
            || buffers[1].bytes != first.bytes
            || buffers[1].data.is_null()
            || !(buffers[1].data as usize).is_multiple_of(4))
    {
        capture.metrics.errors.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let channels = first.channels as usize;
    let count = first.bytes as usize / 4 / channels;
    if count > 65536 {
        capture.metrics.errors.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let sample_time = if input_time.is_null() {
        None
    } else {
        let stamp = unsafe { &*input_time.cast::<ca::TimeStamp>() };
        (stamp.flags & 1 != 0 && stamp.sample_time.is_finite()).then_some(stamp.sample_time)
    };
    let next = sample_time.map_or(f64::NAN, |time| time + count as f64);
    let expected = f64::from_bits(capture.next_sample.swap(next.to_bits(), Ordering::AcqRel));
    if expected.is_finite() && sample_time.is_none_or(|time| (time - expected).abs() > 0.5) {
        capture
            .metrics
            .capture_discontinuities
            .fetch_add(1, Ordering::Release);
    }
    let left = first.data.cast::<f32>();
    let right = if planar {
        buffers[1].data.cast::<f32>()
    } else {
        left
    };
    let mut dropped = 0_u64;
    for i in 0..count {
        let frame = unsafe {
            let l = *left.add(i * channels);
            let r = if planar {
                *right.add(i)
            } else if channels == 2 {
                *left.add(i * channels + 1)
            } else {
                l
            };
            [
                if l.is_finite() { l } else { 0.0 },
                if r.is_finite() { r } else { 0.0 },
            ]
        };
        if capture.queue.push(frame).is_err() {
            dropped += 1;
        }
    }
    if dropped != 0 {
        // Keep the real-time failure path bounded: one atomic update per callback
        // instead of one contended atomic operation for every dropped frame.
        capture
            .metrics
            .overruns
            .fetch_add(dropped, Ordering::Relaxed);
    }
    capture
        .metrics
        .captured_frames
        .fetch_add(count as u64, Ordering::Relaxed);
    0
}

#[cfg(test)]
#[path = "../../tests/unit/native_tap.rs"]
mod tests;

#[cfg(test)]
mod topology_tests {
    use super::*;

    #[test]
    fn unsupported_scoped_tap_initializers_fall_back_before_message_send() {
        let class = AnyClass::get(c"NSObject").expect("NSObject is always available");
        assert!(!scoped_initializer_supported(class, true));
        assert!(!scoped_initializer_supported(class, false));
    }

    #[test]
    fn tap_mode_distinguishes_device_stream_from_global_mixdown() {
        assert_eq!(tap_mode(true), "device_stream");
        assert_eq!(tap_mode(false), "global_mixdown");
    }

    #[test]
    fn only_system_device_list_notifications_force_topology_rebuild() {
        let devices = [ca::address(b"dev#")];
        let output = [ca::address(b"nsrt")];
        assert!(device_list_changed(ca::SYSTEM_OBJECT, &devices));
        assert!(!device_list_changed(ca::SYSTEM_OBJECT, &output));
        assert!(!device_list_changed(42, &devices));
        assert!(!device_list_changed(ca::SYSTEM_OBJECT, &[]));
    }
}
