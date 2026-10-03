//! Native tray construction and event-loop dispatch.
use super::*;
use crate::{
    i18n::Notice,
    ui::desktop::controls::{self as desktop_controls, Controller, Summary},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIcon, TrayIconBuilder,
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod menu_review;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn menu_review(capture: bool) -> Result<Value> {
    menu_review::run(capture)
}

struct Indicator {
    icon: TrayIcon,
    mark: super::native_mark::Mark,
    reduced_motion: bool,
    menu: Menu,
    status: MenuItem,
    profile: MenuItem,
    toggle: MenuItem,
    open: MenuItem,
    stop: MenuItem,
    resume: MenuItem,
    quit: MenuItem,
    presets: Vec<(MenuItem, String)>,
    preset_menu: Submenu,
    preset_headings: Vec<(MenuItem, &'static str)>,
    output_caption: MenuItem,
    tone_caption: MenuItem,
    output_menu: Submenu,
    outputs: Vec<(MenuItem, audio::DeviceInfo)>,
    follow: MenuItem,
    compare: MenuItem,
    undo: MenuItem,
    notice: MenuItem,
    controller: Controller,
    language_menu: Submenu,
    languages: Vec<(MenuItem, &'static str)>,
    last_inventory: Instant,
    last_result_ms: u64,
    language: crate::i18n::Watcher,
    store: Store,
    closing: bool,
    last_refresh: Instant,
    last_visual: Instant,
    cached_runtime: Value,
    application_status: &'static str,
    recovery_error: Option<String>,
    #[cfg(target_os = "macos")]
    mini: MenuItem,
    #[cfg(target_os = "macos")]
    hud: crate::ui::desktop::monitor::Hud,
    #[cfg(target_os = "macos")]
    header: super::menu_header::Header,
}

impl Indicator {
    fn new(store: Store, auto_start: bool) -> Result<Self> {
        let menu = Menu::new();
        let status = MenuItem::new(format!("Maris - {}", t("Standby")), false, None);
        let profile = MenuItem::new(format!("{}: {}", t("Profile"), t("Loading")), false, None);
        let open = MenuItem::new(t("Open tuning console"), true, None);
        let toggle = MenuItem::new(t("Bypass processing (keep safety gain)"), true, None);
        let stop = MenuItem::new(t("Stop processing"), true, None);
        let resume = MenuItem::new(t("Enable system audio tuning"), true, None);
        let quit = MenuItem::new(t("Quit Maris"), true, None);
        let output_caption = MenuItem::new(t("Output"), false, None);
        let tone_caption = MenuItem::new(t("Tone curve"), false, None);
        let output_menu = Submenu::new(t("Select output"), false);
        let follow = MenuItem::new(t("Follow system default"), false, None);
        output_menu.append(&follow)?;
        let compare = MenuItem::new(t("Compare reference / enhanced"), false, None);
        let undo = MenuItem::new(t("Undo listening change"), false, None);
        let notice = MenuItem::new(t("Output and preset selections apply immediately"), false, None);
        menu.append_items(&[
            &status,
            &output_caption,
            &profile,
            &tone_caption,
            &PredefinedMenuItem::separator(),
            &output_menu,
        ])?;
        #[cfg(target_os = "macos")]
        let mini = MenuItem::new(t("Compact monitor"), true, None);
        let preset_menu = Submenu::new(t("Listening presets"), false);
        let mut presets = Vec::new();
        let mut preset_headings = Vec::new();
        for (index, (heading, ids)) in desktop_controls::PRESET_GROUPS.iter().enumerate() {
            if index > 0 {
                preset_menu.append(&PredefinedMenuItem::separator())?;
            }
            let header = MenuItem::new(t(heading), false, None);
            preset_menu.append(&header)?;
            preset_headings.push((header, *heading));
            for id in *ids {
                let item = MenuItem::new(crate::i18n::preset_name(id, id), false, None);
                preset_menu.append(&item)?;
                presets.push((item, (*id).to_owned()));
            }
        }
        let language_menu = Submenu::new(t("Language"), true);
        let mut languages = Vec::new();
        for code in crate::i18n::LANGUAGES {
            let item = MenuItem::new(crate::i18n::language_name(code), true, None);
            language_menu.append(&item)?;
            languages.push((item, code));
        }
        menu.append_items(&[
            &preset_menu,
            &notice,
            &PredefinedMenuItem::separator(),
            &compare,
            &undo,
            &PredefinedMenuItem::separator(),
            &open,
        ])?;
        #[cfg(target_os = "macos")]
        menu.append(&mini)?;
        menu.append_items(&[
            &language_menu,
            &toggle,
            &PredefinedMenuItem::separator(),
            &resume,
            &stop,
            &quit,
        ])?;
        let mark = super::native_mark::Mark::new()?;
        let icon = mark
            .build(
                TrayIconBuilder::new()
                    .with_menu(Box::new(menu.clone()))
                    .with_tooltip(format!("Maris - {}", t("Standby"))),
            )
            .build()?;
        #[cfg(target_os = "macos")]
        let header = super::menu_header::Header::new(&icon, &menu)?;
        store.write_json("desktop.json", &json!({"active":true,"pid":std::process::id(),"updated_at_ms":crate::analysis::now_ms()}))?;
        if auto_start {
            start_system(store.clone());
        }
        Ok(Self {
            icon,
            mark,
            reduced_motion: reduce_motion(),
            menu,
            status,
            profile,
            toggle,
            open,
            stop,
            resume,
            quit,
            presets,
            preset_menu,
            preset_headings,
            output_caption,
            tone_caption,
            output_menu,
            outputs: Vec::new(),
            follow,
            compare,
            undo,
            notice,
            controller: Controller::default(),
            language_menu,
            languages,
            last_inventory: Instant::now() - Duration::from_secs(3),
            last_result_ms: 0,
            language: crate::i18n::Watcher::new(&store),
            store,
            closing: false,
            last_refresh: Instant::now() - Duration::from_secs(1),
            last_visual: Instant::now() - Duration::from_secs(1),
            cached_runtime: json!({"active":false}),
            application_status: "Apply state unknown",
            recovery_error: None,
            #[cfg(target_os = "macos")]
            mini,
            #[cfg(target_os = "macos")]
            hud: crate::ui::desktop::monitor::Hud::new(auto_start)?,
            #[cfg(target_os = "macos")]
            header,
        })
    }
    fn refresh_quick_controls(&mut self) -> Result<()> {
        let runtime = audio::runtime_status(&self.store);
        let summary = Summary::read(&self.store, &runtime, crate::analysis::now_ms())?;
        self.application_status = summary.status;
        self.controller.observe(&summary);
        self.toggle
            .set_enabled(summary.current && runtime["music_processing"] == true);
        if self.last_inventory.elapsed() >= Duration::from_secs(2) {
            let devices = audio::console_devices()?;
            let outputs: Vec<_> = devices
                .into_iter()
                .filter(|d| d.direction == "output")
                .collect();
            let unchanged = outputs.len() == self.outputs.len()
                && outputs.iter().zip(&self.outputs).all(|(a, (_, b))| {
                    a.id == b.id && a.name == b.name && a.is_default == b.is_default
                });
            if !unchanged {
                for (item, _) in &self.outputs {
                    self.output_menu.remove(item)?;
                }
                self.outputs.clear();
                for device in outputs {
                    let item =
                        MenuItem::new(desktop_controls::menu_text(&device.name), false, None);
                    self.output_menu.append(&item)?;
                    self.outputs.push((item, device));
                }
            }
            self.last_inventory = Instant::now();
        }
        self.output_caption
            .set_text(desktop_controls::menu_text(&summary.output));
        self.profile
            .set_text(desktop_controls::menu_text(&summary.listening));
        self.tone_caption
            .set_text(desktop_controls::menu_text(&summary.eq));
        #[cfg(target_os = "macos")]
        self.header.update(
            t(summary.status),
            &desktop_controls::menu_text(&summary.output),
        );
        self.compare.set_text(format!(
            "{} · {}",
            t("Compare reference / enhanced"),
            summary.compare
        ));
        self.compare.set_enabled(summary.controls_enabled);
        self.undo.set_text(t("Undo listening change"));
        self.undo.set_enabled(summary.undo_enabled);
        self.output_menu.set_text(t("Select output"));
        self.output_menu.set_enabled(summary.output_enabled);
        self.follow.set_text(format!(
            "{}{}",
            if summary.current && runtime["output_mode"] == "follow_system_default" {
                "✓ "
            } else {
                ""
            },
            t("Follow system default")
        ));
        self.follow.set_enabled(summary.output_enabled);
        for (item, device) in &self.outputs {
            let (enabled, active) = desktop_controls::output_item_state(&summary, &runtime, device);
            item.set_text(desktop_controls::menu_text(&format!(
                "{}{}",
                if active { "✓ " } else { "" },
                device.name
            )));
            item.set_enabled(enabled);
        }
        self.preset_menu.set_text(t("Listening presets"));
        self.preset_menu.set_enabled(summary.controls_enabled);
        for (item, key) in &self.preset_headings {
            item.set_text(t(key));
        }
        for (item, id) in &self.presets {
            item.set_text(format!(
                "{}{}",
                if summary.preset_id == Some(id.as_str()) && summary.status == "Applied" {
                    "✓ "
                } else {
                    ""
                },
                crate::i18n::preset_name(id, id)
            ));
            item.set_enabled(summary.controls_enabled);
        }
        self.language_menu.set_text(t("Language"));
        for (item, code) in &self.languages {
            item.set_text(format!(
                "{}{}",
                if *code == crate::i18n::code() {
                    "✓ "
                } else {
                    ""
                },
                crate::i18n::language_name(code)
            ));
        }
        if let Some(time) = runtime["last_control_result"]["updated_at_ms"].as_u64() {
            if time > self.last_result_ms
                && runtime["last_control_result"]["session_id"] == runtime["session_id"]
            {
                self.last_result_ms = time;
                if runtime["last_control_result"]["ok"] == false {
                    self.controller.notice = Some(Notice::error(
                        runtime["last_control_result"]["error"]
                            .as_str()
                            .unwrap_or("Audio control failed."),
                    ));
                }
            }
        }
        let text = self.controller.notice.as_ref().map_or_else(
            || t("Output and preset selections apply immediately").to_owned(),
            Notice::render,
        );
        self.notice.set_text(desktop_controls::menu_text(&text));
        Ok(())
    }
    fn compact_window_id(&self) -> Option<isize> {
        #[cfg(target_os = "macos")]
        {
            Some(self.hud.window_number())
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    fn wait_interval(&self) -> Duration {
        #[cfg(target_os = "macos")]
        let visible = self.hud.is_visible();
        #[cfg(not(target_os = "macos"))]
        let visible = false;
        Duration::from_millis(super::status_icon::poll_interval_ms(
            &self.cached_runtime,
            crate::analysis::now_ms(),
            self.reduced_motion,
            visible,
        ))
    }
    fn show_status(&mut self, state: &str, output: &str) {
        self.status.set_text(format!("Maris · {state}"));
        #[cfg(target_os = "macos")]
        self.header.update(state, output);
        let _ = self
            .icon
            .set_tooltip(Some(format!("Maris · {state} · {output}")));
    }
    /// Dispatch exactly one native event. Telemetry refresh never repeats an action.
    /// The confirmer is the OS presentation boundary; the same controller commits in tests.
    fn menu_action(
        &mut self,
        event: MenuEvent,
        _confirm: impl FnOnce(&str, &[String]) -> Result<Option<bool>>,
    ) -> Result<()> {
        let passive = event.id == *self.open.id()
            || self
                .languages
                .iter()
                .any(|(item, _)| event.id == *item.id());
        #[cfg(target_os = "macos")]
        let passive = passive || event.id == *self.mini.id();
        if self.closing
            && !passive
            && event.id != *self.stop.id()
            && event.id != *self.quit.id()
        {
            bail!("Stopping audio");
        }
        #[cfg(target_os = "macos")]
        if event.id == *self.mini.id() {
            self.hud.toggle();
            return Ok(());
        }
        let result = (|| {
            if event.id == *self.open.id() {
                events::open_console(&self.store)?;
            } else if event.id == *self.compare.id() {
                self.controller.compare(&self.store)?;
            } else if event.id == *self.undo.id() {
                self.controller.undo(&self.store)?;
            } else if event.id == *self.follow.id() {
                self.controller.select_output(&self.store, None, &[])?;
                self.controller
                    .apply(&self.store, &audio::console_devices()?)?;
            } else if let Some((_, device)) =
                self.outputs.iter().find(|(item, _)| event.id == *item.id())
            {
                let inventory = audio::console_devices()?;
                self.controller
                    .select_output(&self.store, Some(&device.id), &inventory)?;
                self.controller.apply(&self.store, &inventory)?;
            } else if let Some((_, code)) = self
                .languages
                .iter()
                .find(|(item, _)| event.id == *item.id())
            {
                crate::i18n::save(&self.store, code)?;
            } else if event.id == *self.toggle.id() {
                self.controller.toggle_processing(&self.store)?;
            } else if event.id == *self.resume.id() {
                start_system(self.store.clone());
            } else if event.id == *self.stop.id() {
                control::request_stop(&self.store)?;
            } else if event.id == *self.quit.id() {
                control::request_stop(&self.store)?;
                self.closing = true;
            } else if let Some((_, name)) =
                self.presets.iter().find(|(item, _)| event.id == *item.id())
            {
                self.controller.select_preset(&self.store, name)?;
                self.controller.apply(&self.store, &[])?;
            }
            Ok(())
        })();
        self.last_refresh = Instant::now() - Duration::from_secs(1);
        result
    }
    fn tick(&mut self, signal: bool) -> Result<bool> {
        if signal && !self.closing {
            control::request_stop(&self.store)?;
            self.closing = true;
        }
        #[cfg(target_os = "linux")]
        for _ in 0..32 {
            let Ok(event) = MenuEvent::receiver().try_recv() else {
                break;
            };
            let action = self.menu_action(event, events::confirm);
            if let Err(error) = action {
                self.controller.notice = Some(Notice::error(format!("{error:#}")));
            }
            self.last_refresh = Instant::now() - Duration::from_secs(1);
        }
        if self.closing {
            self.toggle.set_enabled(false);
            self.output_menu.set_enabled(false);
            self.preset_menu.set_enabled(false);
            self.compare.set_enabled(false);
            self.undo.set_enabled(false);
            self.resume.set_enabled(false);
            self.show_status(t("Stopping audio"), &self.output_caption.text());
            if control::is_stopped(&self.store) {
                // A previous crash can leave a route journal even after the engine releases its lock.
                if let Err(error) = audio::restore(&self.store) {
                    #[cfg(target_os = "macos")]
                    {
                        let failure = format!(
                            "{}: {}",
                            t("Restore failed"),
                            crate::i18n::diagnostic(&error.to_string())
                        );
                        self.show_status(&failure, &self.output_caption.text());
                        self.recovery_error = Some(failure.clone());
                        self.controller.notice = Some(Notice::error(failure));
                        self.notice.set_text(desktop_controls::menu_text(
                            &self.controller.notice.as_ref().unwrap().render(),
                        ));
                        self.closing = false;
                        self.resume.set_enabled(true);
                        return Ok(false);
                    }
                    #[cfg(not(target_os = "macos"))]
                    let _ = error;
                }
                return Ok(true);
            }
        }
        let visual_interval = self.wait_interval().max(Duration::from_millis(80));
        if self.last_visual.elapsed() >= visual_interval {
            self.cached_runtime = audio::runtime_status(&self.store);
            self.last_visual = Instant::now();
        }
        self.mark.update(
            &self.icon,
            &self.cached_runtime,
            crate::analysis::now_ms(),
            self.reduced_motion,
        )?;
        #[cfg(target_os = "macos")]
        self.hud.tick(
            &self.cached_runtime,
            self.cached_runtime["tonal_bypass"] == true,
        );
        if self.last_refresh.elapsed() >= Duration::from_millis(500) && !self.closing {
            self.reduced_motion = reduce_motion();
            self.language.refresh(&self.store);
            if let Err(error) = self.refresh_quick_controls() {
                self.application_status = "Apply state unknown";
                self.toggle.set_enabled(false);
                self.controller.notice = Some(Notice::error(format!("{error:#}")));
                self.compare.set_enabled(false);
                self.undo.set_enabled(false);
                self.output_menu.set_enabled(false);
                self.preset_menu.set_enabled(false);
                self.notice.set_text(desktop_controls::menu_text(
                    &self.controller.notice.as_ref().unwrap().render(),
                ));
            }
            self.open.set_text(t("Open tuning console"));
            self.stop.set_text(t("Stop processing"));
            self.resume.set_text(t("Enable system audio tuning"));
            self.quit.set_text(t("Quit Maris"));
            let state = self.store.load()?;
            let runtime = audio::runtime_status(&self.store);
            let startup = read_json::<Value>(&self.store.directory.join("startup.json"))
                .unwrap_or(Value::Null);
            let mode = super::monitor_state::menu_mode(
                &runtime,
                &startup,
                self.application_status,
                control::is_stopped(&self.store),
            );
            // A failed restore remains visible until its journal is successfully
            // resolved; the next telemetry refresh must not hide the diagnostic.
            if !self.store.directory.join("route.json").exists() {
                self.recovery_error = None;
            }
            let display = self
                .recovery_error
                .clone()
                .unwrap_or_else(|| t(mode).to_owned());
            self.show_status(&display, &self.output_caption.text());
            if runtime["active"] != true && startup["phase"] == "failed" {
                self.profile.set_text(format!(
                    "{}: {}",
                    t("Error"),
                    crate::i18n::diagnostic(
                        startup["error"].as_str().unwrap_or("Audio startup failed")
                    )
                ));
            }
            self.toggle.set_text(t(if state.profile.bypass {
                "Enable processing"
            } else {
                "Bypass processing (keep safety gain)"
            }));
            #[cfg(target_os = "macos")]
            self.mini.set_text(t("Compact monitor"));
            self.store.write_json("desktop.json", &json!({"active":true,"pid":std::process::id(),"mode":mode,"compact_window_id":self.compact_window_id(),"updated_at_ms":crate::analysis::now_ms()}))?;
            self.last_refresh = Instant::now();
        }
        Ok(false)
    }
}
impl Drop for Indicator {
    fn drop(&mut self) {
        let _ = self.store.write_json(
            "desktop.json",
            &json!({"active":false,"pid":std::process::id()}),
        );
    }
}
fn start_system(store: Store) {
    if !control::is_stopped(&store) {
        return;
    }
    std::thread::spawn(move || {
        if let Err(error) = launch_audio(&store, &["system".into(), "--accept-routing".into()]) {
            let _ = store.write_json("startup.json", &json!({"phase":"failed","error":format!("{error:#}"),"updated_at_ms":crate::analysis::now_ms()}));
        }
    });
}
fn reduce_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        super::menu_header::reduced_motion()
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::ui::tui::studio::appearance::Appearance::from_environment().reduced_motion
    }
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native_loop;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(super) fn event_loop_review() -> Result<Value> {
    native_loop::review()
}
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn event_loop(store: Store, quitting: Arc<AtomicBool>, auto_start: bool) -> Result<()> {
    native_loop::run(store, quitting, auto_start)
}
#[cfg(target_os = "linux")]
pub fn event_loop(store: Store, quitting: Arc<AtomicBool>, auto_start: bool) -> Result<()> {
    ensure!(std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some(), "A desktop D-Bus session and StatusNotifier tray host are required; use --no-tray on headless Linux");
    let mut indicator = Indicator::new(store, auto_start)?;
    while !indicator.tick(quitting.load(Ordering::Relaxed))? {
        thread::sleep(indicator.wait_interval());
    }
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn event_loop(_store: Store, _quitting: Arc<AtomicBool>, _auto_start: bool) -> Result<()> {
    bail!("Unsupported desktop platform")
}
