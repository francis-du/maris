//! Local PulseAudio protocol access, including PipeWire's pulse server.
//! Discovery and subprocess operations stay outside audio rendering.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct SocketId {
    pub path: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub changed: i64,
    pub changed_ns: i64,
}
impl SocketId {
    pub fn read(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Audio server socket must be absolute");
        let info =
            fs::symlink_metadata(path).context("No local PulseAudio/PipeWire pulse socket")?;
        ensure!(
            info.file_type().is_socket(),
            "Audio server endpoint is not a Unix socket"
        );
        // SAFETY: geteuid has no arguments or ownership requirements.
        ensure!(
            info.uid() == unsafe { libc::geteuid() },
            "Audio server socket belongs to another user"
        );
        Ok(Self {
            path: path.into(),
            device: info.dev(),
            inode: info.ino(),
            changed: info.ctime(),
            changed_ns: info.ctime_nsec(),
        })
    }
}

#[derive(Clone)]
pub(super) struct Server {
    pub socket: SocketId,
    pactl: PathBuf,
    pacat: PathBuf,
}
fn executable(name: &str) -> Result<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|path| path.is_absolute())
        .map(|path| path.join(name))
        .find(|path| fs::metadata(path).is_ok_and(|m| m.is_file() && m.mode() & 0o111 != 0))
        .with_context(|| {
            format!("Required audio tool '{name}' is missing; install pulseaudio-utils")
        })
}
impl Server {
    #[cfg(test)]
    pub fn fixture(socket: SocketId, executable: PathBuf) -> Self {
        Self {
            socket,
            pactl: executable.clone(),
            pacat: executable,
        }
    }
    pub fn discover() -> Result<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() })));
        // Never inherit PULSE_SERVER: raw audio must remain on a local Unix socket.
        Self::at(SocketId::read(&runtime.join("pulse/native"))?)
    }
    pub fn at(socket: SocketId) -> Result<Self> {
        ensure!(
            SocketId::read(&socket.path)? == socket,
            "Audio server restarted; saved routes are stale"
        );
        Ok(Self {
            socket,
            pactl: executable("pactl")?,
            pacat: executable("pacat")?,
        })
    }
    fn base(&self, program: &Path) -> Result<Command> {
        ensure!(
            SocketId::read(&self.socket.path)? == self.socket,
            "Audio server restarted"
        );
        let address = self
            .socket
            .path
            .to_str()
            .context("Audio socket path is not UTF-8")?;
        let mut command = Command::new(program);
        command.arg(format!("--server=unix:{address}"));
        command
            .env_remove("PULSE_SERVER")
            .env("LC_ALL", "C")
            .stdin(Stdio::null());
        Ok(command)
    }
    pub fn command(&self, args: &[&str]) -> Result<String> {
        let mut output = tempfile::tempfile()?;
        let error = tempfile::tempfile()?;
        let mut child = self
            .base(&self.pactl)?
            .args(args)
            .stdout(output.try_clone()?)
            .stderr(error.try_clone()?)
            .spawn()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = (|| -> Result<String> {
            let status = loop {
                ensure!(
                    output.metadata()?.len() <= 1_048_576 && error.metadata()?.len() <= 16_384,
                    "Audio server response exceeds the size limit"
                );
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                ensure!(Instant::now() < deadline, "Audio server command timed out");
                std::thread::sleep(Duration::from_millis(2));
            };
            ensure!(
                status.success(),
                "Audio server command '{}' failed ({status})",
                args.first().unwrap_or(&"unknown")
            );
            ensure!(
                output.metadata()?.len() <= 1_048_576,
                "Audio server response is too large"
            );
            output.seek(SeekFrom::Start(0))?;
            let mut text = String::new();
            output.take(1_048_577).read_to_string(&mut text)?;
            Ok(text)
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        let _ = child.wait();
        result
    }
    pub fn events(&self) -> Result<Events> {
        let mut child = self
            .base(&self.pactl)?
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let output = child.stdout.take().expect("requested stdout pipe");
        let fd = output.as_raw_fd();
        // SAFETY: output owns the descriptor for the lifetime of this subscription.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Cannot initialize audio graph subscription");
        }
        Ok(Events {
            child,
            output,
            pending: Vec::with_capacity(4096),
        })
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let info: Value = serde_json::from_str(&self.command(&["--format=json", "info"])?)?;
        let default = info["default_sink_name"]
            .as_str()
            .context("Audio server did not report its default output")?;
        let sinks: Vec<Sink> =
            serde_json::from_str(&self.command(&["--format=json", "list", "sinks"])?)?;
        let inputs: Vec<Input> =
            serde_json::from_str(&self.command(&["--format=json", "list", "sink-inputs"])?)?;
        ensure!(
            sinks.len() <= 256 && inputs.len() <= 256,
            "Audio graph exceeds supported size"
        );
        for sink in &sinks {
            name(&sink.name)?;
            name(&sink.monitor_source)?;
        }
        name(default)?;
        Ok(Snapshot {
            default: default.into(),
            sinks,
            inputs,
        })
    }
    pub fn stream_command(&self, capture: bool, device: &str, token: &str) -> Result<Command> {
        name(device)?;
        name(token)?;
        let mut command = self.base(&self.pacat)?;
        command.args([
            if capture { "--record" } else { "--playback" },
            "--raw",
            "--format=float32le",
            "--rate=48000",
            "--channels=2",
            "--channel-map=front-left,front-right",
            "--latency-msec=20",
            "--process-time-msec=10",
            "--client-name=Maris",
            "--property=application.id=audio.maris.app",
            "--property=media.role=production",
        ]);
        command
            .arg(format!("--device={device}"))
            .arg(format!("--stream-name={token}"));
        command.stderr(Stdio::null());
        Ok(command)
    }
    pub fn move_input(&self, input: u32, sink: &str) -> Result<()> {
        name(sink)?;
        self.command(&["move-sink-input", &input.to_string(), sink])?;
        Ok(())
    }
    pub fn load_sink(&self, sink: &str) -> Result<u32> {
        name(sink)?;
        ensure!(
            sink.starts_with("maris_"),
            "Private sink requires Maris ownership"
        );
        self.command(&[
            "load-module",
            "module-null-sink",
            &format!("sink_name={sink}"),
            "format=float32le",
            "rate=48000",
            "channels=2",
            "channel_map=front-left,front-right",
            "sink_properties=device.description=Maris",
        ])?
        .trim()
        .parse()
        .context("Invalid audio module ID")
    }
    pub fn verify_module(&self, module: u32, sink: &str) -> Result<bool> {
        name(sink)?;
        let modules: Vec<Value> =
            serde_json::from_str(&self.command(&["--format=json", "list", "modules"])?)?;
        let Some(item) = modules
            .iter()
            .find(|item| item["index"].as_u64() == Some(u64::from(module)))
        else {
            return Ok(false);
        };
        ensure!(
            item["name"] == "module-null-sink"
                && item["argument"].as_str().is_some_and(|s| s
                    .split_whitespace()
                    .any(|a| a == format!("sink_name={sink}"))),
            "Audio module ownership changed; refusing to unload it"
        );
        Ok(true)
    }
    pub fn unload_owned(&self, module: u32, sink: &str) -> Result<()> {
        if self.verify_module(module, sink)? {
            self.command(&["unload-module", &module.to_string()])?;
        }
        Ok(())
    }
}

pub(super) struct Events {
    child: Child,
    output: ChildStdout,
    pending: Vec<u8>,
}
impl Events {
    pub fn changed(&mut self) -> Result<bool> {
        ensure!(
            self.child.try_wait()?.is_none(),
            "Audio graph subscription ended"
        );
        let mut changed = false;
        let mut bytes = [0_u8; 4096];
        for _ in 0..4 {
            match self.output.read(&mut bytes) {
                Ok(0) => anyhow::bail!("Audio graph subscription disconnected"),
                Ok(count) => {
                    self.pending.extend_from_slice(&bytes[..count]);
                    while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                        let line = std::str::from_utf8(&self.pending[..end])?;
                        changed |= graph_event(line);
                        self.pending.drain(..=end);
                    }
                    ensure!(self.pending.len() <= 4096, "Oversized audio graph event");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(changed)
    }
}
impl Drop for Events {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub(super) fn graph_event(line: &str) -> bool {
    line.contains(" on sink #")
        || line.contains(" on sink-input #")
        || line.contains(" on server #")
}

pub(super) fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "Invalid audio endpoint name"
    );
    Ok(())
}
fn property(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    let value = map.get(key)?;
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
}
#[derive(Clone, Debug, Deserialize)]
pub(super) struct Sink {
    pub index: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "monitor_source_name")]
    pub monitor_source: String,
    pub owner_module: Option<u32>,
    #[serde(default)]
    pub flags: Option<Value>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}
impl Sink {
    pub fn physical(&self) -> bool {
        !self.name.starts_with("maris_")
            && property(&self.properties, "device.class").as_deref() != Some("abstract")
            && property(&self.properties, "device.api")
                .is_some_and(|api| matches!(api.as_str(), "alsa" | "bluez5" | "bluez"))
    }
    pub fn routing_safe(&self) -> bool {
        // Flat volumes can alter hardware gain when a client moves or connects.
        match self.flags.as_ref() {
            Some(Value::String(flags)) => !flags.contains("FLAT_VOLUME"),
            Some(Value::Array(flags)) => flags
                .iter()
                .all(|flag| flag.as_str().is_some_and(|s| !s.contains("FLAT_VOLUME"))),
            _ => false,
        }
    }
    pub fn label(&self) -> &str {
        if self.description.is_empty() {
            &self.name
        } else {
            &self.description
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub(super) struct Input {
    pub index: u32,
    pub sink: u32,
    pub client: Option<u32>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}
impl Input {
    pub fn pid(&self) -> Option<i32> {
        property(&self.properties, "application.process.id")?
            .parse::<i32>()
            .ok()
            .filter(|pid| *pid > 0)
    }
    pub fn own(&self) -> bool {
        self.pid() == Some(std::process::id() as i32)
            || property(&self.properties, "application.id").as_deref() == Some("audio.maris.app")
    }
    pub fn stream_name(&self) -> Option<String> {
        property(&self.properties, "media.name")
    }
    pub fn identity(&self) -> Option<InputId> {
        Some(InputId {
            index: self.index,
            client: self.client?,
            pid: self.pid(),
            serial: property(&self.properties, "object.serial"),
            app: property(&self.properties, "application.id"),
        })
    }
    pub fn app(&self) -> Option<String> {
        property(&self.properties, "application.name")
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct InputId {
    pub index: u32,
    pub client: u32,
    pub pid: Option<i32>,
    pub serial: Option<String>,
    pub app: Option<String>,
}
pub(super) struct Snapshot {
    pub default: String,
    pub sinks: Vec<Sink>,
    pub inputs: Vec<Input>,
}
impl Snapshot {
    pub fn output(&self, selector: Option<&str>) -> Result<Sink> {
        let requested = selector.unwrap_or(&self.default);
        let exact_id = requested.starts_with("pulse:");
        let requested = requested.strip_prefix("pulse:").unwrap_or(requested);
        let matches: Vec<_> = self
            .sinks
            .iter()
            .filter(|s| s.name == requested || (!exact_id && s.label() == requested))
            .collect();
        ensure!(
            matches.len() == 1,
            "Output is missing or ambiguous; select a pulse: output from maris devices"
        );
        ensure!(
            matches[0].physical(),
            "System processing requires a physical ALSA or Bluetooth output"
        );
        ensure!(
            matches[0].routing_safe(),
            "Output flat-volume behavior is enabled or unknown; no routes or volumes were changed"
        );
        Ok(matches[0].clone())
    }
}
