//! Audio-responsive status mark. The GUI polls it; audio callbacks never render icons.
use serde_json::Value;

pub const PIXELS: usize = 64;
pub const POINTS: f64 = 18.0;
pub const FRAME_INTERVAL_MS: u64 = 125;

/// Inactive, stale and reduced-motion displays wake twice a second for status.
/// Only an explicitly visible, live HUD needs the finer animation cadence.
pub fn poll_interval_ms(runtime: &Value, now: u64, reduced_motion: bool, hud_visible: bool) -> u64 {
    if reduced_motion || !crate::ui::tui::studio::live(runtime, now) {
        500
    } else if hud_visible {
        33
    } else if runtime["tonal_bypass"] == true
        || runtime["peak_dbfs"]
            .as_f64()
            .is_none_or(|value| !value.is_finite())
    {
        500
    } else {
        FRAME_INTERVAL_MS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Live,
    Bypass,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub mode: Mode,
    pub level: u8,
}

impl Frame {
    pub fn read(runtime: &Value, now: u64, reduced_motion: bool) -> Self {
        let live = crate::ui::tui::studio::live(runtime, now);
        let mode = if super::monitor_state::expired_session(runtime)
            || (runtime["active"] == true && !live)
        {
            Mode::Stale
        } else if !live {
            Mode::Idle
        } else if runtime["tonal_bypass"] == true {
            Mode::Bypass
        } else {
            Mode::Live
        };
        let level = if mode == Mode::Live && !reduced_motion {
            runtime["peak_dbfs"]
                .as_f64()
                .filter(|value| value.is_finite())
                .map_or(0, |db| {
                    (((db + 60.0) / 60.0).clamp(0.0, 1.0) * 16.0).round() as u8
                })
        } else {
            0
        };
        Self { mode, level }
    }

    pub fn cache_index(self) -> usize {
        match self.mode {
            Mode::Live => usize::from(self.level.min(16)),
            Mode::Idle => 17,
            Mode::Bypass => 18,
            Mode::Stale => 19,
        }
    }
}

/// At most eight image changes per second; identical levels and static states do no work.
#[derive(Default)]
pub struct Animation {
    previous: Option<Frame>,
    updated_at_ms: Option<u64>,
}

impl Animation {
    pub fn update(&mut self, runtime: &Value, now: u64, reduced_motion: bool) -> Option<Frame> {
        let frame = Frame::read(runtime, now, reduced_motion);
        if self.previous == Some(frame)
            || self.updated_at_ms.is_some_and(|last| {
                now.checked_sub(last)
                    .is_some_and(|age| age < FRAME_INTERVAL_MS)
            })
        {
            return None;
        }
        self.previous = Some(frame);
        self.updated_at_ms = Some(now);
        Some(frame)
    }
}

pub fn frames() -> impl Iterator<Item = Frame> {
    (0..=16)
        .map(|level| Frame {
            mode: Mode::Live,
            level,
        })
        .chain([Mode::Idle, Mode::Bypass, Mode::Stale].map(|mode| Frame { mode, level: 0 }))
}

/// A rounded M with a measured valley and explicit state punctuation.
/// Black and transparent pixels form the macOS template; other trays use visible tint.
pub fn rgba(frame: Frame, template: bool) -> Vec<u8> {
    let valley = 10.5 - 3.0 * f64::from(frame.level.min(16)) / 16.0;
    let segments = [
        ([3.0, 14.0], [3.0, 4.0]),
        ([3.0, 4.0], [9.0, valley]),
        ([9.0, valley], [15.0, 4.0]),
        ([15.0, 4.0], [15.0, 14.0]),
    ];
    let rgb = if template {
        [0, 0, 0]
    } else {
        match frame.mode {
            Mode::Live => [29, 145, 112],
            Mode::Stale => [184, 117, 39],
            _ => [80, 113, 144],
        }
    };
    let strength = if template {
        match frame.mode {
            Mode::Idle => 0.64,
            Mode::Bypass => 0.78,
            _ => 1.0,
        }
    } else {
        1.0
    };
    let mut result = vec![0; PIXELS * PIXELS * 4];
    for y in 0..PIXELS {
        for x in 0..PIXELS {
            let mut coverage: f64 = 0.0;
            for sy in [0.25, 0.75] {
                for sx in [0.25, 0.75] {
                    let point = [
                        (x as f64 + sx) * POINTS / PIXELS as f64,
                        (y as f64 + sy) * POINTS / PIXELS as f64,
                    ];
                    let mark = segments.iter().any(|&(a, b)| distance(point, a, b) <= 0.82);
                    let punctuation = match frame.mode {
                        Mode::Idle => {
                            let radius = (point[0] - 9.0).hypot(point[1] - 16.1);
                            (0.48..=0.88).contains(&radius)
                        }
                        Mode::Live => (point[0] - 9.0).hypot(point[1] - 16.1) <= 0.82,
                        Mode::Bypass => distance(point, [7.4, 16.1], [10.6, 16.1]) <= 0.45,
                        Mode::Stale => {
                            distance(point, [9.0, 14.4], [9.0, 15.2]) <= 0.4
                                || (point[0] - 9.0).hypot(point[1] - 16.5) <= 0.45
                        }
                    };
                    if mark || punctuation {
                        coverage += strength / 4.0;
                    }
                }
            }
            let index = (y * PIXELS + x) * 4;
            result[index..index + 3].copy_from_slice(&rgb);
            result[index + 3] = (255.0 * coverage).round() as u8;
        }
    }
    result
}

fn distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let position = (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1])
        / (delta[0] * delta[0] + delta[1] * delta[1]))
        .clamp(0.0, 1.0);
    (point[0] - a[0] - position * delta[0]).hypot(point[1] - a[1] - position * delta[1])
}
