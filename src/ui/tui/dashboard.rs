//! Canonical control-row registry shared by Studio and the in-place EQ inspector.
//! Existing row IDs retain their validated keyboard/revision semantics.
use crate::{i18n::text as t, ui::tui::input::Workspace, ui::tui::view::Console};
use ratatui::{layout::Rect, Frame};

pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    if view.workspace == Workspace::Now {
        crate::ui::tui::studio::draw(frame, area, view);
    } else {
        crate::ui::tui::inspector::draw(frame, area, view);
    }
}

pub fn sound_rows(view: &Console<'_>) -> Vec<(String, String)> {
    let state = |enabled: bool| {
        if enabled {
            t("ON").to_owned()
        } else {
            t("OFF").to_owned()
        }
    };
    let percent = |enabled: bool, amount: f64| {
        if enabled {
            format!("{:.0}%", amount * 100.0)
        } else {
            t("OFF").to_owned()
        }
    };
    let mut rows = vec![
        (t("Bass").into(), format!("{:+.1} dB", view.music.bass_db)),
        (
            t("Bass Assist").into(),
            percent(
                view.music.bass_assist.enabled,
                view.music.bass_assist.amount,
            ),
        ),
        (
            t("Presence").into(),
            format!("{:+.1} dB", view.music.presence_db),
        ),
        (t("Air").into(), format!("{:+.1} dB", view.music.air_db)),
        (
            t("Softness").into(),
            format!("{:.0}%", view.music.softness * 100.0),
        ),
        (
            t("Intensity").into(),
            format!("{:.0}%", view.music.intensity * 100.0),
        ),
        (
            t("Dynamic EQ").into(),
            percent(view.music.adaptive.enabled, view.music.adaptive.strength),
        ),
        (
            t("Stereo width").into(),
            format!("{:.0}%", view.music.width * 100.0),
        ),
        (t("Balance").into(), format!("{:+.2}", view.music.balance)),
        (t("Compressor").into(), state(view.music.compressor.enabled)),
        (
            t("Preamp").into(),
            format!("{:+.1} dB", view.snapshot.profile.preamp_db),
        ),
        (t("EQ bypass").into(), state(view.snapshot.profile.bypass)),
        (
            t("Crossfeed").into(),
            format!("{:.0}%", view.snapshot.profile.crossfeed * 100.0),
        ),
        (
            t("EQ width").into(),
            format!("{:.0}%", view.snapshot.profile.stereo_width * 100.0),
        ),
        (t("Music processing").into(), state(view.music.enabled)),
        (t("Level match").into(), state(view.music.level_match)),
    ];
    rows.extend(view.snapshot.profile.bands.iter().map(|band| {
        (
            format!("EQ {:>5.0} Hz", band.frequency_hz),
            format!(
                "{:+.1} dB Q{:.1}",
                band.gain_db,
                band.q_at(view.sample_rate())
            ),
        )
    }));
    rows
}
