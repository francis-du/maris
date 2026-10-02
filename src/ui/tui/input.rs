//! Single-page dashboard navigation with optional detail views.
//! Direct inspector actions keep Tab optional; navigation never writes audio state.

use crate::audio::DeviceInfo;
use anyhow::{ensure, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Event acquisition separated from rendering so click latency has a deterministic oracle.
pub fn next_input(
    mut read: impl FnMut() -> std::io::Result<Option<crossterm::event::Event>>,
) -> std::io::Result<Option<crossterm::event::Event>> {
    use crossterm::event::{Event, MouseEvent, MouseEventKind};
    // Motion has no product action. Bound draining so an unending stream cannot starve rendering.
    // Never reorder or discard presses, releases, drags, keys or resize boundaries.
    for _ in 0..128 {
        match read()? {
            Some(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                ..
            })) => continue,
            other => return Ok(other),
        }
    }
    Ok(None)
}

pub fn read_input() -> std::io::Result<Option<crossterm::event::Event>> {
    use crossterm::event;
    if !event::poll(Duration::from_millis(33))? {
        return Ok(None);
    }
    next_input(|| {
        if event::poll(Duration::ZERO)? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    })
}

pub const DASHBOARD_SOUND_ROWS: usize = 10;
pub const SOUND_ROWS: usize = 26;

pub fn compact_layout(width: u16, height: u16) -> bool {
    width < 90 || height < 26
}
pub const EQ_ROW_START: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndoTarget {
    Listening,
    Profile,
}

pub fn edit_target(row: usize) -> UndoTarget {
    if row < 10 || row == 14 || row == 15 {
        UndoTarget::Listening
    } else {
        UndoTarget::Profile
    }
}

/// Resolve explicit selectors without silently choosing a default or first duplicate name.
pub fn selected_device_index(
    devices: &[DeviceInfo],
    selector: Option<&str>,
) -> Result<Option<usize>> {
    let Some(selector) = selector else {
        return Ok(None);
    };
    let ids: Vec<_> = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| device.id == selector)
        .collect();
    ensure!(ids.len() <= 1, "Ambiguous output identity");
    if let Some((index, _)) = ids.first() {
        return Ok(Some(*index));
    }
    let mut matches = devices
        .iter()
        .enumerate()
        .filter(|(_, device)| device.name == selector);
    let first = matches.next().map(|(index, _)| index);
    ensure!(
        matches.next().is_none(),
        "Device name is ambiguous; select its stable device ID"
    );
    ensure!(first.is_some(), "Selected device is unavailable");
    Ok(first)
}

pub fn preserved_device_index(
    devices: &[DeviceInfo],
    previous: Option<&DeviceInfo>,
) -> Option<usize> {
    let previous = previous?;
    if previous.id.starts_with("uid:") || previous.id.starts_with("wasapi:") {
        // A disappearing stable endpoint must never be rebound to a same-named replacement.
        return devices.iter().position(|device| device.id == previous.id);
    }
    devices
        .iter()
        .position(|device| device.id == previous.id && device.name == previous.name)
}

/// Pickers retain the inventory the user actually saw. A refresh cannot shift Enter
/// onto a different endpoint, and an out-of-range selection never means system default.
pub fn confirmed_output<'a>(
    options: &'a [DeviceInfo],
    choice: usize,
    current: &[DeviceInfo],
) -> Result<Option<&'a DeviceInfo>> {
    if choice == 0 {
        return Ok(None);
    }
    let selected = options
        .get(choice - 1)
        .ok_or_else(|| anyhow::anyhow!("Output selection is unavailable; reopen the picker"))?;
    ensure!(
        selected.direction == "output",
        "Selected output is not a playback endpoint"
    );
    let mut matches = current.iter().filter(|device| {
        device.id == selected.id
            && device.direction == "output"
            && (selected.id.starts_with("uid:")
                || selected.id.starts_with("wasapi:")
                || device.name == selected.name)
    });
    ensure!(
        matches.next().is_some(),
        "Selected output disappeared; reopen the picker"
    );
    ensure!(matches.next().is_none(), "Ambiguous output identity");
    Ok(Some(selected))
}

/// Highlight the output actually reported by the engine, not an unacknowledged
/// request or the OS default while a following session is still rebinding.
pub fn runtime_output_index(devices: &[DeviceInfo], runtime: &Value) -> Option<usize> {
    if runtime["active"] != true {
        return None;
    }
    let selector = runtime["device_identity"]["stable_id"].as_str().map(|id| {
        let prefix = match runtime["device_identity"]["platform"].as_str() {
            Some("windows") => "wasapi:",
            Some("linux") if runtime["system_backend"] == "pulse_server" => "pulse:",
            _ => match runtime["system_backend"].as_str() {
                Some("wasapi_process_loopback") => "wasapi:",
                Some("pulse_server") => "pulse:",
                _ => "uid:",
            },
        };
        format!("{prefix}{id}")
    });
    let name = runtime["output"].as_str();
    let mut matches = devices.iter().enumerate().filter(|(_, d)| {
        d.direction == "output"
            && selector
                .as_ref()
                .map_or_else(|| name == Some(d.name.as_str()), |id| &d.id == id)
    });
    let index = matches.next()?.0;
    matches.next().is_none().then_some(index)
}

pub fn directional(devices: &[DeviceInfo], direction: &str) -> Vec<DeviceInfo> {
    devices
        .iter()
        .filter(|device| device.direction == direction)
        .cloned()
        .collect()
}

pub fn visible_applications(state: &Value) -> Vec<&Value> {
    state["applications"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|app| app["is_maris"] != true)
        .filter(|app| {
            app["pid"]
                .as_i64()
                .is_some_and(|pid| pid > 0 && pid <= i32::MAX as i64)
        })
        .collect()
}

/// Held keys may navigate or make bounded continuous adjustments, never confirm, toggle or undo.
pub fn repeat_key_allowed(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('[' | ']' | '+' | '-')
    )
}

/// Traditional terminals may report auto-repeat as Press, so discrete actions also
/// require a quiet interval. Continuous arrows/Q remain responsive.
#[derive(Default)]
pub struct PressGate {
    last: Option<(KeyCode, Instant)>,
    context_shield: Option<KeyCode>,
}
impl PressGate {
    /// A confirmation belongs to one modal only. Legacy terminals without release
    /// events must supply another deliberate input before Enter can target a new view.
    pub fn shield_confirmation(&mut self) {
        self.context_shield = Some(KeyCode::Enter);
    }
    pub fn pointer_input(&mut self) {
        self.context_shield = None;
        // A real pointer press is not a keyboard auto-repeat. Paired Apply still has its own latch.
        self.last = None;
    }
    pub fn accept(&mut self, key: KeyEvent, now: Instant) -> bool {
        if key.kind == KeyEventKind::Release {
            if self.context_shield == Some(key.code) {
                self.context_shield = None;
            }
            if self.last.is_some_and(|(code, _)| code == key.code) {
                self.last = None;
            }
            return false;
        }
        if key.modifiers.bits() & !KeyModifiers::SHIFT.bits() != 0 {
            return false;
        }
        if self.context_shield == Some(key.code) {
            return false;
        }
        self.context_shield = None;
        if repeat_key_allowed(key.code) {
            // A deliberate adjustment separates two confirmations. Do not compare
            // the next Enter with an older Enter from before this input.
            self.last = Some((key.code, now));
            return true;
        }
        let repeated = self.last.is_some_and(|(code, time)| {
            code == key.code && now.saturating_duration_since(time) < Duration::from_millis(300)
        });
        self.last = Some((key.code, now));
        key.kind != KeyEventKind::Repeat && !repeated
    }
}

fn configuration_identity(runtime: &Value) -> Value {
    serde_json::json!({
        "active": runtime["active"], "session_id": runtime["session_id"],
        "profile_key": runtime["profile_key"], "output": runtime["output"],
        "uid": runtime["device_identity"]["stable_id"], "sample_rate": runtime["sample_rate"],
        "binding_revision": runtime["device_binding_revision"], "rebind_count": runtime["rebind_count"],
        "output_mode": runtime["output_mode"], "capability": runtime["device_capability"]
    })
}

/// Freeze the configuration a picker was opened against; never rebase a pending confirmation.
#[derive(Clone, Debug)]
pub struct PickerGuard {
    identity: Value,
    music_revision: u64,
    eq_revision: u64,
    created_at_ms: u64,
}
impl PickerGuard {
    pub fn capture(runtime: &Value, music_revision: u64, eq_revision: u64, now: u64) -> Self {
        Self {
            identity: configuration_identity(runtime),
            music_revision,
            eq_revision,
            created_at_ms: now,
        }
    }
    pub fn validate(
        &self,
        runtime: &Value,
        music_revision: u64,
        eq_revision: u64,
        now: u64,
    ) -> Result<()> {
        ensure!(
            now.checked_sub(self.created_at_ms)
                .is_some_and(|age| age <= 60_000)
                && self.identity == configuration_identity(runtime)
                && self.music_revision == music_revision
                && self.eq_revision == eq_revision,
            "Configuration changed; reopen the picker"
        );
        ensure!(
            runtime["active"] != true || crate::ui::tui::studio::live(runtime, now),
            "No current audio telemetry"
        );
        Ok(())
    }
}

/// Reject a delayed key/click after output rebinding rather than writing an invisible old profile.
pub fn ensure_displayed_output(
    store: &crate::control::store::Store,
    displayed: &Value,
) -> Result<()> {
    let now = crate::analysis::now_ms();
    let current = crate::audio::runtime_status(store);
    ensure!(
        crate::ui::tui::studio::live(displayed, now) && crate::ui::tui::studio::live(&current, now),
        "No current audio telemetry"
    );
    for field in [
        "session_id",
        "profile_key",
        "output",
        "sample_rate",
        "device_binding_revision",
        "rebind_count",
    ] {
        ensure!(
            displayed[field] == current[field],
            "Active output changed after preview"
        );
    }
    ensure!(
        displayed["device_identity"]["stable_id"] == current["device_identity"]["stable_id"],
        "Active output changed after preview"
    );
    ensure!(
        current["profile_key"]
            .as_str()
            .is_some_and(|key| !key.is_empty()),
        "Missing current output identity"
    );
    Ok(())
}

pub fn changes_sound(workspace: Workspace, key: KeyCode) -> bool {
    matches!(key, KeyCode::Char('b' | 'u'))
        || (matches!(workspace, Workspace::Now | Workspace::Sound)
            && matches!(
                key,
                KeyCode::Left | KeyCode::Right | KeyCode::Char(' ' | '[' | ']')
            ))
}

/// Hidden workspaces cannot mutate audio from compact-monitor keyboard input.
pub fn compact_key_allowed(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Char('o' | 'p' | 'j' | '?' | 's' | 'q') | KeyCode::Esc | KeyCode::Enter
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Workspace {
    Now,
    Sound,
    Apps,
    Device,
    Intelligence,
    System,
}

impl Workspace {
    pub const ALL: [Self; 6] = [
        Self::Now,
        Self::Sound,
        Self::Apps,
        Self::Device,
        Self::Intelligence,
        Self::System,
    ];
    pub const DETAILS: [Self; 5] = [
        Self::Sound,
        Self::Apps,
        Self::Device,
        Self::Intelligence,
        Self::System,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Now => "Dashboard",
            Self::Sound => "Sound settings",
            Self::Apps => "Mixer Matrix",
            Self::Device => "Device Detail",
            Self::Intelligence => "Listening Assist",
            Self::System => "Diagnostics",
        }
    }

    pub fn action_label(self) -> &'static str {
        match self {
            Self::Now => "Studio",
            Self::Sound => "Settings",
            Self::Apps => "Apps",
            Self::Device => "Device",
            Self::Intelligence => "Assist",
            Self::System => "Health",
        }
    }

    pub fn shortcut(self) -> KeyCode {
        match self {
            Self::Now => KeyCode::Esc,
            Self::Sound => KeyCode::Char('e'),
            Self::Apps => KeyCode::Char('m'),
            Self::Device => KeyCode::Char('v'),
            Self::Intelligence => KeyCode::Char('i'),
            Self::System => KeyCode::Char('h'),
        }
    }

    pub fn direct(key: KeyCode) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.shortcut() == key)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|item| *item == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        match Self::DETAILS.iter().position(|detail| *detail == self) {
            Some(index) => Self::DETAILS[(index + 1) % Self::DETAILS.len()],
            None => Self::Sound,
        }
    }

    pub fn previous(self) -> Self {
        match Self::DETAILS.iter().position(|detail| *detail == self) {
            Some(index) => Self::DETAILS[(index + Self::DETAILS.len() - 1) % Self::DETAILS.len()],
            None => Self::System,
        }
    }

    pub fn from_digit(value: char) -> Option<Self> {
        match value {
            '1' => Some(Self::Now),
            '2' => Some(Self::Sound),
            '3' => Some(Self::Apps),
            '4' => Some(Self::Device),
            '5' => Some(Self::Intelligence),
            '6' => Some(Self::System),
            _ => None,
        }
    }
}

/// Independent view cursors prevent EQ entry from walking through listening controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CursorMemory {
    pub home: usize,
    pub eq: usize,
}

impl Default for CursorMemory {
    fn default() -> Self {
        Self {
            home: 0,
            eq: EQ_ROW_START,
        }
    }
}

impl CursorMemory {
    pub fn visit(&mut self, from: Workspace, to: Workspace, row: usize) -> usize {
        match from {
            Workspace::Now => self.home = row.min(DASHBOARD_SOUND_ROWS - 1),
            Workspace::Sound if row < DASHBOARD_SOUND_ROWS => self.home = row,
            Workspace::Sound => self.eq = row.min(SOUND_ROWS - 1),
            _ => {}
        }
        match to {
            Workspace::Now => self.home,
            Workspace::Sound => self.eq,
            _ => row,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Overlay {
    #[default]
    None,
    Help,
    Output,
    Preset,
    Proposal,
    StartSystem,
}
