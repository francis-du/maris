//! Terminal capabilities apply to the complete rendered console, including overlays.
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Indexed,
    Basic,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub color_depth: ColorDepth,
    pub reduced_motion: bool,
}

impl Appearance {
    /// Missing environment hints preserve the existing true-color console.
    pub fn detect(term: Option<&str>, colorterm: Option<&str>, no_color: bool) -> ColorDepth {
        if no_color || term == Some("dumb") {
            ColorDepth::None
        } else if matches!(colorterm, Some("truecolor" | "24bit")) || term.is_none() {
            ColorDepth::TrueColor
        } else if term.is_some_and(|term| term.contains("256color")) {
            ColorDepth::Indexed
        } else {
            ColorDepth::Basic
        }
    }

    pub fn from_environment() -> Self {
        let term = std::env::var("TERM").ok();
        let colorterm = std::env::var("COLORTERM").ok();
        Self {
            color_depth: Self::detect(
                term.as_deref(),
                colorterm.as_deref(),
                std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            ),
            reduced_motion: std::env::var("MARIS_REDUCED_MOTION")
                .is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true")),
        }
    }

    pub fn apply(self, buffer: &mut Buffer) {
        if self.color_depth == ColorDepth::TrueColor {
            return;
        }
        for cell in &mut buffer.content {
            if self.color_depth == ColorDepth::None
                && matches!(cell.fg, Color::Rgb(r, g, b) if r.max(g).max(b) < 170)
            {
                // Retain quiet labels/grid and stronger values even without hue.
                cell.modifier.insert(Modifier::DIM);
            }
            cell.fg = self.color_depth.map(cell.fg);
            cell.bg = self.color_depth.map(cell.bg);
        }
    }
}

impl ColorDepth {
    pub fn map(self, color: Color) -> Color {
        if self == Self::None {
            return Color::Reset;
        }
        let Color::Rgb(r, g, b) = color else {
            return color;
        };
        match self {
            Self::Indexed => {
                let component = |value: u8| {
                    [0_u8, 95, 135, 175, 215, 255]
                        .into_iter()
                        .enumerate()
                        .min_by_key(|(_, candidate)| value.abs_diff(*candidate))
                        .map(|(index, value)| (index as u8, value))
                        .unwrap()
                };
                let (ri, rc) = component(r);
                let (gi, gc) = component(g);
                let (bi, bc) = component(b);
                let gray = ((u16::from(r) + u16::from(g) + u16::from(b)) / 3)
                    .saturating_sub(8)
                    .saturating_add(5)
                    / 10;
                let gray = gray.min(23) as u8;
                let gray_value = 8 + gray * 10;
                if distance([r, g, b], [gray_value; 3]) < distance([r, g, b], [rc, gc, bc]) {
                    Color::Indexed(232 + gray)
                } else {
                    Color::Indexed(16 + 36 * ri + 6 * gi + bi)
                }
            }
            Self::Basic => {
                BASIC
                    .into_iter()
                    .min_by_key(|(_, rgb)| distance([r, g, b], *rgb))
                    .unwrap()
                    .0
            }
            _ => color,
        }
    }
}

fn distance(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| u32::from(a.abs_diff(b)).pow(2))
        .sum()
}

const BASIC: [(Color, [u8; 3]); 16] = [
    (Color::Black, [0, 0, 0]),
    (Color::Red, [128, 0, 0]),
    (Color::Green, [0, 128, 0]),
    (Color::Yellow, [128, 128, 0]),
    (Color::Blue, [0, 0, 128]),
    (Color::Magenta, [128, 0, 128]),
    (Color::Cyan, [0, 128, 128]),
    (Color::Gray, [192, 192, 192]),
    (Color::DarkGray, [128, 128, 128]),
    (Color::LightRed, [255, 0, 0]),
    (Color::LightGreen, [0, 255, 0]),
    (Color::LightYellow, [255, 255, 0]),
    (Color::LightBlue, [0, 0, 255]),
    (Color::LightMagenta, [255, 0, 255]),
    (Color::LightCyan, [0, 255, 255]),
    (Color::White, [255, 255, 255]),
];
