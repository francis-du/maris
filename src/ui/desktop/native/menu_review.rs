//! Offline native construction and production event-dispatch evidence.
use super::*;

fn heartbeat(store: &Store) -> Result<()> {
    let mut runtime: Value = read_json(&store.directory.join("runtime.json"))?;
    runtime["updated_at_ms"] = json!(crate::analysis::now_ms());
    store.write_json("runtime.json", &runtime)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Clone)]
enum CaptureMode {
    None,
    #[cfg(target_os = "macos")]
    All,
    #[cfg(target_os = "macos")]
    One { state: String, appearance: String },
}
#[cfg(target_os = "macos")]
impl CaptureMode {
    fn wants(&self, state: &str, appearance: &str) -> bool {
        match self {
            Self::None => false,
            Self::All => {
                (state == "live" && matches!(appearance, "light" | "dark"))
                    || (matches!(
                        state,
                        "idle" | "stale" | "failed" | "stopping" | "restore-failed"
                    ) && appearance == "dark")
            }
            Self::One {
                state: requested_state,
                appearance: requested_appearance,
            } => requested_state == state && requested_appearance == appearance,
        }
    }
}

#[cfg(target_os = "macos")]
fn snapshot(view: &Indicator, locale: &str, state: &str, appearance: &str) -> Result<Value> {
    use crate::ui::desktop::menu_capture;
    let directory = std::path::Path::new(".maris-review/native-menu");
    let menu_path = directory.join(format!("{locale}-{state}-{appearance}.png"));
    let bar_path = directory.join(format!("{locale}-{state}-{appearance}-bar.png"));
    let header_path = directory.join(format!("{locale}-{state}-{appearance}-header.png"));
    let menu = menu_capture::capture_menu(
        &view._menu,
        &view.header.view,
        &view.header.button,
        &menu_path,
        appearance,
    )?;
    let bar = menu_capture::capture(&view.header.button, &bar_path, appearance)?;
    let header = menu_capture::capture(&view.header.view, &header_path, appearance)?;
    let image = menu_capture::image_metadata(&view.header.button)?;
    Ok(
        json!({"language":locale,"state":state,"appearance":appearance,
        "menu_path":menu_path,"bar_path":bar_path,"header_path":header_path,"menu":menu,"bar":bar,"header":header,"image":image}),
    )
}

#[cfg(target_os = "macos")]
fn state_review(view: &mut Indicator, locale: &str, capture: &CaptureMode) -> Result<Value> {
    let store = view.store.clone();
    let original: Value = read_json(&store.directory.join("runtime.json"))?;
    let mut result = Vec::new();
    for (name, active, expected) in [
        ("live", true, "Processing"),
        ("pending", true, "Pending audio update"),
        ("bypass", true, "Processing bypassed"),
        ("idle", false, "Standby"),
        ("stale", true, "Awaiting telemetry"),
        ("failed", false, "Audio unavailable"),
    ] {
        let mut runtime = original.clone();
        runtime["active"] = json!(active);
        runtime["updated_at_ms"] = json!(crate::analysis::now_ms());
        runtime["applied_music_revision"] =
            json!(crate::tuning::preferences::load(&store)?.revision);
        runtime["peak_dbfs"] = json!(-6.0);
        runtime["tonal_bypass"] = json!(name == "bypass");
        if name == "pending" {
            runtime["applied_music_revision"] = json!(0);
        }
        if name == "idle" || name == "failed" {
            runtime = json!({"active":false,"updated_at_ms":crate::analysis::now_ms()});
        }
        if name == "stale" {
            runtime["updated_at_ms"] = json!(crate::analysis::now_ms().saturating_sub(4000));
        }
        store.write_json("runtime.json", &runtime)?;
        store.write_json(
            "startup.json",
            &if name == "failed" {
                json!({"phase":"failed","error":"OFFLINE fixture startup failure"})
            } else {
                Value::Null
            },
        )?;
        view.last_refresh = Instant::now() - Duration::from_secs(1);
        view.last_visual = Instant::now() - Duration::from_secs(1);
        // Exercise the bounded production cadence, without resetting its cache.
        std::thread::sleep(Duration::from_millis(125));
        ensure!(!view.tick(false)?, "Review unexpectedly closed");
        let state: Value = read_json(&store.directory.join("desktop.json"))?;
        ensure!(
            state["mode"] == expected,
            "Native {name} state rendered {:?}, expected {expected}",
            state["mode"]
        );
        let metadata = crate::ui::desktop::menu_capture::image_metadata(&view.header.button)?;
        if matches!(name, "idle" | "stale" | "failed") {
            ensure!(
                metadata["accessibility_value"]
                    .as_str()
                    .is_some_and(|value| value.contains(t("Unavailable"))
                        && !value.contains("OFFLINE fixture output")),
                "Unavailable native state exposed an unverified current output"
            );
        }
        ensure!(
            metadata["accessibility_value"]
                .as_str()
                .is_some_and(|value| value.contains(t(expected))),
            "Native header did not expose the actual {name} state"
        );
        if matches!(locale, "en" | "zh-CN") {
            for appearance in ["light", "dark"] {
                if capture.wants(name, appearance) {
                    result.push(snapshot(view, locale, name, appearance)?);
                }
            }
        }
    }
    // Hold only this fixture's session lease. Quit must render the real stopping
    // branch, while already queued mutations must not change the saved settings.
    store.write_json("runtime.json", &original)?;
    heartbeat(&store)?;
    view.refresh_quick_controls()?;
    let library = std::fs::read(store.directory.join("listening.json"))?;
    let lease = store.session_lock()?;
    view._menu_action(
        MenuEvent {
            id: view.quit.id().clone(),
        },
        |_, _| bail!("Quit unexpectedly requested confirmation"),
    )?;
    ensure!(!view.tick(false)?, "Quit ignored a running fixture session");
    for id in [
        view.toggle.id().clone(),
        view.follow.id().clone(),
        view.compare.id().clone(),
        view.undo.id().clone(),
        view.presets[0].0.id().clone(),
    ] {
        heartbeat(&store)?;
        ensure!(
            view._menu_action(MenuEvent { id }, |_, _| Ok(None))
                .is_err_and(|error| error.to_string() == "Stopping audio"),
            "Queued native action bypassed the stopping guard"
        );
    }
    ensure!(
        std::fs::read(store.directory.join("listening.json"))? == library
            && !view.controller.has_pending(),
        "Stopping mutated listening settings"
    );
    let stop_command = std::fs::read(store.directory.join("control.json"))?;
    drop(lease);
    // Invalid JSON fails before the restore backend can inspect or change any
    // system route. This exercises the actual error branch without audio access.
    std::fs::write(store.directory.join("route.json"), b"{")?;
    for (name, text) in [
        ("stopping", t("Stopping audio")),
        ("restore-failed", t("Restore failed")),
    ] {
        if name == "restore-failed" {
            // The fixture engine has released its lease and stopped publishing
            // active telemetry; recovery must not enable expired audio controls.
            store.write_json(
                "runtime.json",
                &json!({"active":false,
                "updated_at_ms":crate::analysis::now_ms()}),
            )?;
            ensure!(
                !view.tick(false)? && !view.closing && view.resume.is_enabled(),
                "Restore failure did not leave recovery controls available"
            );
            ensure!(
                view.notice.text().contains(text),
                "Restore failure lost its diagnostic"
            );
            view.last_refresh = Instant::now() - Duration::from_secs(1);
            ensure!(
                !view.tick(false)?,
                "Recovery unexpectedly closed the review"
            );
            ensure!(
                !view.toggle.is_enabled()
                    && !view.compare.is_enabled()
                    && !view.output_menu.is_enabled()
                    && view.resume.is_enabled(),
                "Recovery exposed audio actions without a current session"
            );
        }
        let metadata = crate::ui::desktop::menu_capture::image_metadata(&view.header.button)?;
        ensure!(
            metadata["accessibility_value"]
                .as_str()
                .is_some_and(|value| value.contains(text)),
            "Native header lost {name} status"
        );
        if matches!(locale, "en" | "zh-CN") && capture.wants(name, "dark") {
            result.push(snapshot(view, locale, name, "dark")?);
        }
    }
    ensure!(
        std::fs::read(store.directory.join("control.json"))? == stop_command,
        "Recovery changed the queued stop command"
    );
    std::fs::remove_file(store.directory.join("route.json"))?;
    view.last_refresh = Instant::now() - Duration::from_secs(1);
    ensure!(
        !view.tick(false)? && view.recovery_error.is_none(),
        "Successful journal recovery did not release the failed-restore status"
    );
    Ok(
        json!({"states_checked":["live","pending","bypass","idle","stale","failed","stopping","restore-failed"],
            "closing_dispatch_rejected":true,"restore_failure_before_route_access":true,"captures":result}),
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_impl(locale_filter: Option<&str>, capture: CaptureMode) -> Result<Value> {
    #[cfg(not(target_os = "macos"))]
    let _ = capture;
    let _events = winit::event_loop::EventLoop::new()?;
    let mut locales = Vec::new();
    for code in crate::i18n::LANGUAGES {
        if locale_filter.is_some_and(|requested| requested != code) {
            continue;
        }
        let directory = tempfile::tempdir()?;
        let store = Store::at(directory.path());
        crate::i18n::configure(&store, Some(code))?;
        store.write_json("runtime.json", &json!({"active":true,"music_processing":true,
                "session_id":"offline-menu-fixture","output":"OFFLINE fixture output","profile_key":"OFFLINE fixture output",
                "sample_rate":48000,"device_identity":{"stable_id":"offline-fixture"},
                "device_binding_revision":0,"rebind_count":0,"applied_revision":0,"applied_music_revision":0,
                "system_backend":"coreaudio_process_tap","output_mode":"follow_system_default",
                "updated_at_ms":crate::analysis::now_ms()}))?;
        let mut view = Indicator::new(store.clone(), false)?;
        // The review never polls the menu event receiver or starts the application loop.
        heartbeat(&store)?;
        view.refresh_quick_controls()?;
        let normal_items = view._menu.items().len();
        locales.push(json!({"language":code,"normal_items":normal_items,
                "output":view.output_caption.text(),"listening":view.profile.text(),"eq":view.tone_caption.text(),
                "presets":view.presets.iter().map(|(item,_)|item.text()).collect::<Vec<_>>(),
                "languages":view.languages.iter().map(|(item,_)|item.text()).collect::<Vec<_>>(),
                "immediate_selection_apply":true }));
        // Dispatch a real native item ID through the production handler. Selection
        // must apply in this one event and must never invoke a second confirmation UI.
        let selected = view
            .presets
            .iter()
            .find(|(_, id)| id == "focus")
            .context("Missing focus menu item")?
            .0
            .id()
            .clone();
        heartbeat(&store)?;
        view._menu_action(MenuEvent { id: selected }, |_, _| {
            bail!("Immediate native selection unexpectedly requested confirmation")
        })?;
        ensure!(
            crate::tuning::preferences::load(&store)?.revision == 1
                && !view.controller.has_pending()
                && view._menu.items().len() == normal_items,
            "Native selection did not apply immediately or left pending controls"
        );
        for _ in 0..2 {
            heartbeat(&store)?;
            view._menu_action(
                MenuEvent {
                    id: view.compare.id().clone(),
                },
                |_, _| bail!("A/B unexpectedly requested a modal"),
            )?;
        }
        heartbeat(&store)?;
        view._menu_action(
            MenuEvent {
                id: view.undo.id().clone(),
            },
            |_, _| bail!("Undo unexpectedly requested a modal"),
        )?;
        let restored = crate::tuning::preferences::load(&store)?;
        ensure!(
            restored.revision == 4 && restored.effective("OFFLINE fixture output").bass_db == 0.0,
            "Native comparison or undo failed"
        );
        ensure!(
            !store.directory.join("control.json").exists(),
            "Review queued audio control"
        );

        heartbeat(&store)?;
        view._menu_action(
            MenuEvent {
                id: view.follow.id().clone(),
            },
            |_, _| bail!("Immediate output selection unexpectedly requested confirmation"),
        )?;
        let control: Value = read_json(&store.directory.join("control.json"))?;
        ensure!(
            control["action"] == "select_output"
                && control["output"].is_null()
                && !view.controller.has_pending(),
            "Native output selection did not queue an immediate follow-default request"
        );
        std::fs::remove_file(store.directory.join("control.json"))?;

        #[cfg(target_os = "macos")]
        {
            let states = state_review(&mut view, code, &capture)?;
            let record = locales.last_mut().context("Missing locale review")?;
            record["native_states"] = states;
        }
    }
    Ok(
        json!({"source":"isolated offline fixture","native_menu_construction":true,
            "native_event_dispatch":true,"immediate_selection_apply":true,"native_dialog_clicked":false,
            "audio_started":false,"hardware_validated":false,"locales":locales}),
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn run(capture: bool) -> Result<Value> {
    #[cfg(target_os = "macos")]
    {
        run_impl(
            None,
            if capture {
                CaptureMode::All
            } else {
                CaptureMode::None
            },
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = capture;
        run_impl(None, CaptureMode::None)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn run_capture_one(locale: &str, state: &str, appearance: &str) -> Result<Value> {
    ensure!(
        crate::i18n::LANGUAGES.contains(&locale),
        "Unsupported menu-capture locale"
    );
    ensure!(
        matches!(
            state,
            "live" | "idle" | "stale" | "failed" | "stopping" | "restore-failed"
        ),
        "Unsupported menu-capture state"
    );
    ensure!(
        matches!(appearance, "light" | "dark"),
        "Unsupported menu-capture appearance"
    );
    run_impl(
        Some(locale),
        CaptureMode::One {
            state: state.to_owned(),
            appearance: appearance.to_owned(),
        },
    )
}
