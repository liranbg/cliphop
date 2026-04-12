use std::cell::Cell;
use std::ptr::NonNull;

use block2::StackBlock;
use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSBackingStoreType, NSBox, NSBoxType, NSControl, NSEvent, NSEventMask, NSFloatingWindowLevel,
    NSFont, NSMenu, NSMenuItem, NSPanel, NSTextField, NSView, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString, ns_string};

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
    // Keyboard-driven row selection: None = nothing selected (search field focus)
    static SELECTED_ROW: Cell<Option<usize>> = const { Cell::new(None) };
    // True while a context menu (rightMouseDown:) is running its own event loop.
    // The keyboard monitor must pass all events through during this window so that
    // NSMenu can receive arrow-key navigation and Return — otherwise the monitor
    // would consume those keys and prematurely close the popup.
    static CONTEXT_MENU_OPEN: Cell<bool> = const { Cell::new(false) };
    // Tag of the last highlighted context-menu item. Used to determine which
    // item was selected after popUpMenuPositioningItem: returns, bypassing
    // target-action dispatch (which doesn't work during modal sessions).
    static CONTEXT_HIGHLIGHTED_TAG: Cell<isize> = const { Cell::new(-1) };
}

/// Returns the string with the last character removed, respecting multi-byte
/// UTF-8 boundaries. Returns an empty string if the input is empty or single-char.
fn backspace_str(s: &str) -> &str {
    let end = s.char_indices().next_back().map_or(0, |(i, _)| i);
    &s[..end]
}

// ── Dismiss helper ───────────────────────────────────────────────────────────

/// Stop the running modal session. Safe to call from any ObjC callback on the
/// main thread (delegate, row click, keyboard monitor, …).
fn stop_modal() {
    unsafe {
        let mtm = MainThreadMarker::new_unchecked();
        NSApplication::sharedApplication(mtm).stopModal();
    }
}

// ── KeyablePanel — borderless NSPanel that accepts key-window status ─────────
//
// A standard borderless NSPanel returns NO from canBecomeKeyWindow, which
// prevents the field editor from activating inside the search NSTextField.
// Overriding canBecomeKeyWindow allows typed characters (which pass through the
// local NSEvent monitor) to be dispatched to the first responder correctly.

define_class!(
    #[unsafe(super(NSPanel))]
    #[name = "KeyablePanel"]
    pub struct KeyablePanel;

    impl KeyablePanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }
    }
);

// ── Row highlight helper ──────────────────────────────────────────────────────

unsafe extern "C" {
    fn CGColorCreateGenericRGB(r: f64, g: f64, b: f64, a: f64) -> *mut std::ffi::c_void;
    fn CGColorRelease(color: *mut std::ffi::c_void);
}

/// Wrapper for CGColorRef that provides the correct ObjC type encoding
/// (`^{CGColor=}` instead of `^v`), required by `msg_send!` for
/// `CALayer.setBackgroundColor:`.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CGColorPtr(*mut std::ffi::c_void);

unsafe impl Encode for CGColorPtr {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

fn set_row_highlight(view: &NSView, highlighted: bool) {
    unsafe {
        view.setWantsLayer(true);
        let layer: Retained<NSObject> = msg_send![view, layer];
        if highlighted {
            let cg = CGColorCreateGenericRGB(0.2, 0.4, 0.8, 0.3);
            let cg_ptr = CGColorPtr(cg);
            let _: () = msg_send![&*layer, setBackgroundColor: cg_ptr];
            CGColorRelease(cg);
        } else {
            let null_cg = CGColorPtr(std::ptr::null_mut());
            let _: () = msg_send![&*layer, setBackgroundColor: null_cg];
        }
    }
}

// ── PopupRowView — one row in the history or pinned list ─────────────────────

define_class!(
    #[unsafe(super(NSControl))]
    #[name = "PopupRowView"]
    pub struct PopupRowView;

    impl PopupRowView {
        // Override hitTest: so mouse events always target the row view itself,
        // not the NSTextField label subview (which would silently consume them).
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            let frame: NSRect = unsafe { msg_send![self, frame] };
            if point.x >= frame.origin.x
                && point.x <= frame.origin.x + frame.size.width
                && point.y >= frame.origin.y
                && point.y <= frame.origin.y + frame.size.height
            {
                self as *const PopupRowView as *const NSControl as *const NSView as *mut NSView
            } else {
                std::ptr::null_mut()
            }
        }

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            // Remove existing tracking areas
            let existing: Retained<NSObject> = unsafe { msg_send![self, trackingAreas] };
            let count: usize = unsafe { msg_send![&*existing, count] };
            for i in 0..count {
                let area: Retained<NSObject> = unsafe { msg_send![&*existing, objectAtIndex: i] };
                let _: () = unsafe { msg_send![self, removeTrackingArea: &*area] };
            }
            // Add a new tracking area for the full bounds
            let bounds: NSRect = unsafe { msg_send![self, bounds] };
            let ta_cls = objc2::runtime::AnyClass::get(c"NSTrackingArea").unwrap();
            // NSTrackingMouseEnteredAndExited | NSTrackingActiveAlways
            let opts: usize = 0x01 | 0x80;
            let ta: Retained<NSObject> = unsafe {
                msg_send![
                    msg_send![ta_cls, alloc],
                    initWithRect: bounds,
                    options: opts,
                    owner: self,
                    userInfo: std::ptr::null::<NSObject>()
                ]
            };
            let _: () = unsafe { msg_send![self, addTrackingArea: &*ta] };
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            set_row_highlight(self, true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            set_row_highlight(self, false);
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Control+click → treat as right-click (context menu).
            // AppleScript `click at {x,y} using {control down}` sends a left-click
            // with the control modifier rather than a native rightMouseDown event.
            let flags: usize = unsafe { msg_send![event, modifierFlags] };
            if flags & 0x40000 != 0 {
                // NSControlKeyMask — dispatch via ObjC so the selector is correct
                let _: () = unsafe { msg_send![self, rightMouseDown: event] };
                return;
            }
            // tag encodes: pinned flag (bit 16) | index (bits 0-15)
            let tag: isize = unsafe { msg_send![self, tag] };
            let is_pinned = (tag >> 16) & 1 == 1;
            let index = (tag & 0xFFFF) as usize;
            POPUP_ACTION.with(|c| c.set(Some(PopupAction::Paste { pinned: is_pinned, index })));
            stop_modal();
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            crate::log::log_verbose("rightMouseDown: entered");
            let tag: isize = unsafe { msg_send![self, tag] };
            let is_pinned = (tag >> 16) & 1 == 1;
            let index = (tag & 0xFFFF) as usize;
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            // Create delegate that serves double duty:
            // 1. NSMenu delegate — tracks highlighted item via willHighlightItem:
            // 2. Action target — provides contextAction: + validateMenuItem:
            //    so items stay enabled and clickable during the modal session
            let delegate: Retained<ContextMenuDelegate> =
                unsafe { msg_send![ContextMenuDelegate::alloc(), init] };
            let menu = build_context_menu(is_pinned, index, &delegate, mtm);
            unsafe {
                let _: () = msg_send![&*menu, setDelegate: &*delegate];
            }
            CONTEXT_HIGHLIGHTED_TAG.with(|c| c.set(-1));

            CONTEXT_MENU_OPEN.with(|c| c.set(true));

            // Show the context menu. popUpMenuPositioningItem: blocks until
            // the user selects an item or dismisses the menu.
            // Convert window coordinates to the row view's local coordinate
            // system and pass `self` as the inView parameter.
            let loc_in_window = event.locationInWindow();
            let loc_in_self: NSPoint = unsafe {
                msg_send![
                    self,
                    convertPoint: loc_in_window,
                    fromView: std::ptr::null::<NSView>()
                ]
            };
            let selected: bool = unsafe {
                msg_send![
                    &*menu,
                    popUpMenuPositioningItem: std::ptr::null::<NSMenuItem>(),
                    atLocation: loc_in_self,
                    inView: self as *const PopupRowView as *const NSView
                ]
            };

            CONTEXT_MENU_OPEN.with(|c| c.set(false));

            // Manually dispatch the action based on the highlighted item tag.
            // This bypasses NSApp.sendAction: which blocks during modal sessions.
            let htag = CONTEXT_HIGHLIGHTED_TAG.with(|c| c.get());
            crate::log::log_verbose(&format!(
                "rightMouseDown: selected={}, highlighted_tag={}",
                selected, htag
            ));

            if selected && htag >= 0 {
                let (kind, idx) = ctx_decode(htag);
                let action = match kind {
                    CTX_PIN => Some(PopupAction::Pin { history_index: idx }),
                    CTX_UNPIN => Some(PopupAction::Unpin { pinned_index: idx }),
                    CTX_DEL_HISTORY => Some(PopupAction::DeleteHistory { index: idx }),
                    CTX_DEL_PINNED => Some(PopupAction::DeletePinned { index: idx }),
                    _ => None,
                };
                if let Some(a) = action {
                    crate::log::log_verbose(&format!(
                        "rightMouseDown: dispatching {:?}",
                        a
                    ));
                    POPUP_ACTION.with(|c| c.set(Some(a)));
                    stop_modal();
                }
            }

            let _ = delegate;
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: &NSEvent) -> bool {
            true
        }
    }
);

// ── Context menu delegate — tracks highlighted item for manual dispatch ───────
//
// During `runModalForWindow:`, NSApp.sendAction:to:from: refuses to deliver
// target-action messages to objects outside the modal window's view hierarchy.
// Instead of fighting that, we bypass target-action entirely: the delegate
// records the highlighted item's tag, and after `popUpMenuPositioningItem:`
// returns (with selected=true), the caller reads the tag and dispatches
// the PopupAction directly.

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ContextMenuDelegate"]
    pub struct ContextMenuDelegate;

    impl ContextMenuDelegate {
        #[unsafe(method(menu:willHighlightItem:))]
        fn menu_will_highlight_item(&self, _menu: &NSMenu, item: *const NSMenuItem) {
            if !item.is_null() {
                let tag = unsafe { (*item).tag() };
                CONTEXT_HIGHLIGHTED_TAG.with(|c| c.set(tag));
            }
        }

        /// Dummy action target — NSMenu requires an action selector + target for
        /// items to be considered interactive. The actual dispatch happens after
        /// `popUpMenuPositioningItem:` returns, reading `CONTEXT_HIGHLIGHTED_TAG`.
        #[unsafe(method(contextAction:))]
        fn context_action(&self, _sender: &NSMenuItem) {
            // Intentionally empty — dispatch is handled manually by rightMouseDown.
        }

        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, _item: &NSMenuItem) -> bool {
            true
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
            // Don't dismiss the popup while a context menu is open —
            // the context menu steals key status temporarily.
            if !CONTEXT_MENU_OPEN.with(|c| c.get()) {
                stop_modal();
            }
        }
    }
);

// ── Helpers ───────────────────────────────────────────────────────────────────

// Context menu tag encoding:
// bits 0-15  = item index
// bits 16-17 = action kind: 0=pin, 1=unpin, 2=delete-history, 3=delete-pinned
const CTX_PIN: isize = 0;
const CTX_UNPIN: isize = 1;
const CTX_DEL_HISTORY: isize = 2;
const CTX_DEL_PINNED: isize = 3;

fn ctx_tag(kind: isize, index: usize) -> isize {
    (kind << 16) | (index as isize & 0xFFFF)
}

fn ctx_decode(tag: isize) -> (isize, usize) {
    let kind = (tag >> 16) & 0x3;
    let index = (tag & 0xFFFF) as usize;
    (kind, index)
}

fn add_context_item(
    menu: &NSMenu,
    title: &str,
    tag: isize,
    delegate: &ContextMenuDelegate,
    mtm: MainThreadMarker,
) {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(title),
            Some(sel!(contextAction:)),
            ns_string!(""),
        )
    };
    item.setTag(tag);
    item.setEnabled(true);
    unsafe { item.setTarget(Some(delegate)) };
    menu.addItem(&item);
}

fn build_context_menu(
    is_pinned: bool,
    index: usize,
    delegate: &ContextMenuDelegate,
    mtm: MainThreadMarker,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    if is_pinned {
        add_context_item(&menu, "Unpin", ctx_tag(CTX_UNPIN, index), delegate, mtm);
        add_context_item(
            &menu,
            "Delete",
            ctx_tag(CTX_DEL_PINNED, index),
            delegate,
            mtm,
        );
    } else {
        add_context_item(&menu, "Pin", ctx_tag(CTX_PIN, index), delegate, mtm);
        add_context_item(
            &menu,
            "Delete from history",
            ctx_tag(CTX_DEL_HISTORY, index),
            delegate,
            mtm,
        );
    }

    menu
}

/// Shows a popup panel at the given position (or at the cursor if `None`)
/// with clipboard items.
/// Items: `(index, label, tooltip)` for history items.
/// Pinned: `(index, label, tooltip)` for pinned shelf items.
/// Returns `Some(PopupAction)` or `None` if dismissed without selection.
pub fn show_popup(
    items: &[(usize, String, String)],
    pinned: &[(usize, String, String)],
    mtm: MainThreadMarker,
    position: Option<NSPoint>,
) -> (Option<PopupAction>, NSPoint) {
    POPUP_ACTION.with(|c| c.set(None));
    SELECTED_ROW.with(|c| c.set(None));

    // Save frontmost app for focus restore on paste
    let workspace = NSWorkspace::sharedWorkspace();
    let frontmost = workspace.frontmostApplication();

    let app = NSApplication::sharedApplication(mtm);

    // Panel dimensions
    const W: f64 = 320.0;
    const ROW_H: f64 = 28.0;
    const SEARCH_H: f64 = 36.0;
    const SEP_H: f64 = 1.0;
    const MIN_H: f64 = 28.0; // minimum height for empty state label

    let history_h = items.len() as f64 * ROW_H;
    let pinned_h = if pinned.is_empty() {
        0.0
    } else {
        SEP_H + pinned.len() as f64 * ROW_H
    };
    let content_h = (history_h + pinned_h).max(MIN_H);
    let total_h = SEARCH_H + content_h;

    let location = position.unwrap_or_else(NSEvent::mouseLocation);
    let frame = NSRect::new(
        NSPoint::new(location.x, location.y - total_h),
        NSSize::new(W, total_h),
    );

    // Create borderless floating panel (KeyablePanel overrides canBecomeKeyWindow
    // so the search field's field editor can activate and accept typed characters).
    let panel: Retained<KeyablePanel> = unsafe {
        msg_send![
            mtm.alloc::<KeyablePanel>(),
            initWithContentRect: frame,
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    panel.setLevel(NSFloatingWindowLevel);

    // Attach dismiss delegate
    let delegate: Retained<PopupWindowDelegate> =
        unsafe { msg_send![PopupWindowDelegate::alloc(), init] };
    unsafe {
        let (): () = msg_send![&*panel, setDelegate: &*delegate];
    }

    let content = panel.contentView().unwrap();

    // ── Search field at top ───────────────────────────────────────────
    let search_frame = NSRect::new(
        NSPoint::new(4.0, total_h - SEARCH_H + 4.0),
        NSSize::new(W - 8.0, SEARCH_H - 8.0),
    );
    let search_field = NSTextField::initWithFrame(mtm.alloc(), search_frame);
    search_field.setPlaceholderString(Some(ns_string!("Type to filter...")));
    content.addSubview(&search_field);

    // ── History rows ──────────────────────────────────────────────────
    let mut row_y = total_h - SEARCH_H - ROW_H;

    // Collect (row_view_ptr, searchable_text) for search filtering.
    // row_view_ptr is a raw pointer to the PopupRowView — kept alive by the
    // content view's subview list for the lifetime of the panel.
    let mut row_info: Vec<(*mut PopupRowView, String)> = Vec::new();

    if items.is_empty() && pinned.is_empty() {
        // Empty state
        let empty = NSTextField::labelWithString(ns_string!("No clipboard history yet"), mtm);
        empty.setFrame(NSRect::new(NSPoint::new(0.0, row_y), NSSize::new(W, ROW_H)));
        unsafe {
            let _: () = msg_send![&*empty, setAlignment: 1_isize];
        } // NSTextAlignmentCenter
        content.addSubview(&empty);
    } else {
        for (idx, label, tooltip) in items {
            let row_frame = NSRect::new(NSPoint::new(0.0, row_y), NSSize::new(W, ROW_H));
            let row: Retained<PopupRowView> =
                unsafe { msg_send![mtm.alloc::<PopupRowView>(), initWithFrame: row_frame] };
            unsafe {
                let _: () = msg_send![&*row, setTag: *idx as isize];
            }
            row.setToolTip(Some(&NSString::from_str(tooltip)));

            let label_view = NSTextField::labelWithString(&NSString::from_str(label), mtm);
            label_view.setFrame(NSRect::new(
                NSPoint::new(8.0, 5.0),
                NSSize::new(W - 16.0, ROW_H - 10.0),
            ));
            row.addSubview(&label_view);
            content.addSubview(&row);
            row_info.push((
                &*row as *const PopupRowView as *mut PopupRowView,
                tooltip.clone(),
            ));
            row_y -= ROW_H;
        }

        // ── Pinned shelf ──────────────────────────────────────────────
        if !pinned.is_empty() {
            // Thin separator line between history and pinned sections.
            // Placed at row_y + ROW_H - SEP_H so it sits in the 1px gap below
            // the last history row rather than overlapping with it.
            let sep_frame = NSRect::new(
                NSPoint::new(0.0, row_y + ROW_H - SEP_H),
                NSSize::new(W, SEP_H),
            );
            let sep = NSBox::initWithFrame(mtm.alloc(), sep_frame);
            sep.setBoxType(NSBoxType::Separator);
            content.addSubview(&sep);
            row_y -= 1.0;

            for (idx, label, tooltip) in pinned {
                let row_frame = NSRect::new(NSPoint::new(0.0, row_y), NSSize::new(W, ROW_H));
                let row: Retained<PopupRowView> =
                    unsafe { msg_send![mtm.alloc::<PopupRowView>(), initWithFrame: row_frame] };
                unsafe {
                    let _: () = msg_send![&*row, setTag: ((1_isize << 16) | *idx as isize)];
                }
                row.setToolTip(Some(&NSString::from_str(tooltip)));

                // Pin icon — small fixed-width label on the left
                let icon_label = NSTextField::labelWithString(ns_string!("📌"), mtm);
                let icon_size: f64 = 14.0;
                icon_label.setFrame(NSRect::new(
                    NSPoint::new(6.0, (ROW_H - icon_size) / 2.0),
                    NSSize::new(icon_size, icon_size),
                ));
                let small = NSFont::systemFontOfSize(10.0);
                icon_label.setFont(Some(&small));
                row.addSubview(&icon_label);

                // Text label — offset to the right of the icon
                let text_x: f64 = 22.0;
                let text_label = NSTextField::labelWithString(&NSString::from_str(label), mtm);
                text_label.setFrame(NSRect::new(
                    NSPoint::new(text_x, 5.0),
                    NSSize::new(W - text_x - 8.0, ROW_H - 10.0),
                ));
                row.addSubview(&text_label);
                content.addSubview(&row);
                row_info.push((
                    &*row as *const PopupRowView as *mut PopupRowView,
                    tooltip.clone(),
                ));
                row_y -= ROW_H;
            }
        }
    }

    // ── Row highlight helper ────────────────────────────────────────────
    // Updates the visual highlight on row views to match SELECTED_ROW.
    let update_highlight = {
        let row_info = row_info.clone();
        move |new_sel: Option<usize>| {
            for (i, (ptr, _)) in row_info.iter().enumerate() {
                unsafe {
                    let view = &*(*ptr as *const NSView);
                    set_row_highlight(view, Some(i) == new_sel);
                }
            }
            if let Some(n) = new_sel {
                crate::log::log_verbose(&format!("highlight: row {}", n));
            }
        }
    };

    // ── Search filtering helper ─────────────────────────────────────────
    // Apply search filter: hide rows whose full text doesn't contain the query
    // (case-insensitive). Resets SELECTED_ROW to None when filter changes.
    let apply_filter = {
        let row_info = row_info.clone();
        let update_highlight = update_highlight.clone();
        move |query: &str| {
            let q = query.to_lowercase();
            for (ptr, text) in &row_info {
                let visible = q.is_empty() || text.to_lowercase().contains(&q);
                unsafe {
                    let view = &*(*ptr as *const PopupRowView as *const NSView);
                    view.setHidden(!visible);
                }
            }
            SELECTED_ROW.with(|c| c.set(None));
            update_highlight(None);
        }
    };

    // ── Keyboard event monitor (Esc, arrows, Enter) ───────────────────
    let search_ptr = &*search_field as *const NSTextField as usize; // pass as usize for Send
    let have_items = !items.is_empty() || !pinned.is_empty();
    let history_count = items.len();
    let total_count = history_count + pinned.len();

    let kb_block = StackBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // While a context menu is running its own nested event loop, let all key
        // events pass through so NSMenu can handle arrow navigation and Return.
        if CONTEXT_MENU_OPEN.with(|c| c.get()) {
            return event.as_ptr();
        }
        let key_code = unsafe { event.as_ref().keyCode() };
        match key_code {
            53 => {
                // Esc — clear search query or dismiss
                let search_ref = unsafe { &*(search_ptr as *const NSTextField) };
                let query = search_ref.stringValue();
                if query.is_empty() {
                    stop_modal();
                } else {
                    search_ref.setStringValue(ns_string!(""));
                    apply_filter("");
                }
                std::ptr::null_mut()
            }
            125 if have_items => {
                // Down arrow — advance selection, skipping hidden rows
                let cur = SELECTED_ROW.with(|c| c.get());
                let start = match cur {
                    None => 0,
                    Some(i) => i + 1,
                };
                let next = (start..total_count).find(|&i| {
                    row_info.get(i).is_some_and(|(ptr, _)| unsafe {
                        !(&*(*ptr as *const PopupRowView as *const NSView)).isHidden()
                    })
                });
                if let Some(n) = next {
                    SELECTED_ROW.with(|c| c.set(Some(n)));
                    update_highlight(Some(n));
                }
                std::ptr::null_mut()
            }
            126 if have_items => {
                // Up arrow — retreat selection, skipping hidden rows
                let cur = SELECTED_ROW.with(|c| c.get());
                let prev = match cur {
                    None | Some(0) => None,
                    Some(i) => (0..i).rfind(|&j| {
                        row_info.get(j).is_some_and(|(ptr, _)| unsafe {
                            !(&*(*ptr as *const PopupRowView as *const NSView)).isHidden()
                        })
                    }),
                };
                SELECTED_ROW.with(|c| c.set(prev));
                update_highlight(prev);
                std::ptr::null_mut()
            }
            36 | 76 if have_items => {
                // Enter — paste the keyboard-selected row, or first visible row
                if POPUP_ACTION.with(|c| c.get()).is_none() {
                    let sel = SELECTED_ROW.with(|c| c.get());
                    let index = sel.or_else(|| {
                        // Find first visible row
                        (0..total_count).find(|&i| {
                            row_info.get(i).is_some_and(|(ptr, _)| unsafe {
                                !(&*(*ptr as *const PopupRowView as *const NSView)).isHidden()
                            })
                        })
                    });
                    if let Some(index) = index {
                        if index < history_count {
                            POPUP_ACTION.with(|c| {
                                c.set(Some(PopupAction::Paste {
                                    pinned: false,
                                    index,
                                }))
                            });
                        } else {
                            POPUP_ACTION.with(|c| {
                                c.set(Some(PopupAction::Paste {
                                    pinned: true,
                                    index: index - history_count,
                                }))
                            });
                        }
                        stop_modal();
                    }
                }
                std::ptr::null_mut()
            }
            35 if have_items && SELECTED_ROW.with(|c| c.get()).is_some() => {
                // 'p' key — pin selected history row / unpin selected pinned row
                // Guard: only when a row is selected; otherwise fall through to
                // the default arm so 'p' reaches the search field.
                let index = SELECTED_ROW.with(|c| c.get()).unwrap();
                if index < history_count {
                    POPUP_ACTION.with(|c| {
                        c.set(Some(PopupAction::Pin {
                            history_index: index,
                        }))
                    });
                } else {
                    POPUP_ACTION.with(|c| {
                        c.set(Some(PopupAction::Unpin {
                            pinned_index: index - history_count,
                        }))
                    });
                }
                stop_modal();
                std::ptr::null_mut()
            }
            51 => {
                // Delete/Backspace — pass through to search field, then filter
                let search_ref = unsafe { &*(search_ptr as *const NSTextField) };
                let cur = search_ref.stringValue().to_string();
                if !cur.is_empty() {
                    apply_filter(backspace_str(&cur));
                }
                event.as_ptr()
            }
            _ => {
                // For printable characters, predict the resulting search text and filter
                let chars = unsafe { event.as_ref().characters() };
                if let Some(ch) = chars.and_then(|s| s.to_string().chars().next())
                    && !ch.is_control()
                {
                    let search_ref = unsafe { &*(search_ptr as *const NSTextField) };
                    let mut query = search_ref.stringValue().to_string();
                    query.push(ch);
                    apply_filter(&query);
                }
                event.as_ptr()
            }
        }
    });

    let kb_monitor: Retained<NSObject> = unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        msg_send![
            cls,
            addLocalMonitorForEventsMatchingMask: NSEventMask::KeyDown,
            handler: &kb_block
        ]
    };

    // ── Show panel and run modal session ──────────────────────────────
    // Switch to Regular policy and explicitly activate so that the process
    // becomes the system-frontmost application. This is required for
    // keyboard events (from System Events, CGEvent, or direct input) to be
    // delivered to the app's event queue and intercepted by the local
    // NSEvent monitor. Without activateIgnoringOtherApps:, the previously
    // active app remains frontmost and swallows all keystrokes.
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    unsafe {
        let _: () = msg_send![&*app, activateIgnoringOtherApps: true];
    }
    panel.makeKeyAndOrderFront(None);
    unsafe {
        let _: bool = msg_send![&*panel, makeFirstResponder: &*search_field];
    }
    crate::log::log("popup: modal start");
    unsafe {
        let _: isize = msg_send![&*app, runModalForWindow: &*panel];
    }
    crate::log::log(&format!(
        "popup: modal end, action={:?}",
        POPUP_ACTION.with(|c| c.get())
    ));

    // ── Cleanup ───────────────────────────────────────────────────────
    unsafe {
        let cls = objc2::runtime::AnyClass::get(c"NSEvent").unwrap();
        let (): () = msg_send![cls, removeMonitor: &*kb_monitor];
    }

    panel.orderOut(None);

    let result = POPUP_ACTION.with(|c| c.get());

    // Restore focus to previous app (only for Paste actions — mutating
    // actions like Pin/Delete will reopen the popup immediately).
    let is_terminal = matches!(result, None | Some(PopupAction::Paste { .. }));
    if is_terminal {
        if let Some(prev) = &frontmost {
            #[allow(deprecated)]
            prev.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
        // Restore accessory policy (hides Dock icon).
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }

    (result, location)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backspace_ascii() {
        assert_eq!(backspace_str("hello"), "hell");
    }

    #[test]
    fn backspace_empty() {
        assert_eq!(backspace_str(""), "");
    }

    #[test]
    fn backspace_single_char() {
        assert_eq!(backspace_str("a"), "");
    }

    #[test]
    fn backspace_hebrew() {
        // Hebrew chars are multi-byte UTF-8 (2 bytes each)
        assert_eq!(backspace_str("שלום"), "שלו");
    }

    #[test]
    fn backspace_emoji() {
        // Emoji are 4-byte UTF-8
        assert_eq!(backspace_str("hi🙂"), "hi");
    }

    #[test]
    fn backspace_mixed_scripts() {
        assert_eq!(backspace_str("abcדהו"), "abcדה");
    }
}
