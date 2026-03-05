use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSEvent, NSMenu, NSMenuItem, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSString};

thread_local! {
    static POPUP_RESULT: Cell<Option<usize>> = const { Cell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "PopupTarget"]
    struct PopupTarget;

    impl PopupTarget {
        #[unsafe(method(itemClicked:))]
        fn item_clicked(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            crate::log::log_verbose(&format!("PopupTarget.itemClicked: tag={}", tag));
            POPUP_RESULT.with(|c| c.set(Some(tag as usize)));
        }
    }
);

impl PopupTarget {
    fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

/// Shows a popup menu at the cursor with the given items.
/// Each item is (index, label, tooltip). Returns the selected index or None.
pub fn show_popup(items: &[(usize, String, String)], mtm: MainThreadMarker) -> Option<usize> {
    POPUP_RESULT.with(|c| c.set(None));

    // Save the currently active app so we can restore focus after selection
    let workspace = NSWorkspace::sharedWorkspace();
    let frontmost = workspace.frontmostApplication();
    if let Some(app) = &frontmost {
        let name = app.localizedName();
        crate::log::log_verbose(&format!(
            "Previous frontmost app: pid={}, name={:?}",
            app.processIdentifier(),
            name.map(|n| n.to_string())
        ));
    } else {
        crate::log::log_verbose("WARNING: No frontmost application found");
    }

    let target = PopupTarget::new();
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);

    for (idx, label, tooltip) in items {
        let title = NSString::from_str(&format!("{}: {}", idx, label));
        let key = NSString::from_str(&idx.to_string());
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &title,
                Some(sel!(itemClicked:)),
                &key,
            )
        };
        item.setTag(*idx as isize);
        item.setToolTip(Some(&NSString::from_str(tooltip)));
        unsafe { item.setTarget(Some(&target)) };
        menu.addItem(&item);
    }

    // Activate the app so the menu can receive focus
    let app = NSApplication::sharedApplication(mtm);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    crate::log::log_verbose("Activated Cliphop app for popup");

    // Show at cursor position (blocks until dismissed)
    let location = NSEvent::mouseLocation();
    crate::log::log_verbose(&format!(
        "Showing NSMenu at ({}, {})",
        location.x, location.y
    ));
    menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

    let result = POPUP_RESULT.with(|c| c.get());
    crate::log::log_verbose(&format!("NSMenu dismissed, result={:?}", result));

    // Restore focus to the previous app so paste targets it
    if result.is_some()
        && let Some(prev) = &frontmost
    {
        #[allow(deprecated)]
        let ok =
            prev.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        crate::log::log_verbose(&format!(
            "Restored focus, activateWithOptions returned {}",
            ok
        ));
    }

    result
}
