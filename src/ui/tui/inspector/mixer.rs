//! Active mixer panel: input strips and output buses share visible selection,
//! but retain separate identities and capability-specific commands.
use super::*;
use crate::ui::tui::actions;

pub(super) fn commands(view: &Console<'_>) -> [(&'static str, Option<KeyCode>); 3] {
    let strips = actions::mixer_rows(view.runtime);
    let bus = view
        .app_row
        .checked_sub(strips.len())
        .and_then(|index| actions::mixer_bus_rows(view.runtime).get(index));
    let selected = strips.get(view.app_row).is_some() || bus.is_some();
    [
        ("− Volume", selected.then_some(KeyCode::Left)),
        ("Space Mute", selected.then_some(KeyCode::Char(' '))),
        if bus.is_some() {
            ("+ Volume", Some(KeyCode::Right))
        } else {
            ("X Solo", selected.then_some(KeyCode::Char('x')))
        },
    ]
}

fn measured<'a>(runtime: &'a Value, collection: &str, id: Option<&str>) -> Option<&'a Value> {
    let id = id.filter(|id| !id.is_empty())?;
    let mut matches = runtime["mixer"][collection]
        .as_array()?
        .iter()
        .filter(|item| item["id"].as_str() == Some(id));
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, view: &Console<'_>) {
    let p = view.palette;
    let r = view.runtime;
    let live = studio::live(r, crate::analysis::now_ms());
    let editable = live && r["mixer"]["restart_required"] == false;
    let strips = actions::mixer_rows(r);
    let buses = actions::mixer_bus_rows(r);
    heading(frame, row(area, 0), "Mixer controls", p);
    let status = if !live {
        "Awaiting telemetry"
    } else if !editable {
        "Mixer assignments changed; stop and restart before adjusting channels"
    } else if r["mixer_control"]["revision"].as_u64().is_some()
        && r["mixer_control"]["revision"] == r["mixer"]["processing_revision"]
    {
        "Applied"
    } else {
        "Pending audio update"
    };
    write(frame, row(area, 1), t(status), Style::default().fg(p.muted));
    write(
        frame,
        row(area, 3),
        t("Inputs and output buses"),
        Style::default().fg(p.muted),
    );
    let list = app_rows(area);
    let count = strips.len() + buses.len();
    let start = app_offset(view.app_row, count, list.height);
    if count == 0 {
        write(
            frame,
            list,
            t("No mixer channels"),
            Style::default().fg(p.muted),
        );
    }
    for (index, item) in strips
        .iter()
        .chain(buses)
        .enumerate()
        .skip(start)
        .take(usize::from(list.height))
    {
        let bus = index >= strips.len();
        let rect = row(list, (index - start) as u16);
        if index == view.app_row {
            frame.render_widget(
                Block::default().style(Style::default().bg(p.selected)),
                rect,
            );
        }
        let parts = Layout::horizontal([
            Constraint::Length(5),
            Constraint::Min(1),
            Constraint::Length(9),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(9),
        ])
        .spacing(1)
        .split(rect);
        write(
            frame,
            parts[0],
            match (index == view.app_row, item["mute"] == true) {
                (true, true) => "▶[x]",
                (true, false) => "▶[ ]",
                (false, true) => " [x]",
                (false, false) => " [ ]",
            },
            Style::default().fg(p.accent),
        );
        let name = if bus {
            format!("{} · {}", t("Output"), text(&item["name"]))
        } else {
            text(&item["name"])
        };
        write(frame, parts[1], &name, Style::default().fg(p.text));
        write(
            frame,
            parts[2],
            &number(&item["gain_db"], " dB", true),
            Style::default().fg(p.text),
        );
        let balance_or_delay = if bus {
            item["delay_ms"]
                .as_f64()
                .filter(|value| value.is_finite())
                .map_or_else(
                    || t("Unavailable").to_owned(),
                    |value| format!("{value:.0}ms"),
                )
        } else {
            number(&item["pan"], "", true)
        };
        write(
            frame,
            parts[3],
            &balance_or_delay,
            Style::default().fg(p.muted),
        );
        write(
            frame,
            parts[4],
            if bus {
                "—"
            } else if item["solo"] == true {
                t("ON")
            } else {
                t("OFF")
            },
            Style::default().fg(p.secondary),
        );
        let evidence = measured(r, if bus { "buses" } else { "strips" }, item["id"].as_str());
        let peak = evidence
            .and_then(|measured| {
                if bus {
                    let left = measured["peak_left_dbfs"]
                        .as_f64()
                        .filter(|v| v.is_finite())?;
                    let right = measured["peak_right_dbfs"]
                        .as_f64()
                        .filter(|v| v.is_finite())?;
                    Some(left.max(right))
                } else {
                    measured["peak_dbfs"].as_f64().filter(|v| v.is_finite())
                }
            })
            .map_or(Value::Null, Value::from);
        write(
            frame,
            parts[5],
            &number(&peak, " dB", live),
            Style::default().fg(p.meter),
        );
    }
    let bus = view
        .app_row
        .checked_sub(strips.len())
        .and_then(|index| buses.get(index));
    if let Some(selected) = strips.get(view.app_row).or(bus) {
        let (label, value) = if bus.is_some() {
            ("Output", &selected["output_device"])
        } else {
            ("Source", &selected["source_device"])
        };
        write(
            frame,
            row(area, area.height.saturating_sub(4)),
            &format!("{}: {}", t(label), text(value)),
            Style::default().fg(p.muted),
        );
    }
    write(
        frame,
        row(area, area.height.saturating_sub(3)),
        t(if bus.is_some() {
            "←/→ volume · Space mute · Home/End select · U undo"
        } else {
            "←/→ volume · Space mute · X solo · [/] balance · U undo"
        }),
        Style::default().fg(p.text),
    );
    for (rect, (label, key)) in command_rects(area).into_iter().zip(commands(view)) {
        button(frame, rect, label, p, editable && key.is_some());
    }
}
