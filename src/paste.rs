use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread;
use std::time::Duration;

const KEY_V: CGKeyCode = 0x09; // ANSI_V

/// Simulates Cmd+V via CoreGraphics keyboard events (requires Accessibility permission).
/// `target_pid` is the process that was frontmost before our popup; focus has already been
/// restored to it by `show_popup` before this is called.
pub fn simulate_paste(target_pid: i32) {
    thread::spawn(move || {
        // Give the target app time to finish receiving focus after `activateWithOptions`.
        // NSWorkspace cannot be queried safely from a background thread, so we use a
        // fixed sleep instead of polling frontmostApplication().
        // 100ms is enough for all tested apps (browsers, terminals, editors).
        thread::sleep(Duration::from_millis(100));

        crate::log::log_verbose(&format!(
            "paste thread: posting Cmd+V to pid={} via CoreGraphics",
            target_pid
        ));

        let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
            crate::log::log("paste thread: ERROR — CGEventSource::new failed");
            return;
        };

        let post = |key_down: bool| {
            let Ok(event) = CGEvent::new_keyboard_event(source.clone(), KEY_V, key_down) else {
                crate::log::log("paste thread: ERROR — CGEvent::new_keyboard_event failed");
                return;
            };
            event.set_flags(CGEventFlags::CGEventFlagCommand);
            event.post(CGEventTapLocation::HID);
        };

        post(true); // key down
        post(false); // key up

        crate::log::log_verbose("paste thread: Cmd+V posted");
    });
}
