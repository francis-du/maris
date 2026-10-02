//! A small non-activating macOS monitor. All AppKit work stays on the main thread.
use crate::{analysis::spectrum::Motion, i18n::text as t};
use anyhow::{ensure, Result};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
    MainThreadMarker,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use ratatui::style::Color;
use std::{marker::PhantomData, rc::Rc, time::Instant};

const WIDTH: f64 = 264.0;
const HEIGHT: f64 = 76.0;
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub(crate) struct Hud {
    panel: Retained<AnyObject>,
    status: Retained<AnyObject>,
    device: Retained<AnyObject>,
    bars: Vec<Retained<AnyObject>>,
    levels: Vec<Retained<AnyObject>>,
    motion: Motion,
    theme: crate::ui::theme::Tracker,
    last: Instant,
    _main_thread: PhantomData<Rc<()>>,
}
impl Hud {
    pub fn new(show: bool) -> Result<Self> {
        ensure!(
            MainThreadMarker::new().is_some(),
            "The compact monitor requires the main thread"
        );
        // SAFETY: documented AppKit selectors and ABI types, main thread, retained object ownership.
        unsafe {
            let allocated: Allocated<AnyObject> = msg_send![class!(NSPanel), alloc];
            let panel: Retained<AnyObject> = msg_send![allocated, initWithContentRect: rect(0.0,0.0,WIDTH,HEIGHT), styleMask: 131_usize, backing: 2_usize, defer: false];
            let _: () = msg_send![&panel, setReleasedWhenClosed: false];
            let _: () = msg_send![&panel, setFloatingPanel: true];
            let _: () = msg_send![&panel, setHidesOnDeactivate: false];
            let _: () = msg_send![&panel, setHasShadow: true];
            let _: () = msg_send![&panel, setLevel: 3_isize];
            let _: () = msg_send![&panel, setTitle: &*NSString::from_str("Maris")];
            let _: () = msg_send![&panel, setMovableByWindowBackground: true];
            let _: () = msg_send![&panel, setCollectionBehavior: 257_usize];
            let appearance: Option<Retained<AnyObject>> = msg_send![class!(NSAppearance), appearanceNamed: &*NSString::from_str("NSAppearanceNameDarkAqua")];
            let _: () = msg_send![&panel, setAppearance: appearance.as_deref()];
            let background = color(0.075, 0.078, 0.090);
            let _: () = msg_send![&panel, setBackgroundColor: &*background];
            let content: Retained<AnyObject> = msg_send![&panel, contentView];
            let device = label(&content, rect(10.0, 50.0, 244.0, 16.0), "", 11.0, false);
            let status = label(
                &content,
                rect(10.0, 34.0, 244.0, 14.0),
                t("Standby"),
                10.0,
                true,
            );
            let mut bars = Vec::with_capacity(24);
            for i in 0..24 {
                bars.push(bar(
                    &content,
                    rect(10.0 + i as f64 * 7.2, 8.0, 3.8, 1.0),
                    color(0.67, 0.72, 0.90),
                ));
            }
            let levels = vec![
                bar(
                    &content,
                    rect(190.0, 22.0, 1.0, 3.0),
                    color(0.56, 0.74, 0.68),
                ),
                bar(
                    &content,
                    rect(190.0, 14.0, 1.0, 3.0),
                    color(0.56, 0.74, 0.68),
                ),
            ];
            label(&content, rect(180.0, 20.0, 9.0, 9.0), "L", 8.0, true);
            label(&content, rect(180.0, 12.0, 9.0, 9.0), "R", 8.0, true);
            let screen: Option<Retained<AnyObject>> = msg_send![class!(NSScreen), mainScreen];
            if let Some(screen) = screen {
                let bounds: NSRect = msg_send![&screen, visibleFrame];
                let point = NSPoint::new(
                    bounds.origin.x + bounds.size.width - WIDTH - 18.0,
                    bounds.origin.y + bounds.size.height - 18.0,
                );
                let _: () = msg_send![&panel, setFrameTopLeftPoint: point];
            }
            if show {
                let _: () = msg_send![&panel, orderFrontRegardless];
            }
            Ok(Self {
                panel,
                status,
                device,
                bars,
                levels,
                motion: Motion::default(),
                theme: crate::ui::theme::Tracker::default(),
                last: Instant::now(),
                _main_thread: PhantomData,
            })
        }
    }
    pub fn window_number(&self) -> isize {
        // SAFETY: retained NSWindow, documented NSInteger return, main-thread only type.
        unsafe { msg_send![&self.panel, windowNumber] }
    }
    pub fn toggle(&self) {
        // SAFETY: self cannot be sent between threads; panel is retained for this call.
        unsafe {
            let visible: bool = msg_send![&self.panel, isVisible];
            if visible {
                let _: () = msg_send![&self.panel, orderOut: None::<&AnyObject>];
            } else {
                let _: () = msg_send![&self.panel, orderFrontRegardless];
            }
        }
    }
    pub fn is_visible(&self) -> bool {
        // SAFETY: retained NSPanel, main-thread-only owner and BOOL return.
        unsafe { msg_send![&self.panel, isVisible] }
    }
    pub fn tick(&mut self, runtime: &serde_json::Value, bypass: bool) {
        if !self.is_visible() {
            self.last = Instant::now();
            return;
        }
        let dt = self.last.elapsed().as_secs_f64();
        self.last = Instant::now();
        let now = crate::analysis::now_ms();
        self.motion.update(runtime, dt, now);
        let palette = self.theme.update(runtime, dt);
        let active = crate::ui::tui::studio::live(runtime, now);
        let reduced_motion =
            crate::ui::tui::studio::appearance::Appearance::from_environment().reduced_motion;
        let output =
            crate::ui::theme::clean(runtime["output"].as_str().unwrap_or(t("System default")));
        let state = crate::ui::desktop::monitor_state::status(runtime, bypass, now);
        // SAFETY: retained AppKit controls, valid NSRect and NSString arguments, main thread.
        unsafe {
            let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
            let system_reduced_motion: bool =
                msg_send![&workspace, accessibilityDisplayShouldReduceMotion];
            let reduced_motion = reduced_motion || system_reduced_motion;
            let _: () = msg_send![&self.device, setStringValue: &*NSString::from_str(&output)];
            let _: () = msg_send![&self.status, setStringValue: &*NSString::from_str(&state)];
            let tint = ns_color(if active {
                palette.meter
            } else if runtime["active"] == true {
                palette.warning
            } else {
                palette.muted
            });
            let _: () = msg_send![&self.status, setTextColor: &*tint];
            for (i, bar) in self.bars.iter().enumerate() {
                let _: () = msg_send![bar, setHidden: reduced_motion || !active];
                let _: () = msg_send![bar, setFrame: rect(10.0+i as f64*7.2,8.0,3.8,(self.motion.bands[i]*20.0).max(1.0))];
                let hue = i as f64 / 23.0;
                let tint = ns_color(crate::ui::theme::blend(
                    palette.accent,
                    palette.secondary,
                    hue,
                ));
                let _: () = msg_send![bar, setFillColor: &*tint];
            }
            for (i, bar) in self.levels.iter().enumerate() {
                let _: () = msg_send![bar, setHidden: reduced_motion || !active];
                let _: () = msg_send![bar, setFrame: rect(190.0,22.0-i as f64*8.0,(self.motion.levels[i]*62.0).max(1.0),3.0)];
                let tint = ns_color(palette.meter);
                let _: () = msg_send![bar, setFillColor: &*tint];
            }
        }
    }
}
// SAFETY: helpers are called only by main-thread constructors and use AppKit object types.
unsafe fn color(r: f64, g: f64, b: f64) -> Retained<AnyObject> {
    msg_send![class!(NSColor), colorWithCalibratedRed:r, green:g, blue:b, alpha:1.0_f64]
}
unsafe fn ns_color(value: Color) -> Retained<AnyObject> {
    match value {
        Color::Rgb(r, g, b) => color(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0),
        _ => color(0.67, 0.72, 0.90),
    }
}
unsafe fn label(
    parent: &AnyObject,
    frame: NSRect,
    title: &str,
    size: f64,
    muted: bool,
) -> Retained<AnyObject> {
    let label: Retained<AnyObject> =
        msg_send![class!(NSTextField), labelWithString:&*NSString::from_str(title)];
    let font: Retained<AnyObject> = msg_send![class!(NSFont), systemFontOfSize:size];
    let tint = if muted {
        color(0.57, 0.60, 0.65)
    } else {
        color(0.89, 0.90, 0.93)
    };
    let _: () = msg_send![&label,setFrame:frame];
    let _: () = msg_send![&label,setFont:&*font];
    let _: () = msg_send![&label,setTextColor:&*tint];
    let _: () = msg_send![&label,setLineBreakMode:4_usize];
    let _: () = msg_send![parent,addSubview:&*label];
    label
}
unsafe fn bar(parent: &AnyObject, frame: NSRect, tint: Retained<AnyObject>) -> Retained<AnyObject> {
    let allocated: Allocated<AnyObject> = msg_send![class!(NSBox), alloc];
    let bar: Retained<AnyObject> = msg_send![allocated,initWithFrame:frame];
    let _: () = msg_send![&bar,setBoxType:4_usize];
    let _: () = msg_send![&bar,setBorderType:0_usize];
    let _: () = msg_send![&bar,setTitlePosition:0_usize];
    let _: () = msg_send![&bar,setCornerRadius:1.5_f64];
    let _: () = msg_send![&bar,setFillColor:&*tint];
    let _: () = msg_send![parent,addSubview:&*bar];
    bar
}
impl Drop for Hud {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![&self.panel, close];
        }
    }
}
