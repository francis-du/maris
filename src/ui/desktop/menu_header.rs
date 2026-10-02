//! Semantic AppKit typography and a compact, accessible menu-bar template mark.
use anyhow::{ensure, Result};
use objc2::{
    class, msg_send,
    rc::{Allocated, Retained},
    runtime::AnyObject,
    MainThreadMarker,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::{marker::PhantomData, rc::Rc};
use tray_icon::{
    menu::{ContextMenu, Menu},
    TrayIcon,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
pub(super) struct Header {
    pub view: Retained<AnyObject>,
    pub button: Retained<AnyObject>,
    status: Retained<AnyObject>,
    output: Retained<AnyObject>,
    previous: (String, String),
    _main_thread: PhantomData<Rc<()>>,
}
impl Header {
    pub fn new(icon: &TrayIcon, menu: &Menu) -> Result<Self> {
        ensure!(
            MainThreadMarker::new().is_some(),
            "Menu header requires the main thread"
        );
        let native_menu = menu.ns_menu();
        ensure!(!native_menu.is_null(), "Missing native status menu");
        // SAFETY: documented AppKit selectors and ABI types; all objects are retained
        // by this non-Send main-thread owner and the native menu retains its view.
        unsafe {
            let item = icon
                .ns_status_item()
                .ok_or_else(|| anyhow::anyhow!("Missing native status item"))?;
            let button: Option<Retained<AnyObject>> = msg_send![&item, button];
            let button = button.ok_or_else(|| anyhow::anyhow!("Missing native status button"))?;
            let allocated: Allocated<AnyObject> = msg_send![class!(NSView), alloc];
            let view: Retained<AnyObject> =
                msg_send![allocated, initWithFrame:rect(0.,0.,320.,86.)];
            label(&view, rect(20., 56., 280., 20.), "Maris", 16., true, false);
            let status = label(
                &view,
                rect(20., 34., 280., 16.),
                crate::i18n::text("Standby"),
                12.,
                false,
                false,
            );
            let output = label(
                &view,
                rect(20., 14., 280., 15.),
                crate::i18n::text("Output"),
                11.,
                false,
                true,
            );
            let native_menu = &*native_menu.cast::<AnyObject>();
            let first: Option<Retained<AnyObject>> = msg_send![native_menu, itemAtIndex:0_isize];
            let first = first.ok_or_else(|| anyhow::anyhow!("Missing native menu header item"))?;
            let _: () = msg_send![&first, setView:&*view];
            size_mark(icon);
            Ok(Self {
                view,
                button,
                status,
                output,
                previous: (String::new(), String::new()),
                _main_thread: PhantomData,
            })
        }
    }
    pub fn update(&mut self, status: &str, output: &str) {
        if self.previous.0 == status && self.previous.1 == output {
            return;
        }
        // SAFETY: retained NSTextField controls, NSString ownership through this call.
        unsafe {
            let _: () = msg_send![&self.status, setStringValue:&*NSString::from_str(status)];
            let _: () = msg_send![&self.output, setStringValue:&*NSString::from_str(output)];
            let _: () = msg_send![&self.view, setAccessibilityLabel:&*NSString::from_str(&format!("Maris. {status}. {output}"))];
            let _: () = msg_send![&self.button, setAccessibilityValue:&*NSString::from_str(&format!("{status}. {output}"))];
        }
        self.previous = (status.to_owned(), output.to_owned());
    }
}

pub(super) fn size_mark(icon: &TrayIcon) {
    if let Some(item) = icon.ns_status_item() {
        // SAFETY: called only on GUI main thread; NSStatusBarButton and NSImage are
        // retained locally, and setSize uses logical points instead of pixel dimensions.
        unsafe {
            let button: Option<Retained<AnyObject>> = msg_send![&item, button];
            let Some(button) = button else {
                return;
            };
            let image: Option<Retained<AnyObject>> = msg_send![&button, image];
            if let Some(image) = image {
                let _: () = msg_send![&image, setSize:NSSize::new(super::status_icon::POINTS,super::status_icon::POINTS)];
            }
            let _: () = msg_send![&button, setAccessibilityLabel:&*NSString::from_str("Maris")];
            let _: () = msg_send![&item, setLength:26.0_f64];
            let _: () = msg_send![&button, setNeedsDisplay:true];
        }
    }
}
pub(super) fn reduced_motion() -> bool {
    let env = crate::ui::tui::studio::appearance::Appearance::from_environment().reduced_motion;
    // SAFETY: singleton NSWorkspace, main thread and documented BOOL selector.
    unsafe {
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let system: bool = msg_send![&workspace, accessibilityDisplayShouldReduceMotion];
        env || system
    }
}
// SAFETY: used only by Header's checked main-thread constructor with retained NSView.
unsafe fn label(
    parent: &AnyObject,
    frame: NSRect,
    title: &str,
    size: f64,
    bold: bool,
    secondary: bool,
) -> Retained<AnyObject> {
    let label: Retained<AnyObject> =
        msg_send![class!(NSTextField), labelWithString:&*NSString::from_str(title)];
    let font: Retained<AnyObject> =
        msg_send![class!(NSFont), systemFontOfSize:size, weight:if bold {0.5_f64} else {0.0_f64}];
    let tint: Retained<AnyObject> = if secondary {
        msg_send![class!(NSColor), secondaryLabelColor]
    } else {
        msg_send![class!(NSColor), labelColor]
    };
    let _: () = msg_send![&label,setFrame:frame];
    let _: () = msg_send![&label,setFont:&*font];
    let _: () = msg_send![&label,setTextColor:&*tint];
    let _: () = msg_send![&label,setLineBreakMode:4_usize];
    let _: () = msg_send![parent,addSubview:&*label];
    label
}
