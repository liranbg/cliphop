use std::cell::Cell;
use std::ptr::NonNull;

use block2::StackBlock;
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSBackingStoreType, NSEvent,
    NSEventMask, NSFloatingWindowLevel, NSFont, NSMenu, NSMenuItem, NSPanel,
    NSTextField, NSView, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    MainThreadMarker, NSDate, NSPoint, NSRect, NSRunLoop, NSSize, NSString, ns_string,
};

// ── Shared state (thread-local, main thread only) ────────────────────────────

#[derive(Debug, Copy, Clone)]
pub enum PopupAction {
    Paste { pinned: bool, index: usize },
    Pin { history_index: usize },
    Unpin { pinned_index: usize },
    DeleteHistory { index: usize },
    DeletePinned { index: usize },
}

thread_local! {
    static POPUP_ACTION: Cell<Option<PopupAction>> = const { Cell::new(None) };
    static POPUP_DISMISSED: Cell<bool> = const { Cell::new(false) };
}

// ── PopupRowView — one row in the history or pinned list ─────────────────────

define_class!(
    #[unsafe(super(NSView))]
    #[name = "PopupRowView"]
    pub struct PopupRowView;

    impl PopupRowView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            // tag encodes: pinned flag (bit 16) | index (bits 0-15)
            let tag: isize = unsafe { msg_send![self, tag] };
            let is_pinned = (tag >> 16) & 1 == 1;
            let index = (tag & 0xFFFF) as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::Paste { pinned: is_pinned, index })));
            POPUP_DISMISSED.with(|c| c.set(true));
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            let tag: isize = unsafe { msg_send![self, tag] };
            let is_pinned = (tag >> 16) & 1 == 1;
            let index = (tag & 0xFFFF) as usize;
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let context_target: Retained<PopupContextTarget> =
                unsafe { msg_send![PopupContextTarget::alloc(), init] };
            let menu = build_context_menu(is_pinned, index, &context_target, mtm);
            let location = unsafe { NSEvent::mouseLocation() };
            unsafe {
                let _: () = msg_send![
                    &*menu,
                    popUpMenuPositioningItem: std::ptr::null::<NSMenuItem>(),
                    atLocation: location,
                    inView: std::ptr::null::<NSView>()
                ];
            }
            let _ = context_target; // keep alive until menu dismissed
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: &NSEvent) -> bool {
            true
        }
    }
);

// ── Context menu action target ────────────────────────────────────────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PopupContextTarget"]
    pub struct PopupContextTarget;

    impl PopupContextTarget {
        #[unsafe(method(pinItem:))]
        fn pin_item(&self, sender: &NSMenuItem) {
            let index = sender.tag() as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::Pin { history_index: index })));
            POPUP_DISMISSED.with(|c| c.set(true));
        }

        #[unsafe(method(unpinItem:))]
        fn unpin_item(&self, sender: &NSMenuItem) {
            let index = sender.tag() as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::Unpin { pinned_index: index })));
            POPUP_DISMISSED.with(|c| c.set(true));
        }

        #[unsafe(method(deleteHistoryItem:))]
        fn delete_history_item(&self, sender: &NSMenuItem) {
            let index = sender.tag() as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::DeleteHistory { index })));
            POPUP_DISMISSED.with(|c| c.set(true));
        }

        #[unsafe(method(deletePinnedItem:))]
        fn delete_pinned_item(&self, sender: &NSMenuItem) {
            let index = sender.tag() as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::DeletePinned { index })));
            POPUP_DISMISSED.with(|c| c.set(true));
        }
    }
);

// ── Window resign-key delegate — dismisses panel when focus leaves ────────────

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PopupWindowDelegate"]
    pub struct PopupWindowDelegate;

    impl PopupWindowDelegate {
        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSObject) {
            POPUP_DISMISSED.with(|c| c.set(true));
        }
    }
);

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_context_menu(
    is_pinned: bool,
    index: usize,
    target: &PopupContextTarget,
    mtm: MainThreadMarker,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);

    if is_pinned {
        let unpin = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Unpin"),
                Some(sel!(unpinItem:)),
                ns_string!(""),
            )
        };
        unpin.setTag(index as isize);
        unsafe { unpin.setTarget(Some(target)) };
        menu.addItem(&unpin);

        let delete = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Delete"),
                Some(sel!(deletePinnedItem:)),
                ns_string!(""),
            )
        };
        delete.setTag(index as isize);
        unsafe { delete.setTarget(Some(target)) };
        menu.addItem(&delete);
    } else {
        let pin = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Pin"),
                Some(sel!(pinItem:)),
                ns_string!(""),
            )
        };
        pin.setTag(index as isize);
        unsafe { pin.setTarget(Some(target)) };
        menu.addItem(&pin);

        let delete = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Delete from history"),
                Some(sel!(deleteHistoryItem:)),
                ns_string!(""),
            )
        };
        delete.setTag(index as isize);
        unsafe { delete.setTarget(Some(target)) };
        menu.addItem(&delete);
    }

    menu
}

/// Shows a popup panel at the cursor with clipboard items.
/// Items: `(index, label, tooltip)` for history items.
/// Pinned: `(index, label, tooltip)` for pinned shelf items.
/// Returns `Some(PopupAction)` or `None` if dismissed without selection.
pub fn show_popup(
    items: &[(usize, String, String)],
    pinned: &[(usize, String, String)],
    mtm: MainThreadMarker,
) -> Option<PopupAction> {
    POPUP_ACTION.with(|c| c.set(None));
    POPUP_DISMISSED.with(|c| c.set(false));

    // Save frontmost app for focus restore on paste
    let workspace = NSWorkspace::sharedWorkspace();
    let frontmost = workspace.frontmostApplication();

    // Activate Cliphop so the panel can become key window
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);

    // Panel dimensions
    const W: f64 = 320.0;
    const ROW_H: f64 = 28.0;
    const SEARCH_H: f64 = 36.0;
    const PIN_HEADER_H: f64 = 20.0;
    const MIN_H: f64 = 28.0; // minimum height for empty state label

    let history_h = items.len() as f64 * ROW_H;
    let pinned_h = if pinned.is_empty() {
        0.0
    } else {
        PIN_HEADER_H + pinned.len() as f64 * ROW_H
    };
    let content_h = (history_h + pinned_h).max(MIN_H);
    let total_h = SEARCH_H + content_h;

    let location = unsafe { NSEvent::mouseLocation() };
    let frame = NSRect::new(
        NSPoint::new(location.x, location.y - total_h),
        NSSize::new(W, total_h),
    );

    // Create borderless floating panel
    let panel: Retained<NSPanel> = unsafe {
        msg_send![
            mtm.alloc::<NSPanel>(),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    unsafe { panel.setLevel(NSFloatingWindowLevel) };

    // Attach dismiss delegate
    let delegate: Retained<PopupWindowDelegate> =
        unsafe { msg_send![PopupWindowDelegate::alloc(), init] };
    unsafe { let (): () = msg_send![&*panel, setDelegate: &*delegate]; }

    let content = panel.contentView().unwrap();

    // ── Search field at top ───────────────────────────────────────────
    let search_frame = NSRect::new(
        NSPoint::new(4.0, total_h - SEARCH_H + 4.0),
        NSSize::new(W - 8.0, SEARCH_H - 8.0),
    );
    let search_field = NSTextField::initWithFrame(mtm.alloc(), search_frame);
    search_field.setPlaceholderString(Some(ns_string!("Type to filter...")));
    unsafe {
        content.addSubview(&search_field);
    }

    // ── History rows ──────────────────────────────────────────────────
    let mut row_y = total_h - SEARCH_H - ROW_H;

    if items.is_empty() && pinned.is_empty() {
        // Empty state
        let empty =
            NSTextField::labelWithString(ns_string!("No clipboard history yet"), mtm);
        empty.setFrame(NSRect::new(
            NSPoint::new(0.0, row_y),
            NSSize::new(W, ROW_H),
        ));
        unsafe {
            let _: () = msg_send![&*empty, setAlignment: 1_isize]; // NSTextAlignmentCenter
            content.addSubview(&empty);
        }
    } else {
        for (idx, label, tooltip) in items {
            let row_frame = NSRect::new(NSPoint::new(0.0, row_y), NSSize::new(W, ROW_H));
            let row: Retained<PopupRowView> =
                unsafe { msg_send![mtm.alloc::<PopupRowView>(), initWithFrame: row_frame] };
            unsafe { let _: () = msg_send![&*row, setTag: *idx as isize]; }
            row.setToolTip(Some(&NSString::from_str(tooltip)));

            let label_view =
                NSTextField::labelWithString(&NSString::from_str(label), mtm);
            label_view.setFrame(NSRect::new(
                NSPoint::new(8.0, 5.0),
                NSSize::new(W - 16.0, ROW_H - 10.0),
            ));
            unsafe {
                row.addSubview(&label_view);
                content.addSubview(&row);
            }
            row_y -= ROW_H;
        }

        // ── Pinned shelf ──────────────────────────────────────────────
        if !pinned.is_empty() {
            // Separator
            let sep_frame = NSRect::new(
                NSPoint::new(0.0, row_y + ROW_H),
                NSSize::new(W, 1.0),
            );
            let sep = NSView::initWithFrame(mtm.alloc(), sep_frame);
            unsafe { content.addSubview(&sep) };
            row_y -= 1.0;

            // Header
            let hdr_frame =
                NSRect::new(NSPoint::new(0.0, row_y - PIN_HEADER_H), NSSize::new(W, PIN_HEADER_H));
            let hdr_bg = NSView::initWithFrame(mtm.alloc(), hdr_frame);
            let hdr_label = NSTextField::labelWithString(ns_string!("PINNED"), mtm);
            hdr_label.setFrame(NSRect::new(
                NSPoint::new(8.0, 3.0),
                NSSize::new(W - 16.0, 14.0),
            ));
            let small = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
            hdr_label.setFont(Some(&small));
            unsafe {
                hdr_bg.addSubview(&hdr_label);
                content.addSubview(&hdr_bg);
            }
            row_y -= PIN_HEADER_H;

            for (idx, label, tooltip) in pinned {
                let row_frame =
                    NSRect::new(NSPoint::new(0.0, row_y), NSSize::new(W, ROW_H));
                let row: Retained<PopupRowView> = unsafe {
                    msg_send![mtm.alloc::<PopupRowView>(), initWithFrame: row_frame]
                };
                unsafe { let _: () = msg_send![&*row, setTag: ((1_isize << 16) | *idx as isize)]; }
                row.setToolTip(Some(&NSString::from_str(tooltip)));

                let pin_label = NSTextField::labelWithString(
                    &NSString::from_str(&format!("📌 {}", label)),
                    mtm,
                );
                pin_label.setFrame(NSRect::new(
                    NSPoint::new(8.0, 5.0),
                    NSSize::new(W - 16.0, ROW_H - 10.0),
                ));
                unsafe {
                    row.addSubview(&pin_label);
                    content.addSubview(&row);
                }
                row_y -= ROW_H;
            }
        }
    }

    // ── Keyboard event monitor (Esc, Enter) ───────────────────────────
    let search_ptr = &*search_field as *const NSTextField as usize; // pass as usize for Send
    let have_items = !items.is_empty();

    let kb_block = StackBlock::new(
        move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let key_code = unsafe { event.as_ref().keyCode() };
            match key_code {
                53 => {
                    // Esc
                    let search_ref = unsafe { &*(search_ptr as *const NSTextField) };
                    let query = search_ref.stringValue();
                    if query.len() == 0 {
                        POPUP_DISMISSED.with(|c| c.set(true));
                    } else {
                        search_ref.setStringValue(ns_string!(""));
                    }
                    std::ptr::null_mut()
                }
                36 | 76 if have_items => {
                    // Enter — select most-recent history item
                    if POPUP_ACTION.with(|c| c.get()).is_none() {
                        POPUP_ACTION.with(|c| {
                            c.set(Some(PopupAction::Paste { pinned: false, index: 0 }))
                        });
                        POPUP_DISMISSED.with(|c| c.set(true));
                    }
                    std::ptr::null_mut()
                }
                _ => event.as_ptr(),
            }
        },
    );

    let kb_monitor: Retained<NSObject> = unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        msg_send![
            cls,
            addLocalMonitorForEventsMatchingMask: NSEventMask::KeyDown,
            handler: &kb_block
        ]
    };

    // ── Show panel and focus search field ─────────────────────────────
    panel.makeKeyAndOrderFront(None);
    unsafe { let _: () = msg_send![&*panel, makeFirstResponder: &*search_field]; }

    // ── Spin nested run loop until dismissed ──────────────────────────
    let run_loop = unsafe { NSRunLoop::currentRunLoop() };
    let mode = NSString::from_str("kCFRunLoopDefaultMode");

    while !POPUP_DISMISSED.with(|c| c.get()) {
        let until = unsafe { NSDate::dateWithTimeIntervalSinceNow(0.05) };
        let _: bool =
            unsafe { msg_send![&*run_loop, runMode: &*mode, beforeDate: &*until] };
    }

    // Remove keyboard monitor
    unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        let (): () = msg_send![cls, removeMonitor: &*kb_monitor];
    }

    panel.orderOut(None);

    let result = POPUP_ACTION.with(|c| c.get());

    // Restore focus to previous app (only for Paste actions)
    if matches!(result, Some(PopupAction::Paste { .. })) {
        if let Some(prev) = &frontmost {
            #[allow(deprecated)]
            prev.activateWithOptions(
                NSApplicationActivationOptions::ActivateIgnoringOtherApps,
            );
        }
    }

    result
}
