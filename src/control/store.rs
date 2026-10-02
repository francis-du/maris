use crate::dsp::profile::Profile;
use anyhow::{bail, ensure, Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_JSON_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub profile: Profile,
    pub previous: Option<Profile>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            profile: Profile::default(),
            previous: None,
        }
    }
}

#[derive(Clone)]
pub struct Store {
    pub directory: PathBuf,
}
impl Store {
    pub fn discover() -> Result<Self> {
        let directory = match std::env::var_os("MARIS_STATE_DIR") {
            Some(path) => PathBuf::from(path),
            None => ProjectDirs::from("", "", "maris")
                .context("Cannot determine configuration directory; set MARIS_STATE_DIR")?
                .config_dir()
                .to_owned(),
        };
        Ok(Self { directory })
    }
    pub fn at(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
    fn lock(&self) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("profile.lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn read_unlocked(&self) -> Result<Snapshot> {
        let path = self.directory.join("profile.json");
        if !path.exists() {
            return Ok(Snapshot::default());
        }
        let mut value: Snapshot = read_json(&path)?;
        ensure!(
            value.schema_version == 1,
            "Unsupported state schema_version"
        );
        value.profile.normalize_legacy_builtin_preamp();
        if let Some(previous) = &mut value.previous {
            previous.normalize_legacy_builtin_preamp();
        }
        value.profile.validate()?;
        if let Some(previous) = &value.previous {
            previous.validate()?;
        }
        Ok(value)
    }
    pub fn load(&self) -> Result<Snapshot> {
        let _guard = self.lock()?;
        self.read_unlocked()
    }
    pub fn edit(
        &self,
        expected: Option<u64>,
        change: impl FnOnce(&mut Profile) -> Result<()>,
    ) -> Result<Snapshot> {
        let _guard = self.lock()?;
        let mut state = self.read_unlocked()?;
        if let Some(revision) = expected {
            ensure!(
                state.revision == revision,
                "Revision conflict: expected {revision}, current {}",
                state.revision
            );
        }
        let previous = state.profile.clone();
        change(&mut state.profile)?;
        state.profile.validate()?;
        if state.profile == previous {
            return Ok(state);
        }
        state.previous = Some(previous);
        state.revision = state.revision.checked_add(1).context("Revision overflow")?;
        self.write_json("profile.json", &state)?;
        Ok(state)
    }
    pub fn undo(&self, expected: Option<u64>) -> Result<Snapshot> {
        let _guard = self.lock()?;
        let mut state = self.read_unlocked()?;
        if let Some(revision) = expected {
            ensure!(state.revision == revision, "Revision conflict");
        }
        let previous = state.previous.take().context("Nothing to undo")?;
        state.previous = Some(std::mem::replace(&mut state.profile, previous));
        state.revision = state.revision.checked_add(1).context("Revision overflow")?;
        self.write_json("profile.json", &state)?;
        Ok(state)
    }
    pub fn session_lock(&self) -> Result<File> {
        fs::create_dir_all(&self.directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("session.lock"))?;
        if file.try_lock_exclusive().is_err() {
            bail!("Another Maris audio session is running; use that session's TUI or CLI controls");
        }
        Ok(file)
    }
    pub fn write_json(&self, name: &str, data: &impl Serialize) -> Result<()> {
        atomic_json(&self.directory.join(name), data)
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_JSON_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_JSON_BYTES,
        "JSON file exceeds 64 KiB"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn atomic_json(path: &Path, data: &impl Serialize) -> Result<()> {
    write_atomic(path, data, true)
}
pub fn atomic_json_new(path: &Path, data: &impl Serialize) -> Result<()> {
    write_atomic(path, data, false)
}
fn write_atomic(path: &Path, data: &impl Serialize, replace: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, data)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(path).map_err(|e| e.error)?;
    } else {
        temporary.persist_noclobber(path).map_err(|e| e.error)?;
    }
    Ok(())
}
