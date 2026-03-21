mod hotkey;
mod macos;
mod paste;
mod popup;
mod settings;
mod tray;

use cliphop::clipboard::{self, ClipboardHistory};
use cliphop::config;
use cliphop::history::{self, HistoryEntry};
use cliphop::log;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::MainThreadMarker;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

fn update_tray(tray: &tray::Tray, history: &ClipboardHistory) {
    let items: Vec<(String, String)> = history
        .items()
        .iter()
        .enumerate()
        .map(|(i, text)| {
            (
                format!("{}: {}", i, ClipboardHistory::display_label(text)),
                ClipboardHistory::display_tooltip(text),
            )
        })
        .collect();
    tray.update_items(&items);
}

fn main() {
    log::init();
    log::log("Cliphop starting up...");

    let cfg = config::load();
    clipboard::set_max_history(cfg.max_history);
    log::set_verbose(cfg.verbose_logging);

    if !macos::is_accessibility_trusted() {
        log::log(
            "WARNING: Accessibility NOT granted — paste will not work! \
             If you recently rebuilt, remove and re-add Cliphop in \
             System Settings > Privacy & Security > Accessibility.",
        );
    }

    let event_loop = EventLoop::new();

    // Safety: tao's EventLoop::new() initializes NSApplication on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    let hotkey = hotkey::Hotkey::new();
    log::log("Hotkey registered (Option+V)");

    // Load persisted history into memory
    let loaded = history::load();
    let mut history = ClipboardHistory::new();
    let persisted_texts: Vec<String> = loaded
        .into_iter()
        .filter_map(|e| match e {
            HistoryEntry::Text(s) => Some(s),
        })
        .collect();
    history.load_items(persisted_texts);
    log::log(&format!(
        "Loaded {} items from history",
        history.items().len()
    ));

    let tray = tray::Tray::new(mtm);
    log::log("Tray created");

    // Register clear callback (called from both Settings dialog and tray menu).
    // Safety: closure is only ever invoked on the main thread (ObjC callback).
    {
        let history_ptr = &mut history as *mut ClipboardHistory;
        let tray_ptr = &tray as *const tray::Tray;
        settings::set_clear_fn(move || unsafe {
            (*history_ptr).clear();
            history::clear();
            // Rebuild tray immediately to show "No items yet"
            (*tray_ptr).update_items(&[]);
            log::log("History cleared");
        });
    }

    let hotkey_rx = GlobalHotKeyEvent::receiver();

    log::log("Entering event loop");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + POLL_INTERVAL);

        match event {
            Event::NewEvents(StartCause::Init) => {
                let _ = history.poll(); // discard on init; tray rebuilt unconditionally below
                update_tray(&tray, &history);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let prev_len = history.items().len();
                let changed = history.poll();
                let should_save = changed.is_some() || history.items().len() < prev_len;

                if should_save {
                    let items: Vec<String> = history.items().iter().cloned().collect();
                    history::save_all(&items);
                    log::log_verbose(&format!(
                        "Clipboard changed, history now has {} items",
                        history.items().len()
                    ));
                    update_tray(&tray, &history);
                }
            }
            _ => {}
        }

        // Check for hotkey press
        if let Ok(event) = hotkey_rx.try_recv()
            && event.id == hotkey.hotkey.id()
            && event.state == HotKeyState::Pressed
        {
            log::log_verbose(&format!(
                "Hotkey pressed, {} items in history",
                history.items().len()
            ));

            // Build the list of items for the popup: (index, label, tooltip)
            let items: Vec<(usize, String, String)> = history
                .items()
                .iter()
                .enumerate()
                .map(|(i, text)| {
                    (
                        i,
                        ClipboardHistory::display_label(text),
                        ClipboardHistory::display_tooltip(text),
                    )
                })
                .collect();

            if items.is_empty() {
                log::log("No items to show, skipping popup");
            } else {
                // Capture the frontmost app before the popup grabs focus,
                // so simulate_paste knows which app to wait for.
                let target_pid = NSWorkspace::sharedWorkspace()
                    .frontmostApplication()
                    .map(|a| a.processIdentifier())
                    .unwrap_or(-1);

                log::log_verbose(&format!("Showing popup with {} items", items.len()));
                match popup::show_popup(&items, mtm) {
                    Some(index) => {
                        log::log_verbose(&format!("Popup returned: index={}", index));
                        match history.select(index) {
                            Some(..) => {
                                log::log_verbose(&format!("Clipboard set to item {}", index));
                                log::log_verbose("Calling simulate_paste()");
                                paste::simulate_paste(target_pid);
                            }
                            None => {
                                log::log(&format!(
                                    "ERROR: history.select({}) returned None",
                                    index
                                ));
                            }
                        }
                    }
                    None => {
                        log::log_verbose("Popup dismissed without selection");
                    }
                }
            }
        }
    });
}
