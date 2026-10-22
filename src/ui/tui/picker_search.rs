//! Bounded, presentation-only picker filtering. Results retain frozen catalog indices.
//! No routing, saved-state writes, model calls or device discovery happen here.
use super::input::Overlay;
use crate::{audio::DeviceInfo, i18n::text as t, presets::PresetSummary, ui::theme::Palette};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{layout::Rect, style::Style, text::Span, widgets::Paragraph, Frame};

const QUERY_BYTES: usize = 256;

/// End-aligned text keeps the insertion point visible without splitting graphemes.
fn tail(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if Span::raw(value).width() <= width {
        return value.to_owned();
    }
    let span = Span::raw(value);
    let graphemes: Vec<_> = span.styled_graphemes(Style::default()).collect();
    let mut start = value.len();
    let mut used = 1; // Leading ellipsis.
    for grapheme in graphemes.iter().rev() {
        let cells = Span::raw(grapheme.symbol).width();
        if used + cells > width {
            break;
        }
        used += cells;
        start -= grapheme.symbol.len();
    }
    format!("…{}", &value[start..])
}

#[derive(Default, Debug)]
pub struct Search {
    query: String,
    folded: String,
    editing: bool,
    rejected: bool,
}
impl Search {
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn active(&self) -> bool {
        self.editing || !self.query.is_empty()
    }
    pub fn hint(&self, has_selection: bool) -> &'static str {
        match (self.active(), has_selection && !self.rejected) {
            (true, true) => "Enter confirms · Esc clears search",
            (true, false) => "Esc clears search",
            (false, true) => "Enter confirms · Esc cancels",
            (false, false) => "Esc cancels",
        }
    }
    pub fn clear(&mut self) {
        self.query.clear();
        self.folded.clear();
        self.editing = false;
        self.rejected = false;
    }
    pub fn warning(&self) -> Option<&'static str> {
        self.rejected
            .then_some("Search text is too long (256 bytes max)")
    }
    fn changed(&mut self) {
        // Normalize once per edit, not once per candidate on every rendered frame.
        self.folded = self.query.trim().to_lowercase();
        self.rejected = false;
    }
    /// Pasted text is data, never a stream of shortcuts or Enter confirmations.
    /// Accept it only in a visible, current picker after explicit search activation.
    pub fn handle_input(
        &mut self,
        event: &Event,
        overlay: Overlay,
        invalidated: bool,
        area: Rect,
    ) -> bool {
        if !matches!(overlay, Overlay::Output | Overlay::Preset)
            || invalidated
            || area.width < 30
            || area.height < 8
        {
            return false;
        }
        match event {
            Event::Key(key) => self.text_key(*key),
            Event::Paste(text) if self.editing => {
                // Reject the entire oversized paste; a truncated endpoint ID must
                // not turn into a different actionable search without a warning.
                if text.len() > QUERY_BYTES.saturating_sub(self.query.len()) {
                    self.rejected = true;
                } else {
                    let before = self.query.len();
                    self.query.extend(text.chars().filter_map(|ch| {
                        if ch.is_whitespace() {
                            Some(' ')
                        } else if ch.is_control() {
                            None
                        } else {
                            Some(ch)
                        }
                    }));
                    if self.query.len() != before {
                        self.changed();
                    }
                }
                true
            }
            _ => false,
        }
    }
    /// Consume only search edits. Enter is deliberately left to the guarded
    /// confirmation path; key releases/modifiers never become typed text.
    pub fn text_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if self.editing && key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('u')
        {
            self.query.clear();
            self.changed();
            return true;
        }
        if key.modifiers.bits() & !KeyModifiers::SHIFT.bits() != 0 {
            return false;
        }
        match key.code {
            KeyCode::Char('/') if !self.editing => {
                self.editing = true;
                true
            }
            KeyCode::Esc if self.editing || !self.query.is_empty() => {
                self.clear();
                true
            }
            KeyCode::Backspace if self.editing => {
                let length = Span::raw(self.query.as_str())
                    .styled_graphemes(Style::default())
                    .last()
                    .map_or(0, |grapheme| grapheme.symbol.len());
                self.query.truncate(self.query.len().saturating_sub(length));
                self.changed();
                true
            }
            KeyCode::Char(ch) if self.editing => {
                if !ch.is_control() {
                    if self.query.len() + ch.len_utf8() <= QUERY_BYTES {
                        self.query.push(ch);
                        self.changed();
                    } else {
                        self.rejected = true;
                    }
                }
                true
            }
            _ => false,
        }
    }
    pub fn matches(&self, label: &str) -> bool {
        if self.folded.is_empty() {
            return true;
        }
        let label = crate::ui::theme::clean(label).to_lowercase();
        self.folded
            .split_whitespace()
            .all(|term| label.contains(term))
    }
    pub fn outputs(&self, devices: &[DeviceInfo]) -> Vec<usize> {
        let mut found = Vec::new();
        if self.matches(&format!(
            "Follow system default Automatic {} {}",
            t("Follow system default"),
            t("Automatic")
        )) {
            found.push(0);
        }
        found.extend(
            devices
                .iter()
                .filter(|device| device.direction == "output")
                .enumerate()
                .filter_map(|(index, device)| {
                    self.matches(&format!("{} {}", device.name, device.id))
                        .then_some(index + 1)
                }),
        );
        found
    }
    pub fn presets(&self, presets: &[PresetSummary]) -> Vec<usize> {
        presets
            .iter()
            .enumerate()
            .filter_map(|(index, preset)| {
                let category = match preset.category {
                    "scene" => "Scene",
                    "listening" => "Listening preset",
                    _ => "Tone curve",
                };
                self.matches(&format!(
                    "{} {} {} {} {}",
                    preset.id,
                    preset.name,
                    crate::i18n::preset_name(&preset.id, &preset.name),
                    category,
                    t(category)
                ))
                .then_some(index)
            })
            .collect()
    }
    pub fn choices(
        &self,
        overlay: Overlay,
        devices: &[DeviceInfo],
        presets: &[PresetSummary],
    ) -> Vec<usize> {
        match overlay {
            Overlay::Output => self.outputs(devices),
            Overlay::Preset => self.presets(presets),
            _ => Vec::new(),
        }
    }
    /// Keep the same identity when it remains visible; an empty list has no
    /// actionable selection, not an implicit row-zero fallback.
    pub fn reconcile(choices: &[usize], selected: &mut usize) {
        if !choices.contains(selected) {
            if let Some(first) = choices.first() {
                *selected = *first;
            }
        }
    }
    pub fn browse(key: KeyCode, choices: &[usize], selected: &mut usize, page: usize) -> bool {
        let index = choices
            .iter()
            .position(|choice| choice == selected)
            .unwrap_or(0);
        let last = choices.len().saturating_sub(1);
        let next = match key {
            KeyCode::Up => index.saturating_sub(1),
            KeyCode::Down => index.saturating_add(1).min(last),
            KeyCode::Home => 0,
            KeyCode::End => last,
            KeyCode::PageUp => index.saturating_sub(page.max(1)),
            KeyCode::PageDown => index.saturating_add(page.max(1)).min(last),
            _ => return false,
        };
        if let Some(choice) = choices.get(next) {
            *selected = *choice;
        }
        true
    }
    pub fn draw(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        matches: usize,
        total: usize,
        palette: Palette,
    ) {
        if let Some(warning) = self.warning() {
            frame.render_widget(
                Paragraph::new(super::studio::fit(t(warning), area.width))
                    .style(Style::default().fg(palette.warning)),
                area,
            );
            return;
        }
        let label = if self.active() {
            let count = super::studio::fit(&format!("  {matches}/{total}"), area.width);
            let remaining = usize::from(area.width).saturating_sub(Span::raw(&count).width());
            let cursor = if self.editing && remaining > 0 {
                "▏"
            } else {
                ""
            };
            let prefix = super::studio::fit(
                &format!("/ {}: ", t("Search")),
                remaining.saturating_sub(Span::raw(cursor).width() + 1) as u16,
            );
            let budget =
                remaining.saturating_sub(Span::raw(&prefix).width() + Span::raw(cursor).width());
            let query = tail(&self.query, budget);
            let padding = budget.saturating_sub(Span::raw(&query).width());
            format!("{prefix}{query}{cursor}{}{count}", " ".repeat(padding))
        } else {
            format!("{}  {matches}/{total}", t("/ Search"))
        };
        frame.render_widget(
            Paragraph::new(super::studio::fit(&label, area.width)).style(Style::default().fg(
                if self.editing {
                    palette.accent
                } else {
                    palette.muted
                },
            )),
            area,
        );
    }
}
