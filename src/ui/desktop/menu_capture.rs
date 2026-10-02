//! AppKit view snapshots for the isolated native-menu review, never desktop capture.
use anyhow::{ensure, Context, Result};
use block2::StackBlock;
use objc2::{
    class, define_class, msg_send, rc::Retained, runtime::AnyObject, sel, DefinedClass,
    MainThreadMarker, MainThreadOnly,
};
use objc2_foundation::{NSData, NSObject, NSPoint, NSRect, NSSize, NSString};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
};
use tray_icon::menu::{ContextMenu, Menu};

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

struct MenuSnapshot {
    menu: Retained<AnyObject>,
    header: Retained<AnyObject>,
    path: PathBuf,
    appearance: String,
    result: RefCell<Option<std::result::Result<Value, String>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MarisIsolatedMenuSnapshot"]
    #[ivars = MenuSnapshot]
    struct SnapshotTarget;

    impl SnapshotTarget {
        #[unsafe(method(snapshotMenu:))]
        fn snapshot_menu(&self, _timer: &AnyObject) {
            let state = self.ivars();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Value> {
                // SAFETY: this callback runs on the GUI thread while the owned
                // NSMenu is tracking. Only its retained header's window is read.
                unsafe {
                    let window: Option<Retained<AnyObject>> = msg_send![&state.header, window];
                    let window = window.context("Isolated menu has no native window")?;
                    let view: Option<Retained<AnyObject>> = msg_send![&window, contentView];
                    let view = view.context("Isolated menu has no native content view")?;
                    capture(&view, &state.path, &state.appearance)
                }
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Native snapshot callback panicked")))
            .map_err(|error| error.to_string());
            if let Ok(mut slot) = state.result.try_borrow_mut() {
                *slot = Some(result);
            }
            // SAFETY: cancel only the menu retained by this isolated review.
            unsafe {
                let _: () = msg_send![&state.menu, cancelTrackingWithoutAnimation];
            }
        }
    }
);

/// Open only the caller's isolated fixture menu, snapshot its complete AppKit
/// content view, then dismiss it without selecting or dispatching any item.
pub(super) fn capture_menu(
    menu: &Menu,
    header: &AnyObject,
    button: &AnyObject,
    path: &Path,
    appearance: &str,
) -> Result<Value> {
    let main = MainThreadMarker::new().context("Menu capture requires the main thread")?;
    ensure!(
        matches!(appearance, "light" | "dark"),
        "Menu capture appearance must be light or dark"
    );
    // SAFETY: ContextMenu owns the documented NSMenu pointer. Header and button
    // stay borrowed throughout tracking, and retained callback objects cannot
    // outlive the main-thread operation. No global window or event is inspected.
    unsafe {
        let is_header: bool = msg_send![header, isKindOfClass:class!(NSView)];
        let is_button: bool = msg_send![button, isKindOfClass:class!(NSButton)];
        ensure!(
            is_header && is_button,
            "Menu capture requires its native header and button"
        );
        let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
        let selected: Option<Retained<AnyObject>> = msg_send![class!(NSAppearance),
            appearanceNamed:&*NSString::from_str(if appearance == "dark" {"NSAppearanceNameDarkAqua"} else {"NSAppearanceNameAqua"})];
        let selected = selected.context("Menu capture appearance is unavailable")?;
        // Native menu cells choose their appearance while the popup is built.
        // Scope the caller's isolated process and anchor before tracking starts,
        // rather than recoloring cached light-mode cells after they are shown.
        let _restore_app = Appearance {
            view: &app,
            previous: msg_send![&app, appearance],
        };
        let _restore_button = Appearance {
            view: button,
            previous: msg_send![button, appearance],
        };
        let _restore_header = Appearance {
            view: header,
            previous: msg_send![header, appearance],
        };
        let _: () = msg_send![&app, setAppearance:&*selected];
        let _: () = msg_send![button, setAppearance:&*selected];
        let _: () = msg_send![header, setAppearance:&*selected];
        let native = Retained::retain(menu.ns_menu().cast::<AnyObject>())
            .context("Isolated menu has no native NSMenu")?;
        let header = Retained::retain((header as *const AnyObject).cast_mut())
            .context("Missing isolated menu header")?;
        let target = SnapshotTarget::alloc(main).set_ivars(MenuSnapshot {
            menu: native.clone(),
            header,
            path: path.to_owned(),
            appearance: appearance.to_owned(),
            result: RefCell::new(None),
        });
        let target: Retained<SnapshotTarget> = msg_send![super(target), init];
        let timer: Retained<AnyObject> = msg_send![class!(NSTimer),
            timerWithTimeInterval:0.15_f64, target:&*target,
            selector:sel!(snapshotMenu:), userInfo:None::<&AnyObject>, repeats:false];
        let run_loop: Retained<AnyObject> = msg_send![class!(NSRunLoop), currentRunLoop];
        // Menu tracking runs in its own public AppKit run-loop mode.
        let _: () = msg_send![&run_loop, addTimer:&*timer,
            forMode:&*NSString::from_str("NSEventTrackingRunLoopMode")];
        let bounds: NSRect = msg_send![button, bounds];
        let _: bool = msg_send![&native, popUpMenuPositioningItem:None::<&AnyObject>,
            atLocation:NSPoint::new(0.0,bounds.size.height), inView:button];
        let _: () = msg_send![&timer, invalidate];
        let result = target
            .ivars()
            .result
            .borrow_mut()
            .take()
            .context("Isolated menu closed before native capture completed")?;
        result.map_err(anyhow::Error::msg)
    }
}

/// Read the actual native image and accessibility state, rather than inferring
/// template dimensions from the Rust icon generator.
pub(super) fn image_metadata(button: &AnyObject) -> Result<Value> {
    ensure!(
        MainThreadMarker::new().is_some(),
        "Native image review requires the main thread"
    );
    // SAFETY: the checked NSButton owns its NSImage; every image representation
    // and accessibility object is retained while documented properties are read.
    unsafe {
        let is_button: bool = msg_send![button, isKindOfClass:class!(NSButton)];
        ensure!(is_button, "Native image review requires an NSButton");
        let image: Option<Retained<AnyObject>> = msg_send![button, image];
        let image = image.context("Native status button has no image")?;
        let template: bool = msg_send![&image, isTemplate];
        let size: NSSize = msg_send![&image, size];
        let representations: Retained<AnyObject> = msg_send![&image, representations];
        let count: usize = msg_send![&representations, count];
        ensure!(
            count <= 16,
            "Native image representation count exceeds the review limit"
        );
        let mut pixels = Vec::new();
        for index in 0..count {
            let item: Retained<AnyObject> = msg_send![&representations, objectAtIndex:index];
            let width: isize = msg_send![&item, pixelsWide];
            let height: isize = msg_send![&item, pixelsHigh];
            pixels.push(json!({"width":width,"height":height}));
        }
        ensure!(
            template && size.width == 18.0 && size.height == 18.0,
            "Native status image must be an 18-point template"
        );
        ensure!(
            pixels
                .iter()
                .any(|value| value["width"] == 64 && value["height"] == 64),
            "Native status image must retain its 64-pixel representation"
        );
        let label: Option<Retained<NSString>> = msg_send![button, accessibilityLabel];
        let value: Option<Retained<AnyObject>> = msg_send![button, accessibilityValue];
        let value = value.map(|value| {
            let description: Retained<NSString> = msg_send![&value, description];
            description.to_string()
        });
        Ok(
            json!({"template":template,"logical_width":size.width,"logical_height":size.height,
            "representations":pixels,"accessibility_label":label.map(|label|label.to_string()),
            "accessibility_value":value}),
        )
    }
}

struct Appearance<'a> {
    view: &'a AnyObject,
    previous: Option<Retained<AnyObject>>,
}
impl Drop for Appearance<'_> {
    fn drop(&mut self) {
        // SAFETY: the checked main-thread NSView stays borrowed until restoration.
        unsafe {
            let _: () = msg_send![self.view, setAppearance:self.previous.as_deref()];
        }
    }
}

/// Snapshot only the supplied production view and its descendants. Transparent
/// areas stay transparent; callers must identify any documentation background.
pub(super) fn capture(view: &AnyObject, path: &Path, appearance: &str) -> Result<Value> {
    ensure!(
        MainThreadMarker::new().is_some(),
        "Native capture requires the main thread"
    );
    let named = match appearance {
        "light" => "NSAppearanceNameAqua",
        "dark" => "NSAppearanceNameDarkAqua",
        _ => anyhow::bail!("Native capture appearance must be light or dark"),
    };
    // SAFETY: AppKit selectors and ABI types match the SDK. The NSView class is
    // checked before view selectors, all returned objects are retained locally,
    // and NSData owns its encoded bytes until they have been copied to disk.
    unsafe {
        let is_view: bool = msg_send![view, isKindOfClass:class!(NSView)];
        ensure!(is_view, "Native capture requires an NSView");
        let bounds: NSRect = msg_send![view, bounds];
        ensure!(
            bounds.size.width.is_finite()
                && bounds.size.height.is_finite()
                && bounds.size.width > 0.0
                && bounds.size.height > 0.0
                && bounds.size.width <= 2048.0
                && bounds.size.height <= 2048.0,
            "Native capture view bounds exceed the review limit"
        );
        let previous = msg_send![view, appearance];
        let _restore = Appearance { view, previous };
        let selected: Option<Retained<AnyObject>> =
            msg_send![class!(NSAppearance), appearanceNamed:&*NSString::from_str(named)];
        let selected = selected.context("Native capture appearance is unavailable")?;
        let _: () = msg_send![view, setAppearance:&*selected];
        let _: () = msg_send![view, layoutSubtreeIfNeeded];
        let rendered = RefCell::new(None);
        let draw = StackBlock::new(|| {
            let bitmap: Option<Retained<AnyObject>> =
                msg_send![view, bitmapImageRepForCachingDisplayInRect:bounds];
            if let Some(bitmap) = &bitmap {
                let _: () = msg_send![view, cacheDisplayInRect:bounds, toBitmapImageRep:&**bitmap];
            }
            rendered.replace(bitmap);
        });
        // AppKit explicitly scopes dynamic colors to this nonescaping drawing block.
        let _: () = msg_send![&selected, performAsCurrentDrawingAppearance:&*draw];
        let bitmap = rendered
            .borrow_mut()
            .take()
            .context("AppKit could not create the native view bitmap")?;
        let width: isize = msg_send![&bitmap, pixelsWide];
        let height: isize = msg_send![&bitmap, pixelsHigh];
        ensure!(
            width > 0 && height > 0 && width <= 4096 && height <= 4096,
            "Native capture bitmap exceeds the review limit"
        );
        let properties: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionary];
        // NSBitmapImageFileTypePNG is NSUInteger value 4 in the AppKit SDK.
        let png: Option<Retained<NSData>> =
            msg_send![&bitmap, representationUsingType:4_usize, properties:&*properties];
        let png = png.context("AppKit could not encode the native view PNG")?;
        ensure!(
            png.len() <= 16 * 1024 * 1024,
            "Native capture PNG exceeds the review limit"
        );
        let bytes = png.to_vec();
        ensure!(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "Native capture did not produce PNG"
        );
        let parent = path
            .parent()
            .context("Native capture path requires a parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        output.write_all(&bytes)?;
        output
            .persist(path)
            .context("Could not publish native view review artifact")?;
        Ok(
            json!({"renderer":"AppKit cacheDisplayInRect:toBitmapImageRep:",
            "appearance":appearance, "logical_width":bounds.size.width,
            "logical_height":bounds.size.height, "pixel_width":width, "pixel_height":height,
            "png_bytes":bytes.len(), "transparent_view_regions":true,
            "whole_desktop_captured":false}),
        )
    }
}
