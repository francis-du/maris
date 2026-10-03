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
    thread_local! {
        static INPUT: std::cell::RefCell<TerminalInput> = std::cell::RefCell::default();
    }
    INPUT.with(|input| {
        let mut input = input.borrow_mut();
        let mut wait = Duration::from_millis(33);
        next_input(|| {
            let result = input.next(wait, &mut |timeout| {
                // The level-triggered Unix backend skips even buffered events for
                // a zero timeout. Keep burst draining bounded but actually poll.
                if event::poll(timeout.max(Duration::from_millis(1)))? {
                    event::read().map(Some)
                } else {
                    Ok(None)
                }
            });
            wait = Duration::ZERO;
            result
        })
    })
}

/// Crossterm can report a standalone Esc when a Unix read ends at that byte.
/// Rejoin only a complete SGR mouse report before its fragments reach shortcuts.
/// Ordinary Escape and non-mouse keys retain their order, with at most 30 ms delay.
#[derive(Default)]
struct TerminalInput {
    queued: std::collections::VecDeque<crossterm::event::Event>,
    discard_mouse_tail: bool,
    pending_mouse: Option<String>,
}
impl TerminalInput {
    fn next(
        &mut self,
        wait: Duration,
        read: &mut impl FnMut(Duration) -> std::io::Result<Option<crossterm::event::Event>>,
    ) -> std::io::Result<Option<crossterm::event::Event>> {
        use crossterm::event::Event;
        if let Some(event) = self.queued.pop_front() {
            return Ok(Some(event));
        }
        if self.discard_mouse_tail {
            // Drain a bounded piece of an incomplete/oversized report. Its numeric
            // tail must not select another page. A genuine non-report event survives.
            for index in 0..32 {
                let Some(event) = read(if index == 0 { wait } else { Duration::ZERO })? else {
                    return Ok(None);
                };
                if let Event::Key(key) = &event {
                    if key.kind == KeyEventKind::Press
                        && key.modifiers.bits() & !KeyModifiers::SHIFT.bits() == 0
                    {
                        if let KeyCode::Char(ch) = key.code {
                            if ch.is_ascii_digit() || ch == ';' {
                                continue;
                            }
                            if matches!(ch, 'M' | 'm') {
                                self.discard_mouse_tail = false;
                                return Ok(None);
                            }
                        }
                    }
                }
                self.discard_mouse_tail = false;
                return Ok(Some(event));
            }
            return Ok(None);
        }
        let (mut original, mut text) = if let Some(partial) = self.pending_mouse.take() {
            (std::collections::VecDeque::new(), partial)
        } else {
            let Some(event) = read(wait)? else {
                return Ok(None);
            };
            if !matches!(event, Event::Key(key) if key.code == KeyCode::Esc
                && key.kind == KeyEventKind::Press && key.modifiers.is_empty())
            {
                return Ok(Some(event));
            }
            (std::collections::VecDeque::from([event]), String::new())
        };
        let deadline = Instant::now() + Duration::from_millis(30);
        for _ in text.len()..32 {
            let next = match read(deadline.saturating_duration_since(Instant::now())) {
                Ok(event) => event,
                Err(error) => {
                    if text.starts_with("[<") {
                        self.discard_mouse_tail = true;
                    } else {
                        self.queued.extend(original);
                    }
                    return Err(error);
                }
            };
            let Some(next) = next else {
                // A short read is not a lost click. Preserve the bounded report
                // across polling calls instead of discarding a delayed press/release.
                if text.starts_with("[<") {
                    self.pending_mouse = Some(text);
                    return Ok(None);
                }
                self.queued.extend(original);
                return Ok(self.queued.pop_front());
            };
            let character = match &next {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press
                        && key.modifiers.bits() & !KeyModifiers::SHIFT.bits() == 0 =>
                {
                    match key.code {
                        KeyCode::Char(ch) if ch.is_ascii() => Some(ch),
                        _ => None,
                    }
                }
                _ => None,
            };
            original.push_back(next);
            let Some(ch) = character else {
                if text.starts_with("[<") {
                    // Preserve the interrupting event, then discard the unfinished
                    // report's coordinates instead of turning them into shortcuts.
                    self.discard_mouse_tail = true;
                    return Ok(original.pop_back());
                }
                self.queued.extend(original);
                return Ok(self.queued.pop_front());
            };
            text.push(ch);
            if text == "[" || text == "[<" {
                continue;
            }
            if !text.starts_with("[<") {
                self.queued.extend(original);
                return Ok(self.queued.pop_front());
            }
            if matches!(ch, 'M' | 'm') {
                return Ok(sgr_mouse(&text).map(Event::Mouse));
            }
            if !ch.is_ascii_digit() && ch != ';' {
                self.discard_mouse_tail = true;
                return Ok(None);
            }
        }
        // Bound malformed input without interpreting its prefix as keyboard commands.
        self.discard_mouse_tail = true;
        Ok(None)
    }
}

fn sgr_mouse(text: &str) -> Option<crossterm::event::MouseEvent> {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let released = text.ends_with('m');
    let mut fields = text
        .strip_prefix("[<")?
        .strip_suffix(['M', 'm'])?
        .split(';');
    let code = fields.next()?.parse::<u16>().ok()?;
    let column = fields.next()?.parse::<u16>().ok()?.checked_sub(1)?;
    let row = fields.next()?.parse::<u16>().ok()?.checked_sub(1)?;
    if fields.next().is_some() || code > 127 {
        return None;
    }
    let mut modifiers = KeyModifiers::NONE;
    for (mask, modifier) in [
        (4, KeyModifiers::SHIFT),
        (8, KeyModifiers::ALT),
        (16, KeyModifiers::CONTROL),
    ] {
        if code & mask != 0 {
            modifiers.insert(modifier);
        }
    }
    let kind = if code & 64 != 0 {
        if released || code & 32 != 0 {
            return None;
        }
        match code & 3 {
            0 => MouseEventKind::ScrollUp,
            1 => MouseEventKind::ScrollDown,
            2 => MouseEventKind::ScrollLeft,
            _ => MouseEventKind::ScrollRight,
        }
    } else if code & 3 == 3 && code & 32 != 0 && !released {
        MouseEventKind::Moved
    } else {
        let button = match code & 3 {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => return None,
        };
        if released {
            MouseEventKind::Up(button)
        } else if code & 32 != 0 {
            MouseEventKind::Drag(button)
        } else {
            MouseEventKind::Down(button)
        }
    };
    Some(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/terminal_fragments.rs"]
mod fragment_tests;

pub const DASHBOARD_SOUND_ROWS: usize = 10;
pub const SOUND_ROWS: usize = 28;

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
    if row < 10 || row == 14 || row == 15 || row == 26 || row == 27 {
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
    ensure_displayed_output_at(store, displayed, crate::analysis::now_ms())
}

pub(crate) fn ensure_displayed_output_at(
    store: &crate::control::store::Store,
    displayed: &Value,
    now: u64,
) -> Result<()> {
    let current = crate::audio::runtime_status_at(store, now);
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
