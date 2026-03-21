use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, define_class, msg_send, sel};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem};
use objc2_foundation::{MainThreadMarker, NSString, ns_string};

use crate::settings::{self, SettingsTarget};

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ClearHistoryTarget"]
    pub struct ClearHistoryTarget;

    impl ClearHistoryTarget {
        #[unsafe(method(clearHistory:))]
        fn clear_history_action(&self, _sender: &NSObject) {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            settings::confirm_and_clear_history(mtm);
        }
    }
);

impl ClearHistoryTarget {
    pub fn new() -> Retained<Self> {
        unsafe { msg_send![Self::alloc(), init] }
    }
}

/// NSVariableStatusItemLength
const VARIABLE_LENGTH: f64 = -1.0;

pub struct Tray {
    status_item: Retained<NSStatusItem>,
    settings_target: Retained<SettingsTarget>,
    clear_history_target: Retained<ClearHistoryTarget>,
    mtm: MainThreadMarker,
}

impl Tray {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let status_bar = NSStatusBar::systemStatusBar();
        let status_item = status_bar.statusItemWithLength(VARIABLE_LENGTH);

        // Set SF Symbol icon (monochrome template, matches system style)
        if let Some(button) = status_item.button(mtm)
            && let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                ns_string!("doc.on.clipboard"),
                Some(ns_string!("Cliphop")),
            )
        {
            image.setTemplate(true);
            button.setImage(Some(&image));
        }

        let settings_target = SettingsTarget::new();
        let clear_history_target = ClearHistoryTarget::new();

        // Build initial empty menu
        status_item.setMenu(Some(&Self::build_menu(
            &[],
            &settings_target,
            &clear_history_target,
            mtm,
        )));

        Self {
            status_item,
            settings_target,
            clear_history_target,
            mtm,
        }
    }

    /// Rebuilds the tray menu with current clipboard items.
    pub fn update_items(&self, items: &[(String, String)]) {
        self.status_item.setMenu(Some(&Self::build_menu(
            items,
            &self.settings_target,
            &self.clear_history_target,
            self.mtm,
        )));
    }

    fn build_menu(
        items: &[(String, String)],
        settings_target: &SettingsTarget,
        clear_history_target: &ClearHistoryTarget,
        mtm: MainThreadMarker,
    ) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);

        if items.is_empty() {
            let empty_item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    mtm.alloc(),
                    &NSString::from_str("No items yet"),
                    None,
                    ns_string!(""),
                )
            };
            empty_item.setEnabled(false);
            menu.addItem(&empty_item);
        } else {
            for (label, tooltip) in items {
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        mtm.alloc(),
                        &NSString::from_str(label),
                        None,
                        ns_string!(""),
                    )
                };
                item.setEnabled(false);
                item.setToolTip(Some(&NSString::from_str(tooltip)));
                menu.addItem(&item);
            }
        }

        // Separator + Clear History + separator + Settings + Quit
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let clear_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Clear History"),
                Some(objc2::sel!(clearHistory:)),
                ns_string!(""),
            )
        };
        unsafe { clear_item.setTarget(Some(clear_history_target)) };
        menu.addItem(&clear_item);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let settings_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Settings..."),
                Some(sel!(showSettings:)),
                ns_string!(","),
            )
        };
        unsafe { settings_item.setTarget(Some(settings_target)) };
        menu.addItem(&settings_item);

        let quit_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                mtm.alloc(),
                &NSString::from_str("Quit Cliphop"),
                Some(sel!(terminate:)),
                ns_string!("q"),
            )
        };
        menu.addItem(&quit_item);

        menu
    }
}
