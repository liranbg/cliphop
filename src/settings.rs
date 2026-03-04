use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, AnyThread};
use objc2_app_kit::{
    NSAlert, NSBox, NSBoxType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn,
    NSFont, NSImage, NSTextField, NSView,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

// Target for the "Settings..." tray menu item
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SettingsTarget"]
    pub struct SettingsTarget;

    impl SettingsTarget {
        #[unsafe(method(showSettings:))]
        fn show_settings_action(&self, _sender: &NSObject) {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            show_settings(mtm);
        }
    }
);

impl SettingsTarget {
    pub fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// Target for the "Open System Settings" button inside the dialog.
// Delegates to crate::macos::open_accessibility_settings().
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "OpenAccessibilityTarget"]
    struct OpenAccessibilityTarget;

    impl OpenAccessibilityTarget {
        #[unsafe(method(openAccessibilitySettings:))]
        fn open_accessibility_settings(&self, _sender: &NSObject) {
            crate::macos::open_accessibility_settings();
        }
    }
);

impl OpenAccessibilityTarget {
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

    let _open_target = if trusted {
        let ax_status = make_label("\u{2705} Granted", 74.0, mtm);
        container.addSubview(&ax_status);
        None
    } else {
        let ax_status = make_label("\u{26A0}\u{FE0F} Not Granted", 76.0, mtm);
        ax_status.setFrame(NSRect::new(
            NSPoint::new(0.0, 76.0),
            NSSize::new(120.0, 18.0),
        ));
        ax_status.setToolTip(Some(&NSString::from_str(
            "Remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility",
        )));
        container.addSubview(&ax_status);

        let open_target = OpenAccessibilityTarget::new();
        let open_button = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str("Open System Settings"),
                Some(&open_target),
                Some(objc2::sel!(openAccessibilitySettings:)),
                mtm,
            )
        };
        open_button.setFrame(NSRect::new(
            NSPoint::new(120.0, 72.0),
            NSSize::new(175.0, 26.0),
        ));
        container.addSubview(&open_button);
        Some(open_target)
    };

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
    checkbox.setFrame(NSRect::new(
        NSPoint::new(0.0, 20.0),
        NSSize::new(W, 20.0),
    ));
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

    alert.runModal();

    // Persist checkbox state after dialog closes
    let new_verbose = checkbox.state() == NSControlStateValueOn;
    crate::log::set_verbose(new_verbose);
}
