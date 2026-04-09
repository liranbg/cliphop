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
use objc2_foundation::{MainThreadMarker, NSPoint};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

fn update_tray(tray: &tray::Tray, history: &ClipboardHistory) {
    let items: Vec<(String, String)> = history
        .items()
        .iter()
        .map(|text| {
            (
                ClipboardHistory::display_label(text),
                ClipboardHistory::display_tooltip(text),
            )
        })
        .collect();
    let pinned: Vec<(String, String)> = history
        .pinned_items()
        .iter()
        .map(|text| {
            (
                ClipboardHistory::display_label(text),
                ClipboardHistory::display_tooltip(text),
            )
        })
        .collect();
    tray.update_items(&items, &pinned);
}

fn save_history(history: &ClipboardHistory) {
    let items: Vec<String> = history.items().iter().cloned().collect();
    let pinned: Vec<String> = history.pinned_items().iter().cloned().collect();
    history::save_all(&items, &pinned);
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

    let mut hotkey = hotkey::Hotkey::new();
    // Apply saved hotkey combo (may differ from the default "alt+v")
    if cfg.hotkey != "alt+v"
        && let Err(e) = hotkey.re_register(&cfg.hotkey)
    {
        log::log(&format!(
            "Failed to apply saved hotkey '{}': {}",
            cfg.hotkey, e
        ));
    }
    log::log(&format!("Hotkey registered ({})", cfg.hotkey));

    // Load persisted history into memory
    let loaded = history::load();
    let mut history = ClipboardHistory::new();
    let mut persisted_texts: Vec<String> = Vec::new();
    let mut persisted_pinned: Vec<String> = Vec::new();
    for e in loaded {
        match e {
            HistoryEntry::Text(s) => persisted_texts.push(s),
            HistoryEntry::PinnedText(s) => persisted_pinned.push(s),
        }
    }
    history.load_items(persisted_texts);
    history.load_pinned(persisted_pinned);
    log::log(&format!(
        "Loaded {} history + {} pinned items",
        history.items().len(),
        history.pinned_items().len(),
    ));

    let tray = tray::Tray::new(mtm);
    log::log("Tray created");

    let hotkey_rx = GlobalHotKeyEvent::receiver();

    log::log("Entering event loop");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + POLL_INTERVAL);

        match event {
            Event::NewEvents(StartCause::Init) => {
                {
                    let history_ptr = &mut history as *mut ClipboardHistory;
                    let tray_ptr = &tray as *const tray::Tray;
                    settings::set_clear_fn(move || unsafe {
                        (*history_ptr).clear();
                        save_history(&*history_ptr);
                        update_tray(&*tray_ptr, &*history_ptr);
                        log::log("History cleared");
                    });
                }
                {
                    // Tray item click: tag encodes pinned flag (bit 16) | index (bits 0-15)
                    let history_ptr = &mut history as *mut ClipboardHistory;
                    let tray_ptr = &tray as *const tray::Tray;
                    settings::set_tray_paste_fn(move |tag| {
                        let is_pinned = (tag >> 16) & 1 == 1;
                        let idx = tag & 0xFFFF;
                        let target_pid = NSWorkspace::sharedWorkspace()
                            .frontmostApplication()
                            .map(|a| a.processIdentifier())
                            .unwrap_or(-1);
                        let selected = unsafe {
                            if is_pinned {
                                (*history_ptr).select_pinned(idx)
                            } else {
                                (*history_ptr).select(idx)
                            }
                        };
                        if selected.is_some() {
                            paste::simulate_paste(target_pid);
                            save_history(unsafe { &*history_ptr });
                            update_tray(unsafe { &*tray_ptr }, unsafe { &*history_ptr });
                        }
                    });
                }
                {
                    settings::set_reregister_fn(move |combo: &str| {
                        // We can't hold a mutable ref to hotkey inside the closure here
                        // (the hotkey is owned by this closure scope) — instead we re-register
                        // via a separate channel. hotkey re_register is called from the settings
                        // ObjC callback, which runs on the main thread in the same run-loop tick.
                        // We use a thread-local to communicate the new combo back.
                        PENDING_HOTKEY.with(|p| *p.borrow_mut() = Some(combo.to_string()));
                        Ok(())
                    });
                }
                let _ = history.poll();
                update_tray(&tray, &history);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                let prev_len = history.items().len();
                let changed = history.poll();
                let should_save = changed.is_some() || history.items().len() < prev_len;

                if should_save {
                    save_history(&history);
                    log::log_verbose(&format!(
                        "Clipboard changed, history now has {} items",
                        history.items().len()
                    ));
                    update_tray(&tray, &history);
                }
            }
            _ => {}
        }

        // Apply pending hotkey re-registration (set by settings callback)
        PENDING_HOTKEY.with(|p| {
            if let Some(combo) = p.borrow_mut().take() {
                if let Err(e) = hotkey.re_register(&combo) {
                    log::log(&format!("hotkey re-register failed: {}", e));
                } else {
                    log::log(&format!("Hotkey re-registered: {}", combo));
                }
            }
        });

        // Check for hotkey press
        if let Ok(event) = hotkey_rx.try_recv()
            && event.id == hotkey.hotkey.id()
            && event.state == HotKeyState::Pressed
        {
            log::log_verbose(&format!(
                "Hotkey pressed, {} items in history, {} pinned",
                history.items().len(),
                history.pinned_items().len(),
            ));

            if history.items().is_empty() && history.pinned_items().is_empty() {
                log::log("No items to show, skipping popup");
            } else {
                let target_pid = NSWorkspace::sharedWorkspace()
                    .frontmostApplication()
                    .map(|a| a.processIdentifier())
                    .unwrap_or(-1);

                // The popup stays open for mutating actions (pin, unpin, delete).
                // Each iteration rebuilds items/pinned and re-shows the popup at
                // the same position.  Only Paste and dismiss (None) exit the loop.
                let mut popup_pos: Option<NSPoint> = None;
                loop {
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

                    let pinned: Vec<(usize, String, String)> = history
                        .pinned_items()
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

                    if items.is_empty() && pinned.is_empty() {
                        log::log("All items removed, closing popup");
                        break;
                    }

                    log::log(&format!(
                        "Showing popup: {} history, {} pinned",
                        items.len(),
                        pinned.len()
                    ));
                    let (action, pos) = popup::show_popup(&items, &pinned, mtm, popup_pos);
                    popup_pos = Some(pos);

                    match action {
                        Some(popup::PopupAction::Paste {
                            pinned: false,
                            index,
                        }) => {
                            log::log(&format!("Popup: paste history[{}]", index));
                            if history.select(index).is_some() {
                                paste::simulate_paste(target_pid);
                                save_history(&history);
                                update_tray(&tray, &history);
                            }
                            break;
                        }
                        Some(popup::PopupAction::Paste {
                            pinned: true,
                            index,
                        }) => {
                            log::log_verbose(&format!("Popup: paste pinned[{}]", index));
                            if history.select_pinned(index).is_some() {
                                paste::simulate_paste(target_pid);
                                save_history(&history);
                                update_tray(&tray, &history);
                            }
                            break;
                        }
                        Some(popup::PopupAction::Pin { history_index }) => {
                            log::log_verbose(&format!("Popup: pin history[{}]", history_index));
                            history.pin(history_index);
                            save_history(&history);
                            update_tray(&tray, &history);
                            // Loop: reopen popup with updated content
                        }
                        Some(popup::PopupAction::Unpin { pinned_index }) => {
                            log::log_verbose(&format!("Popup: unpin pinned[{}]", pinned_index));
                            history.unpin(pinned_index);
                            save_history(&history);
                            update_tray(&tray, &history);
                        }
                        Some(popup::PopupAction::DeleteHistory { index }) => {
                            log::log_verbose(&format!("Popup: delete history[{}]", index));
                            history.delete_history(index);
                            save_history(&history);
                            update_tray(&tray, &history);
                        }
                        Some(popup::PopupAction::DeletePinned { index }) => {
                            log::log_verbose(&format!("Popup: delete pinned[{}]", index));
                            history.delete_pinned(index);
                            save_history(&history);
                            update_tray(&tray, &history);
                        }
                        None => {
                            log::log("Popup dismissed without action");
                            break;
                        }
                    }
                }
            }
        }
    });
}

// Thread-local to pass hotkey combo from settings callback back to the event loop
// (avoids needing a mut ref to hotkey inside a closure).
use std::cell::RefCell;
thread_local! {
    static PENDING_HOTKEY: RefCell<Option<String>> = const { RefCell::new(None) };
}
