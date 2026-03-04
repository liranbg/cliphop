mod clipboard;
mod hotkey;
mod log;
mod macos;
mod paste;
mod popup;
mod settings;
mod tray;

use clipboard::ClipboardHistory;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
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
    macos::check_accessibility();

    let event_loop = EventLoop::new();

    // Safety: tao's EventLoop::new() initializes NSApplication on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    let hotkey = hotkey::Hotkey::new();
    log::log("Hotkey registered (Option+V)");

    let mut history = ClipboardHistory::new();
    log::log("ClipboardHistory initialized");

    let tray = tray::Tray::new(mtm);
    log::log("Tray created");

    let hotkey_rx = GlobalHotKeyEvent::receiver();

    log::log("Entering event loop");
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + POLL_INTERVAL);

        match event {
            Event::NewEvents(StartCause::Init) => {
                history.poll();
                update_tray(&tray, &history);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if history.poll() {
                    log::log(&format!(
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
            log::log(&format!(
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
                log::log(&format!("Showing popup with {} items", items.len()));
                match popup::show_popup(&items, mtm) {
                    Some(index) => {
                        log::log(&format!("Popup returned: index={}", index));
                        match history.select(index) {
                            Some(text) => {
                                log::log(&format!(
                                    "Clipboard set to item {}: \"{}\"",
                                    index,
                                    ClipboardHistory::display_label(&text)
                                ));
                                log::log("Calling simulate_paste()");
                                paste::simulate_paste();
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
                        log::log("Popup dismissed without selection");
                    }
                }
            }
        }
    });
}
