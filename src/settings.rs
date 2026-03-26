use std::cell::RefCell;
use std::ptr::NonNull;

use block2::StackBlock;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSBox,
    NSBoxType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSEvent,
    NSEventMask, NSFont, NSImage, NSMenuItem, NSRunningApplication, NSStepper, NSTextField, NSView,
    NSWindow, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, ns_string};

// Holds the settings alert window while the dialog is open, so re-clicking
// "Settings..." brings it back to front instead of opening a second dialog.
thread_local! {
    static SETTINGS_WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

// Clear callback registered by main.rs; invoked from both the Settings dialog
// and the tray "Clear History" menu item.
thread_local! {
    static CLEAR_FN: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Register the clear-history callback. Called once at startup by `main.rs`.
pub fn set_clear_fn(f: impl Fn() + 'static) {
    CLEAR_FN.with(|cell| *cell.borrow_mut() = Some(Box::new(f)));
}

pub(crate) fn invoke_clear_fn() {
    CLEAR_FN.with(|cell| {
        if let Some(f) = cell.borrow().as_ref() {
            f();
        }
    });
}

// Tray paste callback registered by main.rs; invoked when a tray history/pinned
// item is clicked. tag encodes: pinned flag (bit 16) | index (bits 0-15).
type TrayPasteFn = Box<dyn Fn(usize)>;
thread_local! {
    static TRAY_PASTE_FN: RefCell<Option<TrayPasteFn>> = const { RefCell::new(None) };
}

pub fn set_tray_paste_fn(f: impl Fn(usize) + 'static) {
    TRAY_PASTE_FN.with(|cell| *cell.borrow_mut() = Some(Box::new(f)));
}

pub(crate) fn invoke_tray_paste_fn(tag: usize) {
    TRAY_PASTE_FN.with(|cell| {
        if let Some(f) = cell.borrow().as_ref() {
            f(tag);
        }
    });
}

// Hotkey re-registration callback registered by main.rs
type ReregisterFn = Box<dyn Fn(&str) -> Result<(), String>>;
thread_local! {
    static REREGISTER_FN: RefCell<Option<ReregisterFn>> =
        const { RefCell::new(None) };
    static HOTKEY_BADGE: RefCell<Option<Retained<NSTextField>>> = const { RefCell::new(None) };
    static HOTKEY_RECORD_BTN: RefCell<Option<Retained<NSButton>>> = const { RefCell::new(None) };
    static HOTKEY_MONITOR: RefCell<Option<Retained<NSObject>>> = const { RefCell::new(None) };
    static HOTKEY_TIMER: RefCell<Option<Retained<NSObject>>> = const { RefCell::new(None) };
}

pub fn set_reregister_fn(f: impl Fn(&str) -> Result<(), String> + 'static) {
    REREGISTER_FN.with(|cell| *cell.borrow_mut() = Some(Box::new(f)));
}

fn invoke_reregister_fn(combo: &str) -> Result<(), String> {
    REREGISTER_FN.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|f| f(combo))
            .unwrap_or_else(|| Err("no reregister fn set".to_string()))
    })
}

fn stop_hotkey_recording() {
    HOTKEY_MONITOR.with(|m| {
        if let Some(monitor) = m.borrow_mut().take() {
            let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
            let (): () = unsafe { msg_send![cls, removeMonitor: &*monitor] };
        }
    });
    HOTKEY_TIMER.with(|t| {
        if let Some(timer) = t.borrow_mut().take() {
            let (): () = unsafe { msg_send![&*timer, invalidate] };
        }
    });
    // Restore button to "Record…"
    HOTKEY_RECORD_BTN.with(|b| {
        if let Some(btn) = b.borrow().as_ref() {
            btn.setTitle(&NSString::from_str("Record\u{2026}"));
            unsafe { btn.setAction(Some(sel!(recordClicked:))) };
        }
    });
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

// Target for the "Clear History" button inside the settings dialog (no confirmation).
define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ClearButtonTarget"]
    struct ClearButtonTarget;

    impl ClearButtonTarget {
        #[unsafe(method(clearClicked:))]
        fn clear_clicked(&self, _sender: &NSObject) {
            invoke_clear_fn();
        }
    }
);

impl ClearButtonTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// ── HotkeyRecordTarget — Record… / Cancel button in General section ──────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HotkeyRecordTarget"]
    struct HotkeyRecordTarget;

    impl HotkeyRecordTarget {
        #[unsafe(method(recordClicked:))]
        fn record_clicked(&self, sender: &NSObject) {
            HOTKEY_BADGE.with(|b| {
                if let Some(badge) = b.borrow().as_ref() {
                    badge.setStringValue(&NSString::from_str("Press shortcut\u{2026}"));
                }
            });
            HOTKEY_RECORD_BTN.with(|b| {
                if let Some(btn) = b.borrow().as_ref() {
                    btn.setTitle(&NSString::from_str("Cancel"));
                    unsafe { btn.setAction(Some(sel!(cancelRecording:))) };
                }
            });

            // Install NSEvent local monitor to capture next modifier+key combo
            let kb_block = StackBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
                let key_code = unsafe { event.as_ref().keyCode() };
                // Ignore pure modifier keypresses
                if [55u16, 56, 57, 58, 59, 60, 61, 62].contains(&key_code) {
                    return event.as_ptr();
                }
                let flags = unsafe { event.as_ref().modifierFlags() };
                let raw_flags: u64 = flags.bits() as u64;
                let chars: Retained<NSString> = unsafe {
                    msg_send![event.as_ref(), charactersIgnoringModifiers]
                };
                let key_str = chars.to_string();
                match crate::hotkey::combo_from_event_flags(raw_flags, &key_str) {
                    Ok(combo) => {
                        let display = crate::hotkey::display_combo(&combo);
                        HOTKEY_BADGE.with(|b| {
                            if let Some(badge) = b.borrow().as_ref() {
                                badge.setStringValue(&NSString::from_str(&display));
                            }
                        });
                        if let Err(e) = invoke_reregister_fn(&combo) {
                            cliphop::log::log(&format!("hotkey re-register failed: {}", e));
                        } else {
                            // Save new hotkey to config
                            let mut cfg = cliphop::config::load();
                            cfg.hotkey = combo;
                            cliphop::config::save(&cfg);
                        }
                        stop_hotkey_recording();
                    }
                    Err(e) => {
                        cliphop::log::log_verbose(&format!("hotkey capture skipped: {}", e));
                        return event.as_ptr();
                    }
                }
                std::ptr::null_mut()
            });

            let monitor: Retained<NSObject> = unsafe {
                let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
                msg_send![
                    cls,
                    addLocalMonitorForEventsMatchingMask: NSEventMask::KeyDown,
                    handler: &kb_block
                ]
            };
            HOTKEY_MONITOR.with(|m| *m.borrow_mut() = Some(monitor));

            // 10-second auto-cancel timer
            let timer_target: Retained<HotkeyTimerTarget> =
                unsafe { msg_send![HotkeyTimerTarget::alloc(), init] };
            let timer: Retained<NSObject> = unsafe {
                let cls = objc2::runtime::AnyClass::get(c"NSTimer").unwrap();
                msg_send![
                    cls,
                    scheduledTimerWithTimeInterval: 10.0_f64,
                    target: &*timer_target,
                    selector: sel!(timerFired:),
                    userInfo: Option::<&NSObject>::None,
                    repeats: false
                ]
            };
            HOTKEY_TIMER.with(|t| *t.borrow_mut() = Some(timer));
        }

        #[unsafe(method(cancelRecording:))]
        fn cancel_recording(&self, _sender: &NSObject) {
            stop_hotkey_recording();
            // Restore badge to current config hotkey display
            let combo = cliphop::config::load().hotkey;
            let display = crate::hotkey::display_combo(&combo);
            HOTKEY_BADGE.with(|b| {
                if let Some(badge) = b.borrow().as_ref() {
                    badge.setStringValue(&NSString::from_str(&display));
                }
            });
        }
    }
);

impl HotkeyRecordTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// ── HotkeyTimerTarget — auto-cancel after 10 seconds ─────────────────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "HotkeyTimerTarget"]
    struct HotkeyTimerTarget;

    impl HotkeyTimerTarget {
        #[unsafe(method(timerFired:))]
        fn timer_fired(&self, _sender: &NSObject) {
            stop_hotkey_recording();
            let combo = cliphop::config::load().hotkey;
            let display = crate::hotkey::display_combo(&combo);
            HOTKEY_BADGE.with(|b| {
                if let Some(badge) = b.borrow().as_ref() {
                    badge.setStringValue(&NSString::from_str(&display));
                }
            });
        }
    }
);

// ── OpenLogTarget — "Open" button next to log path ───────────────────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "OpenLogTarget"]
    struct OpenLogTarget;

    impl OpenLogTarget {
        #[unsafe(method(openLog:))]
        fn open_log(&self, _sender: &NSObject) {
            let path = cliphop::log::log_path();
            unsafe {
                let url_cls = objc2::runtime::AnyClass::get(c"NSURL").unwrap();
                let ns_path = NSString::from_str(&path);
                let url: Retained<NSObject> =
                    msg_send![url_cls, fileURLWithPath: &*ns_path];
                let urls_cls = objc2::runtime::AnyClass::get(c"NSArray").unwrap();
                let urls: Retained<NSObject> =
                    msg_send![urls_cls, arrayWithObject: &*url];
                let ws = NSWorkspace::sharedWorkspace();
                let (): () = msg_send![&*ws, activateFileViewerSelectingURLs: &*urls];
            }
        }
    }
);

impl OpenLogTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

const W: f64 = 340.0;

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
    // Layout (y=0 at bottom, increasing upward):
    //   300: Accessibility header
    //   278: Accessibility status / 275: Request Access button
    //   264: Separator (Accessibility / General)
    //   244: General header
    //   222: Launch at login checkbox
    //   200: Hotkey row (label + badge + Record… button)
    //   180: Hotkey hint
    //   168: Separator (General / History)
    //   148: History header
    //   122: Items retained row
    //    98: Clear all history label + Clear button
    //    86: Separator (History / Logging)
    //    66: Logging header
    //    44: Verbose logging checkbox
    //    22: Log file path + Open button
    let h: f64 = 320.0;
    let container = NSView::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, h)),
    );

    // Section: Accessibility
    let ax_header = make_header("Accessibility", 300.0, mtm);
    container.addSubview(&ax_header);

    let trusted = crate::macos::is_accessibility_trusted();

    // Accessibility status label (always present, text updated live by timer)
    let ax_status = make_label(
        if trusted {
            "\u{2705} Granted"
        } else {
            "\u{26A0}\u{FE0F} Not Granted"
        },
        278.0,
        mtm,
    );
    ax_status.setFrame(NSRect::new(
        NSPoint::new(0.0, 278.0),
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
        NSPoint::new(120.0, 275.0),
        NSSize::new(175.0, 26.0),
    ));
    open_button.setHidden(trusted);
    container.addSubview(&open_button);

    // Separator (Accessibility / General)
    let sep_ax_gen = make_separator(264.0, mtm);
    container.addSubview(&sep_ax_gen);

    // Section: General
    let general_header = make_header("General", 244.0, mtm);
    container.addSubview(&general_header);

    // Launch at login checkbox (moved into General)
    let login_enabled = crate::macos::launch_at_login_status();
    let login_checkbox = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str("Launch at login"),
            None,
            None,
            mtm,
        )
    };
    login_checkbox.setFrame(NSRect::new(NSPoint::new(0.0, 222.0), NSSize::new(W, 20.0)));
    login_checkbox.setState(if login_enabled {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
    container.addSubview(&login_checkbox);

    // Hotkey row: label + badge + Record… button
    let hotkey_label = make_label("Hotkey", 200.0, mtm);
    hotkey_label.setFrame(NSRect::new(
        NSPoint::new(0.0, 200.0),
        NSSize::new(55.0, 22.0),
    ));
    container.addSubview(&hotkey_label);

    let current_hotkey = cliphop::config::load().hotkey;
    let hotkey_display = crate::hotkey::display_combo(&current_hotkey);
    let badge = NSTextField::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(60.0, 200.0), NSSize::new(80.0, 22.0)),
    );
    badge.setStringValue(&NSString::from_str(&hotkey_display));
    badge.setEditable(false);
    badge.setBezeled(true);
    container.addSubview(&badge);

    let record_target = HotkeyRecordTarget::new();
    let record_btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Record\u{2026}"),
            Some(&record_target),
            Some(sel!(recordClicked:)),
            mtm,
        )
    };
    record_btn.setFrame(NSRect::new(
        NSPoint::new(148.0, 198.0),
        NSSize::new(90.0, 26.0),
    ));
    container.addSubview(&record_btn);

    let hint_label = make_label("Click Record, then press your shortcut", 180.0, mtm);
    hint_label.setFrame(NSRect::new(NSPoint::new(0.0, 180.0), NSSize::new(W, 16.0)));
    let tiny_font = NSFont::systemFontOfSize(NSFont::smallSystemFontSize() - 1.0);
    hint_label.setFont(Some(&tiny_font));
    hint_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    container.addSubview(&hint_label);

    // Store badge and button in thread-locals for HotkeyRecordTarget callbacks
    HOTKEY_BADGE.with(|b| *b.borrow_mut() = Some(badge));
    HOTKEY_RECORD_BTN.with(|b| *b.borrow_mut() = Some(record_btn));

    // Separator (General / History)
    let sep1 = make_separator(168.0, mtm);
    container.addSubview(&sep1);

    // Section: History (y values unchanged)
    let history_header = make_header("History", 148.0, mtm);
    container.addSubview(&history_header);

    // History row: "Items retained" label + editable text field + stepper + range hint
    let items_label = NSTextField::labelWithString(&NSString::from_str("Items retained"), mtm);
    items_label.setFrame(NSRect::new(
        NSPoint::new(0.0, 122.0),
        NSSize::new(110.0, 22.0),
    ));
    container.addSubview(&items_label);

    let current_limit = cliphop::clipboard::get_max_history() as isize;

    // y=124 (vs label at y=122): 2px upward nudge to optically center the text field
    // against the taller "Items retained" label.
    let history_field = NSTextField::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(114.0, 124.0), NSSize::new(44.0, 19.0)),
    );
    history_field.setIntegerValue(current_limit);
    container.addSubview(&history_field);

    let stepper = NSStepper::initWithFrame(
        mtm.alloc(),
        NSRect::new(NSPoint::new(160.0, 122.0), NSSize::new(19.0, 22.0)),
    );
    unsafe {
        stepper.setMinValue(cliphop::config::MIN_MAX_HISTORY as f64);
        stepper.setMaxValue(cliphop::config::MAX_MAX_HISTORY as f64);
        stepper.setIncrement(1.0);
        stepper.setIntegerValue(current_limit);
        // Wire stepper → text field: clicking arrows calls `takeIntegerValueFrom:` on
        // the field, which reads the stepper's integerValue and updates its display.
        // The reverse (typing → stepper) is not wired; we always read history_field
        // at close time so the saved value is always correct regardless.
        //
        // Safety: history_field lives on the stack until after runModal() returns, so
        // the raw pointer stored inside the stepper is valid for the entire modal session.
        stepper.setTarget(Some(&*history_field));
        stepper.setAction(Some(objc2::sel!(takeIntegerValueFrom:)));
    }
    container.addSubview(&stepper);

    let range_hint = NSTextField::labelWithString(
        &NSString::from_str(&format!(
            "({}–{})",
            cliphop::config::MIN_MAX_HISTORY,
            cliphop::config::MAX_MAX_HISTORY,
        )),
        mtm,
    );
    range_hint.setFrame(NSRect::new(
        NSPoint::new(184.0, 122.0),
        NSSize::new(80.0, 22.0),
    ));
    range_hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let small_font = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
    range_hint.setFont(Some(&small_font));
    container.addSubview(&range_hint);

    // Clear History: left label + right button
    let clear_btn_target = ClearButtonTarget::new();
    let clear_label = NSTextField::labelWithString(&NSString::from_str("Clear all history"), mtm);
    clear_label.setFrame(NSRect::new(
        NSPoint::new(0.0, 98.0),
        NSSize::new(W - 140.0, 22.0),
    ));
    clear_label.setTextColor(Some(&NSColor::systemRedColor()));
    container.addSubview(&clear_label);
    let clear_button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Clear\u{2026}"),
            Some(&clear_btn_target),
            Some(objc2::sel!(clearClicked:)),
            mtm,
        )
    };
    clear_button.setFrame(NSRect::new(
        NSPoint::new(W - 130.0, 98.0),
        NSSize::new(130.0, 28.0),
    ));
    container.addSubview(&clear_button);

    // Separator (History / Logging)
    let sep2 = make_separator(86.0, mtm);
    container.addSubview(&sep2);

    // Section: Logging
    let log_header = make_header("Logging", 66.0, mtm);
    container.addSubview(&log_header);

    let checkbox = unsafe {
        NSButton::checkboxWithTitle_target_action(
            &NSString::from_str("Verbose logging"),
            None,
            None,
            mtm,
        )
    };
    checkbox.setFrame(NSRect::new(NSPoint::new(0.0, 44.0), NSSize::new(W, 20.0)));
    let current_state = if cliphop::log::is_verbose() {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    };
    checkbox.setState(current_state);
    container.addSubview(&checkbox);

    let open_log_target = OpenLogTarget::new();
    let open_log_btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Open"),
            Some(&open_log_target),
            Some(sel!(openLog:)),
            mtm,
        )
    };
    open_log_btn.setFrame(NSRect::new(
        NSPoint::new(W - 55.0, 18.0),
        NSSize::new(55.0, 22.0),
    ));
    container.addSubview(&open_log_btn);

    let path_label = NSTextField::labelWithString(
        &NSString::from_str(&format!("Log: {}", cliphop::log::log_path())),
        mtm,
    );
    path_label.setFrame(NSRect::new(
        NSPoint::new(0.0, 22.0),
        NSSize::new(W - 60.0, 18.0),
    ));
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

    // Close the settings dialog when the app loses focus (user clicks another window).
    // Without this, the modal session stays active but hidden, blocking the tray.
    let deactivate_observer = unsafe {
        let nc_cls = objc2::runtime::AnyClass::get(c"NSNotificationCenter").unwrap();
        let nc: Retained<NSObject> = msg_send![nc_cls, defaultCenter];
        let app = NSApplication::sharedApplication(mtm);
        let block = StackBlock::new(|_notif: NonNull<NSObject>| {
            let mtm = MainThreadMarker::new_unchecked();
            NSApplication::sharedApplication(mtm).stopModal();
        });
        let name = NSString::from_str("NSApplicationDidResignActiveNotification");
        let observer: Retained<NSObject> = msg_send![
            &*nc,
            addObserverForName: &*name,
            object: &*app,
            queue: Option::<&NSObject>::None,
            usingBlock: &block
        ];
        observer
    };

    alert.runModal();

    // Remove the deactivation observer
    unsafe {
        let nc_cls = objc2::runtime::AnyClass::get(c"NSNotificationCenter").unwrap();
        let nc: Retained<NSObject> = msg_send![nc_cls, defaultCenter];
        let (): () = msg_send![&*nc, removeObserver: &*deactivate_observer];
    }

    // Stop the timer and clear view references
    stop_accessibility_timer(&timer);

    // Stop any in-progress hotkey recording and clear view references
    stop_hotkey_recording();
    HOTKEY_BADGE.with(|b| *b.borrow_mut() = None);
    HOTKEY_RECORD_BTN.with(|b| *b.borrow_mut() = None);

    // Clear the window reference now that the dialog has closed.
    SETTINGS_WINDOW.with(|w| *w.borrow_mut() = None);

    // Hide the dock icon again — showing NSAlert switches the policy to Regular.
    NSApplication::sharedApplication(mtm)
        .setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    // Read and apply settings after dialog closes
    let new_verbose = checkbox.state() == NSControlStateValueOn;
    let new_history = history_field.integerValue().clamp(
        cliphop::config::MIN_MAX_HISTORY as isize,
        cliphop::config::MAX_MAX_HISTORY as isize,
    ) as usize;

    cliphop::log::set_verbose(new_verbose);
    cliphop::clipboard::set_max_history(new_history);
    cliphop::clipboard::request_trim();

    let current_config = cliphop::config::load();
    cliphop::config::save(&cliphop::config::Config {
        verbose_logging: new_verbose,
        max_history: new_history,
        hotkey: current_config.hotkey,
    });

    if new_verbose {
        cliphop::log::log("Verbose logging enabled");
    } else {
        cliphop::log::log("Verbose logging disabled");
    }

    // Apply launch-at-login change if toggled
    let new_login = login_checkbox.state() == NSControlStateValueOn;
    if new_login != login_enabled
        && let Err(e) = crate::macos::set_launch_at_login(new_login)
    {
        cliphop::log::log(&format!("set_launch_at_login failed: {}", e));
    }
}
