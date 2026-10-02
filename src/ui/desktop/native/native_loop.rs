//! Window-independent native event delivery without GTK dependency resolution.
use super::*;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::run_on_demand::EventLoopExtRunOnDemand,
    window::WindowId,
};

struct Application {
    store: Store,
    quitting: Arc<AtomicBool>,
    auto_start: bool,
    indicator: Option<Indicator>,
    failure: Option<anyhow::Error>,
}

impl ApplicationHandler<MenuEvent> for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.indicator.is_none() {
            match Indicator::new(self.store.clone(), self.auto_start) {
                Ok(indicator) => self.indicator = Some(indicator),
                Err(error) => {
                    self.failure = Some(error);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {
        // The status menu and AppKit HUD own their native windows directly.
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: MenuEvent) {
        if let Some(indicator) = &mut self.indicator {
            if let Err(error) = indicator.menu_action(event, events::confirm) {
                indicator.controller.notice = Some(Notice::error(format!("{error:#}")));
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(indicator) = &mut self.indicator {
            match indicator.tick(self.quitting.load(Ordering::Relaxed)) {
                Ok(true) => event_loop.exit(),
                Ok(false) => event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + indicator.wait_interval(),
                )),
                Err(error) => {
                    self.failure = Some(error);
                    event_loop.exit();
                }
            }
        }
    }
}

fn create_loop() -> Result<EventLoop<MenuEvent>> {
    let mut builder = EventLoop::<MenuEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_default_menu(false)
            .with_activate_ignoring_other_apps(false);
    }
    Ok(builder.build()?)
}

pub(super) fn run(store: Store, quitting: Arc<AtomicBool>, auto_start: bool) -> Result<()> {
    let mut event_loop = create_loop()?;
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        // Native callbacks only wake the loop; mutations and dialogs stay on its thread.
        let _ = proxy.send_event(event);
    }));
    let mut application = Application {
        store,
        quitting,
        auto_start,
        indicator: None,
        failure: None,
    };
    let result = event_loop.run_app_on_demand(&mut application);
    drop(application.indicator.take());
    result?;
    match application.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Exercise the production event receiver and shutdown against temporary state only.
pub(super) fn review() -> Result<Value> {
    let directory = tempfile::tempdir()?;
    let store = Store::at(directory.path());
    crate::i18n::configure(&store, Some("en"))?;
    let mut event_loop = create_loop()?;
    let indicator = Indicator::new(store.clone(), false)?;
    let selected = indicator
        .languages
        .iter()
        .position(|(_, language)| *language == "zh-CN")
        .context("Missing review language action")?;
    let proxy = event_loop.create_proxy();
    #[cfg(target_os = "macos")]
    {
        use tray_icon::menu::ContextMenu;
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy.send_event(event);
        }));
        let menu = indicator.language_menu.ns_menu();
        ensure!(!menu.is_null(), "Missing native language menu");
        // SAFETY: our retained NSMenu on the GUI main thread. This invokes Muda's
        // actual native action, which must reach the production user_event receiver.
        unsafe {
            let menu = &*menu.cast::<objc2::runtime::AnyObject>();
            let _: () = objc2::msg_send![menu, performActionForItemAtIndex:selected as isize];
        }
    }
    #[cfg(target_os = "windows")]
    proxy.send_event(MenuEvent {
        id: indicator.languages[selected].0.id().clone(),
    })?;
    let quitting = Arc::new(AtomicBool::new(false));
    let signal = quitting.clone();
    let shutdown = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(750));
        signal.store(true, Ordering::Relaxed);
    });
    let mut application = Application {
        store: store.clone(),
        quitting,
        auto_start: false,
        indicator: Some(indicator),
        failure: None,
    };
    let began = Instant::now();
    let result = event_loop.run_app_on_demand(&mut application);
    drop(application.indicator.take());
    shutdown
        .join()
        .map_err(|_| anyhow::anyhow!("Review shutdown worker failed"))?;
    result?;
    if let Some(error) = application.failure {
        return Err(error);
    }
    ensure!(
        began.elapsed() < Duration::from_secs(5),
        "Event loop shutdown was not bounded"
    );
    let saved: Value = read_json(&store.directory.join("ui.json"))?;
    ensure!(
        saved["language"] == "zh-CN",
        "Native action did not reach the event receiver"
    );
    let desktop: Value = read_json(&store.directory.join("desktop.json"))?;
    ensure!(
        desktop["active"] == false,
        "Shutdown retained an active status item"
    );
    ensure!(
        !store.directory.join("startup.json").exists(),
        "Review attempted audio startup"
    );
    Ok(
        json!({"source":"isolated offline fixture", "production_event_loop":true,
        "native_os_menu_action":cfg!(target_os="macos"), "language_saved":"zh-CN",
        "shutdown_complete":true, "audio_started":false, "hardware_validated":false}),
    )
}
