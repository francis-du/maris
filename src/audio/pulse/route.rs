//! Temporary per-stream routing with a write-ahead recovery record.
//! Never changes server defaults, device volumes or persistent server configuration.
use super::server::{name, Input, InputId, Server, Snapshot, SocketId};
use crate::control::store::{self, Store};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};
const LEGACY_RECORD: &str = "pulse-route.json";
const RECORD_PREFIX: &str = "pulse-route-";

fn record_name(token: &str) -> Result<String> {
    name(token)?;
    Ok(format!("{RECORD_PREFIX}{token}.json"))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedRoute {
    pub input: InputId,
    pub original_sink: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    pub version: u32,
    pub token: String,
    pub socket: SocketId,
    pub sink: String,
    pub module: Option<u32>,
    pub original_default: String,
    pub routes: Vec<OwnedRoute>,
}
impl Journal {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.routes.len() <= 128,
            "Invalid audio recovery record"
        );
        name(&self.token)?;
        name(&self.sink)?;
        name(&self.original_default)?;
        ensure!(
            self.sink == format!("maris_{}", self.token),
            "Invalid recovery ownership"
        );
        for route in &self.routes {
            name(&route.original_sink)?;
        }
        Ok(())
    }
}

struct Watcher {
    child: Child,
    input: Option<ChildStdin>,
}
impl Watcher {
    fn start(store: &Store, token: &str) -> Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .process_group(0)
            .args(["__route-watch", token])
            .env("MARIS_STATE_DIR", &store.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take();
        let mut output = child
            .stdout
            .take()
            .context("Missing recovery handshake pipe")?;
        let fd = output.as_raw_fd();
        // SAFETY: output owns this pipe descriptor until the handshake finishes.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Cannot initialize route recovery handshake");
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut ready = [0_u8];
        loop {
            if matches!(output.read(&mut ready), Ok(1)) && ready == *b"R" {
                break;
            }
            if child.try_wait()?.is_some() || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "Route recovery helper did not become ready; audio was not redirected"
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(Self { child, input })
    }
    fn disarm(&mut self) {
        if let Some(mut input) = self.input.take() {
            let _ = input.write_all(b"D");
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
// Dropping an armed watcher closes its stdin. The helper then restores the recorded routes.

pub(super) struct Guard {
    pub server: Server,
    pub journal: Journal,
    store: Store,
    watcher: Option<Watcher>,
    pids: BTreeMap<i32, String>,
    exemptions: Vec<InputId>,
    active: bool,
}
fn process_start(pid: i32) -> Result<String> {
    ensure!(
        pid > 0 && pid != std::process::id() as i32,
        "Invalid application PID"
    );
    let file =
        std::fs::File::open(format!("/proc/{pid}/stat")).context("Selected application exited")?;
    let mut value = String::new();
    file.take(8192).read_to_string(&mut value)?;
    value
        .rsplit_once(')')
        .and_then(|(_, s)| s.split_whitespace().nth(19))
        .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_owned)
        .context("Cannot identify the selected process lifetime")
}
fn scope(pids: &[i32]) -> Result<BTreeMap<i32, String>> {
    ensure!(
        pids.len() <= 64,
        "At most 64 application PIDs can be selected"
    );
    let mut result = BTreeMap::new();
    for pid in pids {
        ensure!(
            result.insert(*pid, process_start(*pid)?).is_none(),
            "Duplicate application PID"
        );
    }
    Ok(result)
}
impl Guard {
    pub fn prepare(store: &Store, server: Server, pids: &[i32]) -> Result<Self> {
        let pids = scope(pids)?;
        let before = server.snapshot()?;
        before.output(None)?;
        let token = format!(
            "{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let journal = Journal {
            version: 1,
            sink: format!("maris_{token}"),
            token,
            socket: server.socket.clone(),
            module: None,
            original_default: before.default,
            routes: Vec::new(),
        };
        journal.validate()?;
        let record = record_name(&journal.token)?;
        store.write_json(&record, &journal)?;
        let mut result = Self {
            server,
            journal,
            store: store.clone(),
            watcher: None,
            pids,
            exemptions: Vec::new(),
            active: true,
        };
        result.watcher = Some(Watcher::start(store, &result.journal.token)?);
        result.journal.module = Some(result.server.load_sink(&result.journal.sink)?);
        result.save()?;
        let after = result.server.snapshot()?;
        ensure!(
            after.default == result.journal.original_default,
            "Server policy changed the default on sink creation; the temporary route was removed"
        );
        ensure!(
            after.sinks.iter().any(|s| s.name == result.journal.sink
                && s.owner_module == result.journal.module
                && s.routing_safe()),
            "Private audio sink is unavailable or uses unsafe flat-volume behavior"
        );
        Ok(result)
    }
    fn save(&self) -> Result<()> {
        self.journal.validate()?;
        self.store
            .write_json(&record_name(&self.journal.token)?, &self.journal)
    }
    pub fn present(&self) -> bool {
        self.pids
            .iter()
            .all(|(pid, start)| process_start(*pid).is_ok_and(|now| &now == start))
    }
    fn wanted(&self, input: &Input) -> bool {
        !input.own()
            && (self.pids.is_empty()
                || input.pid().is_some_and(|pid| {
                    self.pids
                        .get(&pid)
                        .is_some_and(|start| process_start(pid).is_ok_and(|now| &now == start))
                }))
    }
    pub fn pids(&self) -> Vec<i32> {
        self.pids.keys().copied().collect()
    }
    pub fn change_scope(&mut self, pids: &[i32]) -> Result<()> {
        let previous = std::mem::replace(&mut self.pids, scope(pids)?);
        if let Err(error) = self.reconcile() {
            self.pids = previous;
            let _ = self.reconcile();
            return Err(error);
        }
        Ok(())
    }
    pub fn reconcile(&mut self) -> Result<()> {
        let snapshot = self.server.snapshot()?;
        self.reconcile_snapshot(&snapshot)
    }
    pub fn reconcile_snapshot(&mut self, snapshot: &Snapshot) -> Result<()> {
        let private = snapshot
            .sinks
            .iter()
            .find(|s| s.name == self.journal.sink)
            .context("Private audio sink disappeared")?;
        ensure!(
            private.owner_module == self.journal.module,
            "Private audio sink ownership changed"
        );
        // Do not overwrite a user's manual route change or an index reused by another stream.
        let mut retained = Vec::new();
        for route in &self.journal.routes {
            if let Some(input) = snapshot
                .inputs
                .iter()
                .find(|i| i.identity().as_ref() == Some(&route.input))
            {
                if input.sink != private.index {
                    self.exemptions.push(route.input.clone());
                } else if !self.wanted(input) {
                    if let Some(target) = restore_target(snapshot, &self.journal, route) {
                        self.server.move_input(input.index, target)?;
                    } else {
                        anyhow::bail!("No original or default output is available for restoring an application");
                    }
                } else {
                    retained.push(route.clone());
                }
            }
        }
        self.journal.routes = retained;
        self.exemptions.retain(|id| {
            snapshot
                .inputs
                .iter()
                .any(|i| i.identity().as_ref() == Some(id))
        });
        self.save()?;
        for input in &snapshot.inputs {
            let Some(identity) = input.identity() else {
                continue;
            };
            if !self.wanted(input)
                || input.sink == private.index
                || self.exemptions.contains(&identity)
                || self.journal.routes.iter().any(|r| r.input == identity)
            {
                continue;
            }
            let Some(original) = snapshot
                .sinks
                .iter()
                .find(|sink| sink.index == input.sink && sink.physical() && sink.routing_safe())
            else {
                continue;
            };
            ensure!(
                self.journal.routes.len() < 128,
                "Too many application streams for safe recovery"
            );
            self.journal.routes.push(OwnedRoute {
                input: identity,
                original_sink: original.name.clone(),
            });
            // Persist first: a timeout or parent crash after a move is still recoverable.
            self.save()?;
            self.server.move_input(input.index, &self.journal.sink)?;
        }
        Ok(())
    }
    pub fn restore(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        recover_record(
            &self.store,
            &record_name(&self.journal.token)?,
            Some(&self.journal.token),
        )?;
        self.active = false;
        if let Some(mut watcher) = self.watcher.take() {
            watcher.disarm();
        }
        Ok(())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn restore_target<'a>(
    snapshot: &'a Snapshot,
    journal: &Journal,
    route: &OwnedRoute,
) -> Option<&'a str> {
    snapshot
        .sinks
        .iter()
        .find(|sink| sink.name == route.original_sink && sink.physical())
        .or_else(|| {
            snapshot.sinks.iter().find(|sink| {
                sink.name == snapshot.default && sink.name != journal.sink && sink.physical()
            })
        })
        .map(|sink| sink.name.as_str())
}

pub(super) fn recover(store: &Store, token: Option<&str>) -> Result<()> {
    if let Some(token) = token {
        return recover_record(store, &record_name(token)?, Some(token));
    }
    if store.directory.join(LEGACY_RECORD).exists() {
        recover_record(store, LEGACY_RECORD, None)?;
    }
    if !store.directory.exists() {
        return Ok(());
    }
    let mut records = Vec::new();
    for entry in std::fs::read_dir(&store.directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(RECORD_PREFIX) && name.ends_with(".json") {
            ensure!(
                kind.is_file() && !kind.is_symlink(),
                "Linked recovery record rejected"
            );
            records.push(name.to_owned());
            ensure!(records.len() <= 64, "Too many audio recovery records");
        }
    }
    records.sort();
    for record in records {
        recover_record(store, &record, None)?;
    }
    Ok(())
}

fn recover_record(store: &Store, record: &str, token: Option<&str>) -> Result<()> {
    let path = store.directory.join(record);
    if !path.exists() {
        return Ok(());
    }
    let journal: Journal = store::read_json(&path)?;
    journal.validate()?;
    ensure!(
        record == LEGACY_RECORD || record == record_name(&journal.token)?,
        "Recovery record identity mismatch"
    );
    if token.is_some_and(|token| token != journal.token) {
        return Ok(());
    }
    let current = SocketId::read(&journal.socket.path);
    if current
        .as_ref()
        .is_ok_and(|socket| socket != &journal.socket)
        || !journal.socket.path.exists()
    {
        std::fs::remove_file(path)?;
        return Ok(());
    }
    let server = Server::at(journal.socket.clone())?;
    finish_recovery(store, record, &journal, &server)
}

fn finish_recovery(store: &Store, record: &str, journal: &Journal, server: &Server) -> Result<()> {
    journal.validate()?;
    let path = store.directory.join(record);
    let snapshot = server.snapshot()?;
    let Some(private) = snapshot.sinks.iter().find(|s| s.name == journal.sink) else {
        std::fs::remove_file(path)?;
        return Ok(());
    };
    let module = journal
        .module
        .or(private.owner_module)
        .context("Missing private module ownership")?;
    ensure!(
        private.owner_module == Some(module),
        "Private module ownership changed"
    );
    ensure!(
        server.verify_module(module, &journal.sink)?,
        "Owned audio module disappeared during recovery"
    );
    let mut restore_error = None;
    for route in &journal.routes {
        if let Some(input) = snapshot.inputs.iter().find(|input| {
            input.sink == private.index
                && input.identity().as_ref() == Some(&route.input)
                && !input.own()
        }) {
            match restore_target(&snapshot, journal, route) {
                Some(target) => {
                    if let Err(error) = server.move_input(input.index, target) {
                        restore_error = Some(error);
                    }
                }
                None => {
                    restore_error = Some(anyhow::anyhow!(
                        "No physical output is available for route recovery"
                    ))
                }
            }
        }
    }
    // Removing the owned sink also lets the server restore unrecorded late-arriving streams.
    server.unload_owned(module, &journal.sink)?;
    std::fs::remove_file(path)?;
    if let Some(error) = restore_error {
        return Err(error);
    }
    Ok(())
}

pub(crate) fn watch(store: &Store, token: &str) -> Result<()> {
    name(token)?;
    let record = record_name(token)?;
    let journal: Journal = store::read_json(&store.directory.join(&record))?;
    ensure!(
        journal.token == token,
        "Recovery token differs from the active route"
    );
    journal.validate()?;
    std::io::stdout().write_all(b"R")?;
    std::io::stdout().flush()?;
    let mut byte = [0_u8];
    loop {
        match std::io::stdin().read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte == *b"D" => return Ok(()),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    // The owner's session lease closes on exit, including abnormal termination.
    for _ in 0..100 {
        if let Ok(_lease) = store.session_lock() {
            return recover_record(store, &record, Some(token));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    anyhow::bail!("Route recovery waits for a live session; the recovery record was retained")
}

#[cfg(test)]
#[path = "../../../tests/unit/pulse_route.rs"]
mod tests;
