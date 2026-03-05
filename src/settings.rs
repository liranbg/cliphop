use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBox,
    NSBoxType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont, NSImage,
    NSMenuItem, NSRunningApplication, NSTextField, NSView, NSWindow,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, ns_string};

// Holds the settings alert window while the dialog is open, so re-clicking
// "Settings..." brings it back to front instead of opening a second dialog.
thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

// Target for the "Settings..." tray menu item
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SettingsTarget"]
    pub struct SettingsTarget;

    impl SettingsTarget {
        #[unsafe(method(showSettings:))]
        fn show_settings_action(&self, _sender: &NSObject) {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            // If the dialog is already open, just bring it back to front.
            let already_open = SETTINGS_WINDOW.with(|w| w.borrow().is_some());
            if already_open {
                SETTINGS_WINDOW.with(|w| {
                    if let Some(window) = w.borrow().as_ref() {
                        window.makeKeyAndOrderFront(None);
                    }
                });
                #[allow(deprecated)]
                NSRunningApplication::currentApplication()
                    .activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
                return;
            }

            show_settings(mtm);
        }

        // Keep the menu item enabled even while a modal dialog is running.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, _item: &NSMenuItem) -> bool {
            true
        }
    }
);

impl SettingsTarget {
    pub fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// ── Accessibility UI targets ─────────────────────────────────────────

// Target for the "Request Access" button inside the settings dialog.
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "OpenAccessibilityTarget"]
    struct OpenAccessibilityTarget;

    impl OpenAccessibilityTarget {
        #[unsafe(method(requestAccess:))]
        fn request_access(&self, _sender: &NSObject) {
            crate::macos::request_accessibility_trust();
            // Lower our window level so the OS accessibility prompt appears in front.
            SETTINGS_WINDOW.with(|w| {
                if let Some(window) = w.borrow().as_ref() {
                    window.setLevel(0);
                }
            });
        }
    }
);

impl OpenAccessibilityTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// Thread-locals for live accessibility status updates in the settings dialog.
thread_local! {
    static AX_STATUS_LABEL: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static AX_OPEN_BUTTON: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
}

// Timer target that polls accessibility status and updates the settings UI.
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AccessibilityTimerTarget"]
    struct AccessibilityTimerTarget;

    impl AccessibilityTimerTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSObject) {
            let trusted = crate::macos::is_accessibility_trusted();
            AX_STATUS_LABEL.with(|label| {
                if let Some(label) = label.borrow().as_ref() {
                    let text = if trusted {
                        "\u{2705} Granted"
                    } else {
                        "\u{26A0}\u{FE0F} Not Granted"
                    };
                    label.setStringValue(&NSString::from_str(text));
                    if trusted {
                        label.setToolTip(None);
                    } else {
                        label.setToolTip(Some(&NSString::from_str(
                            "Remove and re-add Cliphop in System Settings > \
                             Privacy & Security > Accessibility",
                        )));
                    }
                }
            });
            AX_OPEN_BUTTON.with(|button| {
                if let Some(button) = button.borrow().as_ref() {
                    button.setHidden(trusted);
                }
            });
        }
    }
);

impl AccessibilityTimerTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

const W: f64 = 300.0;

fn make_header(text: &str, y: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(NSRect::new(NSPoint::new(0.0, y), NSSize::new(W, 18.0)));
    let bold = NSFont::boldSystemFontOfSize(NSFont::systemFontSize());
    label.setFont(Some(&bold));
    label
}

fn make_label(text: &str, y: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(NSRect::new(NSPoint::new(0.0, y), NSSize::new(W, 18.0)));
    label
}

fn make_separator(y: f64, mtm: MainThreadMarker) -> Retained<NSBox> {
    let sep = NSBox::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0.0, y), NSSize::new(W, 1.0)),
    );
    sep.setBoxType(NSBoxType::Separator);
    sep
}

/// Starts a 2-second timer that polls accessibility status and updates the
/// given label and button. Uses `NSRunLoopCommonModes` so it fires during
/// modal dialog sessions (`runModal` uses `NSModalPanelRunLoopMode`).
fn start_accessibility_timer(
    label: Retained<NSTextField>,
    button: Retained<NSButton>,
) -> Retained<NSObject> {
    AX_STATUS_LABEL.with(|l| *l.borrow_mut() = Some(label));
    AX_OPEN_BUTTON.with(|b| *b.borrow_mut() = Some(button));

    let timer_target = AccessibilityTimerTarget::new();
    unsafe {
        let timer_cls = objc2::runtime::AnyClass::get(c"NSTimer").unwrap();
        let timer: Retained<NSObject> = msg_send![
            timer_cls,
            timerWithTimeInterval: 2.0_f64,
            target: &*timer_target,
            selector: objc2::sel!(tick:),
            userInfo: Option::<&NSObject>::None,
            repeats: true
        ];

        let run_loop_cls = objc2::runtime::AnyClass::get(c"NSRunLoop").unwrap();
        let run_loop: Retained<NSObject> = msg_send![run_loop_cls, currentRunLoop];
        let common_modes = NSString::from_str("kCFRunLoopCommonModes");
        let () = msg_send![&*run_loop, addTimer: &*timer, forMode: &*common_modes];

        timer
    }
}

/// Stops the accessibility status timer and clears view references.
fn stop_accessibility_timer(timer: &NSObject) {
    let () = unsafe { msg_send![timer, invalidate] };
    AX_STATUS_LABEL.with(|l| *l.borrow_mut() = None);
    AX_OPEN_BUTTON.with(|b| *b.borrow_mut() = None);
}

// ── Dialog ───────────────────────────────────────────────────────────

fn show_settings(mtm: MainThreadMarker) {
    let alert = NSAlert::new(mtm);

    // Icon: same SF Symbol as the tray
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        ns_string!("doc.on.clipboard"),
        Some(ns_string!("Cliphop")),
    ) {
        unsafe { alert.setIcon(Some(&image)) };
    }

    alert.setMessageText(&NSString::from_str("Cliphop"));
    alert.setInformativeText(&NSString::from_str(&format!(
        "Version {}",
        env!("CARGO_PKG_VERSION")
    )));

    // ── Accessory view ───────────────────────────────────────────────
    let h: f64 = 118.0;
    let container = NSView::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, h)),
    );

    // Section: Accessibility
    let ax_header = make_header("Accessibility", 97.0, mtm);
    container.addSubview(&ax_header);

    let trusted = crate::macos::is_accessibility_trusted();

    // Accessibility status label (always present, text updated live by timer)
    let ax_status = make_label(
        if trusted {
            "\u{2705} Granted"
        } else {
            "\u{26A0}\u{FE0F} Not Granted"
        },
        76.0,
        mtm,
    );
    ax_status.setFrame(NSRect::new(
        NSPoint::new(0.0, 76.0),
        NSSize::new(120.0, 18.0),
    ));
    if !trusted {
        ax_status.setToolTip(Some(&NSString::from_str(
            "Remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility",
        )));
    }
    container.addSubview(&ax_status);

    // "Request Access" button (always present, hidden when already trusted)
    let open_target = OpenAccessibilityTarget::new();
    let open_button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Request Access"),
            Some(&open_target),
            Some(objc2::sel!(requestAccess:)),
            mtm,
        )
    };
    open_button.setFrame(NSRect::new(
        NSPoint::new(120.0, 72.0),
        NSSize::new(175.0, 26.0),
    ));
    open_button.setHidden(trusted);
    container.addSubview(&open_button);

    // Separator
    let sep = make_separator(64.0, mtm);
    container.addSubview(&sep);

    // Section: Logging
    let log_header = make_header("Logging", 42.0, mtm);
    container.addSubview(&log_header);

    let checkbox = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str("Verbose logging"),
            None,
            None,
            mtm,
        )
    };
    checkbox.setFrame(NSRect::new(NSPoint::new(0.0, 20.0), NSSize::new(W, 20.0)));
    let current_state = if crate::log::is_verbose() {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    };
    checkbox.setState(current_state);
    container.addSubview(&checkbox);

    let path_label = make_label(&crate::log::log_path(), 0.0, mtm);
    path_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let small = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
    path_label.setFont(Some(&small));
    container.addSubview(&path_label);

    alert.setAccessoryView(Some(&container));
    alert.addButtonWithTitle(&NSString::from_str("Close"));

    // Track the alert window so re-clicking "Settings..." can refocus it.
    SETTINGS_WINDOW.with(|w| *w.borrow_mut() = Some(alert.window()));

    // Bring the app to front without changing the activation policy (which would show a Dock icon).
    // NSRunningApplication activates the process directly, unlike NSApplication.activate()
    // which can switch the policy from .accessory to .regular on macOS 14+.
    #[allow(deprecated)]
    NSRunningApplication::currentApplication()
        .activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);

    // Start live accessibility status polling (updates label & button while modal runs)
    let timer = start_accessibility_timer(ax_status, open_button);

    alert.runModal();

    // Stop the timer and clear view references
    stop_accessibility_timer(&timer);

    // Clear the window reference now that the dialog has closed.
    SETTINGS_WINDOW.with(|w| *w.borrow_mut() = None);

    // Hide the dock icon again — showing NSAlert switches the policy to Regular.
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Persist checkbox state after dialog closes
    let new_verbose = checkbox.state() == NSControlStateValueOn;
    crate::log::set_verbose(new_verbose);
    if new_verbose {
        crate::log::log("Verbose logging enabled");
    }
}
