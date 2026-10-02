//! Dynamic music-linked palette for the console and compact monitor.
use ratatui::style::Color;
use serde_json::Value;

pub const BG: Color = Color::Rgb(7, 9, 13);
pub const PANEL: Color = Color::Rgb(12, 15, 21);
pub const EDGE: Color = Color::Rgb(35, 40, 52);
pub const TEXT: Color = Color::Rgb(232, 235, 242);
pub const MUTED: Color = Color::Rgb(119, 127, 145);
pub const ACCENT: Color = Color::Rgb(151, 91, 255);
pub const METER: Color = Color::Rgb(0, 230, 196);
pub const WARNING: Color = Color::Rgb(255, 187, 72);
pub const SELECTED: Color = Color::Rgb(38, 27, 67);
pub const DANGER: Color = Color::Rgb(255, 86, 111);
pub const DIM: Color = Color::Rgb(75, 82, 98);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub name: &'static str,
    pub source: &'static str,
    pub bg: Color,
    pub panel: Color,
    pub edge: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub secondary: Color,
    pub meter: Color,
    pub warning: Color,
    pub selected: Color,
}

const fn palette_named(
    name: &'static str,
    source: &'static str,
    accent: Color,
    secondary: Color,
    meter: Color,
    selected: Color,
) -> Palette {
    Palette {
        name,
        source,
        bg: BG,
        panel: PANEL,
        edge: EDGE,
        text: TEXT,
        muted: MUTED,
        accent,
        secondary,
        meter,
        warning: WARNING,
        selected,
    }
}

pub const VIOLET: Palette = palette_named(
    "Violet Pulse",
    "default",
    Color::Rgb(158, 92, 255),
    Color::Rgb(0, 224, 255),
    Color::Rgb(0, 236, 184),
    Color::Rgb(42, 25, 72),
);
pub const VIOLET_SIGNAL: Palette = palette_named(
    "Violet Pulse",
    "signal",
    Color::Rgb(158, 92, 255),
    Color::Rgb(0, 224, 255),
    Color::Rgb(0, 236, 184),
    Color::Rgb(42, 25, 72),
);
pub const NEON: Palette = palette_named(
    "Neon Drive",
    "signal",
    Color::Rgb(0, 220, 255),
    Color::Rgb(255, 72, 200),
    Color::Rgb(78, 255, 169),
    Color::Rgb(14, 47, 67),
);
pub const EMBER: Palette = palette_named(
    "Ember Stage",
    "signal",
    Color::Rgb(255, 95, 78),
    Color::Rgb(255, 188, 66),
    Color::Rgb(255, 110, 135),
    Color::Rgb(69, 27, 31),
);
pub const AURORA: Palette = palette_named(
    "Aurora Air",
    "signal",
    Color::Rgb(106, 122, 255),
    Color::Rgb(49, 238, 213),
    Color::Rgb(89, 208, 255),
    Color::Rgb(29, 34, 69),
);
pub const LIME: Palette = palette_named(
    "Lime Bass",
    "signal",
    Color::Rgb(168, 255, 69),
    Color::Rgb(180, 83, 255),
    Color::Rgb(65, 244, 174),
    Color::Rgb(39, 52, 24),
);
pub const ROSE: Palette = palette_named(
    "Rose Vocal",
    "signal",
    Color::Rgb(255, 89, 164),
    Color::Rgb(139, 109, 255),
    Color::Rgb(255, 190, 93),
    Color::Rgb(65, 25, 49),
);

fn semantic_palette(style: &str) -> Option<Palette> {
    let style = style.to_ascii_lowercase();
    if style.contains("elect") || style.contains("techno") || style.contains("dance") {
        Some(Palette {
            source: "semantic",
            ..NEON
        })
    } else if style.contains("rock") || style.contains("metal") || style.contains("punk") {
        Some(Palette {
            source: "semantic",
            ..EMBER
        })
    } else if style.contains("hip") || style.contains("rap") || style.contains("bass") {
        Some(Palette {
            source: "semantic",
            ..LIME
        })
    } else if style.contains("vocal") || style.contains("pop") || style.contains("r&b") {
        Some(Palette {
            source: "semantic",
            ..ROSE
        })
    } else if style.contains("ambient") || style.contains("classical") || style.contains("jazz") {
        Some(Palette {
            source: "semantic",
            ..AURORA
        })
    } else {
        None
    }
}

/// A semantic label needs a current, explicitly semantic and confident observation.
pub fn semantic_label(runtime: &Value, now_ms: u64) -> Option<&str> {
    let context = &runtime["music_context"];
    if runtime["active"] != true
        || context["mode"] != "semantic"
        || !context["confidence"]
            .as_f64()
            .is_some_and(|value| (0.5..=1.0).contains(&value))
        || context["updated_at_ms"]
            .as_u64()
            .is_none_or(|time| now_ms.checked_sub(time).is_none_or(|age| age > 10_000))
    {
        return None;
    }
    context["genre"]
        .as_array()?
        .iter()
        .filter(|item| {
            item["confidence"]
                .as_f64()
                .is_some_and(|value| (0.5..=1.0).contains(&value))
        })
        .max_by(|a, b| {
            a["confidence"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["confidence"].as_f64().unwrap_or(0.0))
        })
        .and_then(|item| item["label"].as_str())
        .filter(|label| {
            !label.is_empty() && label.len() <= 128 && !label.chars().any(char::is_control)
        })
}

pub fn palette(runtime: &Value) -> Palette {
    if runtime["active"] != true {
        return VIOLET;
    }
    if let Some(style) = semantic_label(runtime, crate::analysis::now_ms()) {
        if let Some(palette) = semantic_palette(style) {
            return palette;
        }
    }
    let analysis = &runtime["analysis"];
    let shares = analysis["band_energy_share"].as_array();
    let sum = |range: std::ops::Range<usize>| {
        shares
            .map(|values| {
                range
                    .filter_map(|index| values.get(index).and_then(Value::as_f64))
                    .sum::<f64>()
            })
            .unwrap_or(0.0)
    };
    let bass = sum(0..3);
    let highs = sum(7..10);
    let crest = analysis["crest_db"].as_f64();
    let correlation = analysis["stereo_correlation"].as_f64();
    let rms = analysis["rms_dbfs"].as_f64();
    let signal_evidence =
        shares.is_some() || crest.is_some() || correlation.is_some() || rms.is_some();
    if bass > 0.55 {
        LIME
    } else if highs > 0.16 {
        NEON
    } else if correlation.is_some_and(|value| value < 0.25) {
        ROSE
    } else if crest.is_some_and(|value| value > 12.0) {
        AURORA
    } else if rms.is_some_and(|value| value > -18.0) {
        EMBER
    } else if runtime["active"] == true && signal_evidence {
        VIOLET_SIGNAL
    } else {
        VIOLET
    }
}

const THEME_DEBOUNCE_SECONDS: f64 = 1.25;
const THEME_TRANSITION_SECONDS: f64 = 0.65;

#[derive(Clone, Copy, Debug)]
pub struct Tracker {
    current: Palette,
    pending: Palette,
    pending_for: f64,
    from: Palette,
    target: Palette,
    transition: f64,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            current: VIOLET,
            pending: VIOLET,
            pending_for: 0.0,
            from: VIOLET,
            target: VIOLET,
            transition: 1.0,
        }
    }
}

impl Tracker {
    pub fn update(&mut self, runtime: &Value, dt_seconds: f64) -> Palette {
        let desired = palette(runtime);
        let dt = if dt_seconds.is_finite() {
            dt_seconds.clamp(0.0, 0.25)
        } else {
            0.0
        };
        if desired.name != self.pending.name || desired.source != self.pending.source {
            self.pending = desired;
            self.pending_for = 0.0;
        } else {
            self.pending_for = (self.pending_for + dt).min(10.0);
        }
        if self.pending_for >= THEME_DEBOUNCE_SECONDS
            && (self.pending.name != self.target.name || self.pending.source != self.target.source)
        {
            self.from = self.current;
            self.target = self.pending;
            self.transition = 0.0;
            self.pending_for = 0.0;
        }
        if self.transition < 1.0 {
            self.transition =
                (self.transition + dt / THEME_TRANSITION_SECONDS.max(1e-6)).clamp(0.0, 1.0);
            let t = self.transition * self.transition * (3.0 - 2.0 * self.transition);
            self.current = blend_palette(self.from, self.target, t);
        } else {
            self.current = self.target;
        }
        self.current
    }
}

fn blend_palette(from: Palette, to: Palette, t: f64) -> Palette {
    Palette {
        name: if t < 0.5 { from.name } else { to.name },
        source: if t < 0.5 { from.source } else { to.source },
        bg: blend(from.bg, to.bg, t),
        panel: blend(from.panel, to.panel, t),
        edge: blend(from.edge, to.edge, t),
        text: blend(from.text, to.text, t),
        muted: blend(from.muted, to.muted, t),
        accent: blend(from.accent, to.accent, t),
        secondary: blend(from.secondary, to.secondary, t),
        meter: blend(from.meter, to.meter, t),
        warning: blend(from.warning, to.warning, t),
        selected: blend(from.selected, to.selected, t),
    }
}

pub fn blend(a: Color, b: Color, t: f64) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let t = t.clamp(0.0, 1.0);
            let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
            Color::Rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
        }
        _ if t < 0.5 => a,
        _ => b,
    }
}

pub fn clean(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).collect()
}
