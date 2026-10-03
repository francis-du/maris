//! Configuration edits are local drafts, never live mixer gestures.
//! Browse, draft and commit have separate controls and retain the existing transaction domains.
use crate::{
    audio,
    control::store::{Snapshot, Store},
    devices::capability::{self as device_profile, Capability},
    dsp::music::MusicProfile,
    dsp::profile::Profile,
    tuning::preferences as listening,
    ui::tui::input::{self as control_panel, PickerGuard, UndoTarget, EQ_ROW_START},
};
use anyhow::{ensure, Context as _, Result};
use crossterm::event::KeyCode;
use serde_json::Value;

mod input;
mod view;
pub use input::PointerLatch;
pub use view::{browser_rows, draw, group_rects, layout, pointer_action, Areas};

/// Existing row IDs are stable. Sections separate tone, shared EQ and playback policy.
pub const ROW_ORDER: [usize; 28] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 26, 27, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 10, 12, 13, 14,
    15, 11,
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Output,
    Equalizer,
    Playback,
}
impl Group {
    pub const ALL: [Self; 3] = [Self::Output, Self::Equalizer, Self::Playback];
    pub fn of(row: usize) -> Self {
        match row {
            0..=9 | 26 | 27 => Self::Output,
            10 | 12 | 13 | 16..=25 => Self::Equalizer,
            _ => Self::Playback,
        }
    }
    pub fn rows(self) -> &'static [usize] {
        match self {
            Self::Output => &ROW_ORDER[..12],
            Self::Equalizer => &ROW_ORDER[12..25],
            Self::Playback => &ROW_ORDER[25..],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Output => "Device sound",
            Self::Equalizer => "Global EQ",
            Self::Playback => "Playback switches",
        }
    }
    pub fn key(self) -> KeyCode {
        KeyCode::Char(match self {
            Self::Output => '1',
            Self::Equalizer => '2',
            Self::Playback => '3',
        })
    }
}
pub fn is_switch(row: usize) -> bool {
    matches!(row, 9 | 11 | 14 | 15)
}
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub runtime: &'a Value,
    pub eq: &'a Snapshot,
    pub listening: &'a listening::Library,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Browse,
    Staged,
    Cancelled,
    Applied(Option<UndoTarget>),
}
#[derive(Clone, Debug)]
struct Draft {
    guard: PickerGuard,
    runtime: Value,
    key: String,
    rate: u32,
    domain: UndoTarget,
    group: Group,
    eq_revision: u64,
    music_revision: u64,
    capability: Capability,
    eq_before: Profile,
    music_before: MusicProfile,
    eq: Profile,
    music: MusicProfile,
    invalidated: bool,
}
impl Draft {
    fn new(store: &Store, context: Context<'_>, row: usize) -> Result<Self> {
        ensure!(row < 28, "Unknown sound control");
        control_panel::ensure_displayed_output(store, context.runtime)?;
        let key = context.runtime["profile_key"]
            .as_str()
            .context("Missing current output identity")?
            .to_owned();
        let rate = context.runtime["sample_rate"]
            .as_u64()
            .filter(|rate| (44_100..=192_000).contains(rate))
            .context("Unsupported output sample rate")? as u32;
        let capability = device_profile::effective(store, &key)?;
        let mut runtime = context.runtime.clone();
        runtime["device_capability"] = serde_json::to_value(&capability)?;
        let music = context.listening.effective(&key).clone();
        let draft = Self {
            guard: PickerGuard::capture(
                &runtime,
                context.listening.revision,
                context.eq.revision,
                crate::analysis::now_ms(),
            ),
            runtime,
            key,
            rate,
            domain: control_panel::edit_target(row),
            group: Group::of(row),
            eq_revision: context.eq.revision,
            music_revision: context.listening.revision,
            capability,
            eq_before: context.eq.profile.clone(),
            music_before: music.clone(),
            eq: context.eq.profile.clone(),
            music,
            invalidated: false,
        };
        draft.validate_store(store)?;
        Ok(draft)
    }
    fn dirty(&self) -> bool {
        self.eq != self.eq_before || self.music != self.music_before
    }
    fn validate_context(&self, context: Context<'_>) -> Result<()> {
        ensure!(
            !self.invalidated,
            "Configuration changed; cancel this draft"
        );
        self.guard.validate(
            context.runtime,
            context.listening.revision,
            context.eq.revision,
            crate::analysis::now_ms(),
        )?;
        ensure!(
            context.eq.profile == self.eq_before
                && *context.listening.effective(&self.key) == self.music_before,
            "Configuration changed; cancel this draft"
        );
        Ok(())
    }
    fn validate_route(&self, store: &Store, eq_revision: u64, music_revision: u64) -> Result<()> {
        ensure!(
            !self.invalidated,
            "Configuration changed; cancel this draft"
        );
        // The frozen runtime identifies the draft's target; its original heartbeat
        // is not a live observation. Validate freshness on the current runtime below
        // while the guard still rejects a changed session, route or configuration.
        let capability = device_profile::effective(store, &self.key)?;
        ensure!(
            capability == self.capability,
            "Device capability changed after preview"
        );
        let mut runtime = audio::runtime_status(store);
        runtime["device_capability"] = serde_json::to_value(capability)?;
        self.guard.validate(
            &runtime,
            music_revision,
            eq_revision,
            crate::analysis::now_ms(),
        )
    }
    fn validate_store(&self, store: &Store) -> Result<()> {
        let eq = store.load()?;
        let library = listening::load(store)?;
        self.validate_route(store, eq.revision, library.revision)?;
        ensure!(
            eq.profile == self.eq_before && *library.effective(&self.key) == self.music_before,
            "Configuration changed; cancel this draft"
        );
        Ok(())
    }
    fn adjust(&mut self, row: usize, direction: f64, q: bool) -> Result<()> {
        ensure!(row < 28, "Unknown sound control");
        ensure!(
            !self.invalidated,
            "Configuration changed; cancel this draft"
        );
        ensure!(
            control_panel::edit_target(row) == self.domain && Group::of(row) == self.group,
            "Apply or cancel before editing another scope"
        );
        let mut candidate = self.clone();
        if q {
            adjust_q(
                &mut candidate.eq,
                row.checked_sub(EQ_ROW_START)
                    .context("Select a band above")?,
                direction,
                self.rate,
            )?;
        } else if row < 10 {
            let enabled = candidate.music.enabled;
            let reference = candidate.music.reference;
            candidate.music = crate::ui::tui::music::adjusted_profile(
                &candidate.music,
                &self.capability,
                row,
                direction,
            )?;
            // Constraints belong to the rendered profile, not unrelated stored
            // preferences. Editing width must not rewrite a saved bass preference.
            // Configuration is not a live audition. Playback policy has its own explicit controls.
            candidate.music.enabled = enabled;
            candidate.music.reference = reference;
        } else if row == 14 {
            candidate.music.enabled = direction > 0.0;
        } else if row == 15 {
            candidate.music.level_match = direction > 0.0;
        } else if row == 26 || row == 27 {
            candidate.music = crate::ui::tui::music::adjusted_profile(
                &candidate.music,
                &self.capability,
                if row == 26 { 10 } else { 11 },
                direction,
            )?;
        } else {
            adjust_eq(&mut candidate.eq, row, direction)?;
        }
        let mut normalized = candidate.eq.clone();
        normalized.name = self.eq_before.name.clone();
        if normalized == self.eq_before {
            candidate.eq = normalized;
        }
        candidate.eq.validate()?;
        candidate.music.validate()?;
        crate::dsp::Settings::compile(&candidate.eq, self.rate)?.with_music(
            &device_profile::apply_constraints(&candidate.music, &self.capability),
            self.rate,
        )?;
        *self = candidate;
        Ok(())
    }
    fn apply(&self, store: &Store) -> Result<Option<UndoTarget>> {
        self.validate_store(store)?;
        if !self.dirty() {
            return Ok(None);
        }
        crate::dsp::Settings::compile(&self.eq, self.rate)?.with_music(
            &device_profile::apply_constraints(&self.music, &self.capability),
            self.rate,
        )?;
        match self.domain {
            UndoTarget::Listening => {
                listening::edit_if_changed(
                    store,
                    Some(self.music_revision),
                    Some(&self.key),
                    |p| {
                        let eq = store.load()?;
                        self.validate_route(store, eq.revision, self.music_revision)?;
                        ensure!(
                            *p == self.music_before && eq.profile == self.eq_before,
                            "Configuration changed; cancel this draft"
                        );
                        *p = self.music.clone();
                        Ok(())
                    },
                )?;
            }
            UndoTarget::Profile => {
                store.edit(Some(self.eq_revision), |p| {
                    let library = listening::load(store)?;
                    self.validate_route(store, self.eq_revision, library.revision)?;
                    ensure!(
                        *p == self.eq_before && *library.effective(&self.key) == self.music_before,
                        "Configuration changed; cancel this draft"
                    );
                    *p = self.eq.clone();
                    Ok(())
                })?;
            }
        }
        Ok(Some(self.domain))
    }
}

#[derive(Default)]
pub struct Editor {
    draft: Option<Draft>,
    review_scroll: usize,
    generation: u64,
}
impl Editor {
    pub fn pending(&self) -> bool {
        self.draft.as_ref().is_some_and(Draft::dirty)
    }
    pub fn invalidated(&self) -> bool {
        self.draft.as_ref().is_some_and(|d| d.invalidated)
    }
    pub fn cancel(&mut self) {
        self.draft = None;
        self.review_scroll = 0;
        self.generation = self.generation.wrapping_add(1);
    }
    pub fn refresh(&mut self, context: Context<'_>) {
        if let Some(draft) = &mut self.draft {
            if draft.validate_context(context).is_err() {
                draft.invalidated = true;
            }
        }
    }
    pub fn permits(&self, key: KeyCode) -> bool {
        !self.pending()
            || matches!(
                key,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Enter
                    | KeyCode::Esc
                    | KeyCode::Char(
                        '1' | '2' | '3' | '+' | '-' | '[' | ']' | '?' | 'l' | 's' | ' '
                    )
            )
    }
    pub fn handle(
        &mut self,
        store: &Store,
        context: Context<'_>,
        selected: &mut usize,
        key: KeyCode,
    ) -> Result<Outcome> {
        if let Some(group) = Group::ALL.into_iter().find(|group| group.key() == key) {
            ensure!(
                self.draft.as_ref().is_none_or(|d| d.group == group),
                "Apply or cancel before editing another scope"
            );
            *selected = group.rows()[0];
            return Ok(Outcome::Browse);
        }
        match key {
            KeyCode::Up
            | KeyCode::Left
            | KeyCode::Down
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End => {
                let rows = Group::of(*selected).rows();
                let position = rows.iter().position(|row| row == selected).unwrap_or(0);
                let next = match key {
                    KeyCode::Home => 0,
                    KeyCode::End => rows.len() - 1,
                    KeyCode::Up | KeyCode::Left => position.saturating_sub(1),
                    _ => (position + 1).min(rows.len() - 1),
                };
                *selected = rows[next];
                Ok(Outcome::Browse)
            }
            KeyCode::Char(' ' | 'b') => Ok(Outcome::Browse),
            KeyCode::PageUp => {
                self.review_scroll = self.review_scroll.saturating_sub(3);
                Ok(Outcome::Browse)
            }
            KeyCode::PageDown => {
                self.review_scroll = (self.review_scroll + 3).min(64);
                Ok(Outcome::Browse)
            }
            KeyCode::Esc if self.draft.is_some() => {
                self.cancel();
                Ok(Outcome::Cancelled)
            }
            KeyCode::Enter => {
                let Some(draft) = &mut self.draft else {
                    return Ok(Outcome::Browse);
                };
                let result = match draft.apply(store) {
                    Ok(result) => result,
                    Err(error) => {
                        draft.invalidated = true;
                        return Err(error);
                    }
                };
                self.cancel();
                Ok(Outcome::Applied(result))
            }
            KeyCode::Char('+' | '-' | '[' | ']') => {
                let q = matches!(key, KeyCode::Char('[' | ']'));
                ensure!(!q || (16..26).contains(selected), "Select a band above");
                let direction = if matches!(key, KeyCode::Char('-' | '[')) {
                    -1.0
                } else {
                    1.0
                };
                let mut draft = match &mut self.draft {
                    Some(draft) => {
                        if let Err(error) = draft.validate_store(store) {
                            draft.invalidated = true;
                            return Err(error);
                        }
                        draft.clone()
                    }
                    None => Draft::new(store, context, *selected)?,
                };
                draft.adjust(*selected, direction, q)?;
                self.draft = draft.dirty().then_some(draft);
                self.generation = self.generation.wrapping_add(1);
                Ok(Outcome::Staged)
            }
            _ => Ok(Outcome::Pass),
        }
    }
}

/// The exact same bounded EQ adjustment is used by live controls and drafts.
pub fn adjust_eq(profile: &mut Profile, row: usize, direction: f64) -> Result<()> {
    ensure!(matches!(row, 10..=13 | 16..=25), "Unknown sound control");
    ensure!(
        direction == -1.0 || direction == 1.0,
        "Adjustment direction must be -1 or 1"
    );
    let before = profile.clone();
    match row {
        10 => profile.preamp_db = (profile.preamp_db + direction * 0.5).clamp(-30.0, 0.0),
        11 => profile.bypass = direction > 0.0,
        12 => profile.crossfeed = (profile.crossfeed + direction * 0.01).clamp(0.0, 0.3),
        13 => profile.stereo_width = (profile.stereo_width + direction * 0.05).clamp(0.0, 1.5),
        _ => {
            profile.bands[row - EQ_ROW_START].gain_db =
                (profile.bands[row - EQ_ROW_START].gain_db + direction * 0.5).clamp(-24.0, 24.0)
        }
    }
    if *profile != before {
        profile.name = "custom".into();
    }
    profile.validate()
}
pub fn adjust_q(profile: &mut Profile, band: usize, direction: f64, rate: u32) -> Result<()> {
    ensure!(band < 10, "Unknown sound control");
    ensure!(
        direction == -1.0 || direction == 1.0,
        "Adjustment direction must be -1 or 1"
    );
    let before = profile.clone();
    let band = &mut profile.bands[band];
    let current = band.q_at(rate);
    let target = (current + direction * 0.1).clamp(0.2, 5.0);
    if (target - current).abs() > 1e-9 {
        band.bandwidth_octaves = None;
        band.q = target;
    }
    if *profile != before {
        profile.name = "custom".into();
    }
    profile.validate()
}
