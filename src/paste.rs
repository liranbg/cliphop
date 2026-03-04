use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2_app_kit::NSWorkspace;
use std::thread;
use std::time::{Duration, Instant};

const KEY_V: CGKeyCode = 0x09; // ANSI_V

/// Simulates Cmd+V via CoreGraphics keyboard events (requires Accessibility permission).
/// `target_pid` is the process that was frontmost before our popup; we poll until
/// it regains focus before posting the events.
pub fn simulate_paste(target_pid: i32) {
    thread::spawn(move || {
        // Poll until the target app is frontmost again, with a 200ms timeout.
        let deadline = Instant::now() + Duration::from_millis(200);
        loop {
            let frontmost = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .map(|a| a.processIdentifier())
                .unwrap_or(-1);
            if frontmost == target_pid {
                break;
            }
            if Instant::now() >= deadline {
                crate::log::log("paste thread: timeout waiting for target app to regain focus");
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        crate::log::log("paste thread: posting Cmd+V via CoreGraphics");

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

        post(true);  // key down
        post(false); // key up

        crate::log::log("paste thread: Cmd+V posted");
    });
}
