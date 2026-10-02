//! Explicit Windows audio-session routing guard.
use anyhow::{ensure, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, RPC_E_CHANGED_MODE, S_OK},
        Media::Audio::{
            eRender, AudioSessionStateActive, AudioSessionStateExpired, AudioSessionStateInactive,
            IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
            MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
                COINIT_MULTITHREADED,
            },
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        },
    },
};

struct OriginalSession {
    volume: ISimpleAudioVolume,
    muted: bool,
}

struct Sessions {
    originals: BTreeMap<String, OriginalSession>,
}
impl Drop for Sessions {
    fn drop(&mut self) {
        restore(&mut self.originals);
        unsafe { CoUninitialize() };
    }
}

pub(super) struct Guard {
    failed: Arc<AtomicBool>,
    worker: super::worker::Worker,
}

impl Guard {
    pub fn start(pids: &[i32]) -> Result<Self> {
        let roots: BTreeSet<u32> = pids
            .iter()
            .map(|pid| u32::try_from(*pid).context("Invalid Windows PID"))
            .collect::<Result<_>>()?;
        let failed = Arc::new(AtomicBool::new(false));
        let thread_failed = failed.clone();
        let initial_roots = roots.clone();
        let worker = super::worker::Worker::start(
            "maris-windows-route",
            Duration::from_secs(5),
            move |stop| {
                unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                    .ok()
                    .context("Initialize Windows route COM apartment")?;
                let mut sessions = Sessions {
                    originals: BTreeMap::new(),
                };
                reconcile(&initial_roots, &mut sessions.originals, stop)?;
                Ok(sessions)
            },
            move |mut sessions, thread_stop| {
                while !thread_stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(200));
                    if thread_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if reconcile(&roots, &mut sessions.originals, thread_stop).is_err() {
                        thread_failed.store(true, Ordering::Release);
                        break;
                    }
                }
            },
        )?;
        Ok(Self { failed, worker })
    }

    pub fn healthy(&self) -> bool {
        !self.failed.load(Ordering::Acquire) && self.worker.healthy()
    }
}

pub(super) fn validate_capture_pids(pids: &[i32]) -> Result<()> {
    ensure!(pids.len() <= 64, "Select at most 64 application PIDs");
    let roots: BTreeSet<u32> = pids
        .iter()
        .map(|pid| {
            ensure!(*pid > 0, "Invalid Windows application PID");
            Ok(*pid as u32)
        })
        .collect::<Result<_>>()?;
    ensure!(roots.len() == pids.len(), "Duplicate application PID");
    if !roots.is_empty() {
        super::windows_scope::selection(&roots, std::process::id(), &process_parents()?)?;
    }
    Ok(())
}

fn process_parents() -> Result<BTreeMap<u32, u32>> {
    Ok(process_snapshot()?
        .into_iter()
        .map(|(pid, (parent, _))| (pid, parent))
        .collect())
}

fn process_snapshot() -> Result<BTreeMap<u32, (u32, String)>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .context("Create Windows process snapshot")?;
    struct Snapshot(HANDLE);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    let snapshot = Snapshot(snapshot);
    let mut processes = BTreeMap::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // windows 0.54 returns Result<()>, not BOOL. Do not turn enumeration
    // failures into an apparently valid empty process tree.
    unsafe { Process32FirstW(snapshot.0, &mut entry) }.context("Read first Windows process")?;
    loop {
        let end = entry
            .szExeFile
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
        processes.insert(entry.th32ProcessID, (entry.th32ParentProcessID, name));
        ensure!(
            processes.len() <= 65_536,
            "Windows process list is too large"
        );
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(error).context("Continue Windows process snapshot"),
        }
    }
    Ok(processes)
}

/// Read audio sessions and process names without opening capture or mute controls.
/// One app may have several sessions; merge by PID and retain all output IDs.
pub(super) fn applications() -> Result<Vec<serde_json::Value>> {
    let processes = process_snapshot()?;
    let parents = processes
        .iter()
        .map(|(&pid, (parent, _))| (pid, *parent))
        .collect();
    let (_, excluded) =
        super::windows_scope::selection(&BTreeSet::new(), std::process::id(), &parents)?;
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    ensure!(
        initialized.is_ok() || initialized == RPC_E_CHANGED_MODE,
        "Initialize Windows audio discovery: {initialized:?}"
    );
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    // Declared before the interfaces so they are released before the apartment.
    let _apartment = Apartment(initialized.is_ok());
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
    let endpoints = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }?;
    let count = unsafe { endpoints.GetCount() }?;
    ensure!(count <= 256, "Too many Windows audio outputs");
    let mut apps = BTreeMap::<u32, (String, bool, BTreeSet<String>)>::new();
    let mut total_sessions = 0;
    for index in 0..count {
        let device = unsafe { endpoints.Item(index) }?;
        let pointer = unsafe { device.GetId() }?;
        let identifier = unsafe { pointer.to_string() };
        unsafe { CoTaskMemFree(Some(pointer.0.cast())) };
        let identifier = identifier?;
        let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }?;
        let sessions = unsafe { manager.GetSessionEnumerator() }?;
        let count = unsafe { sessions.GetCount() }?;
        ensure!(
            (0..=4096).contains(&count),
            "Invalid Windows audio session count"
        );
        total_sessions += count;
        ensure!(total_sessions <= 16384, "Too many Windows audio sessions");
        for index in 0..count {
            let control = unsafe { sessions.GetSession(index) }?;
            let identity: IAudioSessionControl2 = control.cast()?;
            if unsafe { identity.IsSystemSoundsSession() } == S_OK {
                continue;
            }
            let pid = unsafe { identity.GetProcessId() }?;
            if pid == 0 || pid > i32::MAX as u32 || excluded.contains(&pid) {
                continue;
            }
            let Some((_, name)) = processes.get(&pid) else {
                continue;
            };
            let state = unsafe { control.GetState() }?;
            if state == AudioSessionStateExpired {
                continue;
            }
            ensure!(
                state == AudioSessionStateActive || state == AudioSessionStateInactive,
                "Unknown Windows audio session state"
            );
            let app = apps
                .entry(pid)
                .or_insert_with(|| (name.clone(), false, BTreeSet::new()));
            app.1 |= state == AudioSessionStateActive;
            app.2.insert(format!("wasapi:{identifier}"));
        }
    }
    Ok(apps
        .into_iter()
        .map(|(pid, (name, active, devices))| {
            serde_json::json!({
                "object_id":pid, "pid":pid, "name":name, "display_name":name,
                "bundle_id":serde_json::Value::Null, "executable":serde_json::Value::Null,
                "running_output":active, "devices":devices, "is_maris":false
            })
        })
        .collect())
}

fn session_key(control: &IAudioSessionControl2) -> Result<String> {
    let ptr = unsafe { control.GetSessionInstanceIdentifier() }
        .context("Read Windows audio session instance ID")?;
    let value = unsafe { ptr.to_string() }.context("Decode Windows audio session instance ID");
    unsafe { CoTaskMemFree(Some(ptr.0.cast())) };
    value
}

fn reconcile(
    roots: &BTreeSet<u32>,
    originals: &mut BTreeMap<String, OriginalSession>,
    stop: &AtomicBool,
) -> Result<()> {
    ensure!(
        !stop.load(Ordering::Acquire),
        "Windows routing startup cancelled"
    );
    // Use one process snapshot for both capture and self-exclusion decisions.
    let (targets, excluded) =
        super::windows_scope::selection(roots, std::process::id(), &process_parents()?)?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .context("Create Windows audio endpoint enumerator")?;
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .context("Enumerate Windows render endpoints")?;
    let endpoints = unsafe { collection.GetCount() }.context("Count Windows render endpoints")?;
    for endpoint in 0..endpoints {
        let device =
            unsafe { collection.Item(endpoint) }.context("Open Windows render endpoint")?;
        let manager: IAudioSessionManager2 =
            unsafe { device.Activate(CLSCTX_ALL, None) }.context("Open Windows audio sessions")?;
        let sessions = unsafe { manager.GetSessionEnumerator() }
            .context("Enumerate Windows audio sessions")?;
        let count = unsafe { sessions.GetCount() }.context("Count Windows audio sessions")?;
        for index in 0..count {
            let control =
                unsafe { sessions.GetSession(index) }.context("Read Windows audio session")?;
            let control2: IAudioSessionControl2 = control
                .cast()
                .context("Read Windows audio session identity")?;
            if unsafe { control2.IsSystemSoundsSession() } == S_OK {
                continue;
            }
            let pid = match unsafe { control2.GetProcessId() } {
                Ok(pid) if pid != 0 && !excluded.contains(&pid) => pid,
                _ => continue,
            };
            if !roots.is_empty() && !targets.contains(&pid) {
                continue;
            }
            let key = session_key(&control2)?;
            if originals.contains_key(&key) {
                continue;
            }
            let volume: ISimpleAudioVolume = control
                .cast()
                .context("Open Windows session mute control")?;
            let muted = unsafe { volume.GetMute() }
                .context("Read Windows session mute state")?
                .as_bool();
            ensure!(!stop.load(Ordering::Acquire), "Windows routing cancelled");
            // Own restoration before the mutation: a failing COM call may have
            // partially taken effect. Sessions also restores on a startup timeout.
            originals.insert(
                key,
                OriginalSession {
                    volume: volume.clone(),
                    muted,
                },
            );
            if !muted {
                unsafe { volume.SetMute(true, std::ptr::null()) }
                    .context("Mute original Windows audio session")?;
            }
        }
    }
    Ok(())
}

fn restore(originals: &mut BTreeMap<String, OriginalSession>) {
    for (_, session) in std::mem::take(originals) {
        let _ = unsafe { session.volume.SetMute(session.muted, std::ptr::null()) };
    }
}
