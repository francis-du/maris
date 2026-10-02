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
        Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE, S_OK},
        Media::Audio::{
            eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
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
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .context("Create Windows process snapshot")?;
    struct Snapshot(HANDLE);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
    let snapshot = Snapshot(snapshot);
    let mut parents = BTreeMap::<u32, u32>::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // windows 0.54 returns Result<()>, not BOOL. Do not turn enumeration
    // failures into an apparently valid empty process tree.
    unsafe { Process32FirstW(snapshot.0, &mut entry) }.context("Read first Windows process")?;
    loop {
        parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        ensure!(parents.len() <= 65_536, "Windows process list is too large");
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(error) => return Err(error).context("Continue Windows process snapshot"),
        }
    }
    Ok(parents)
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
