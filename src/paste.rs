use objc2::AnyThread;
use objc2::runtime::AnyObject;
use objc2::msg_send;
use objc2_foundation::{NSAppleScript, NSString};
use std::thread;
use std::time::Duration;

/// Simulates Cmd+V paste via in-process NSAppleScript.
/// Spawns a background thread to avoid blocking the event loop during the delay.
pub fn simulate_paste() {
    thread::spawn(|| {
        // Wait for the previous app to regain focus
        thread::sleep(Duration::from_millis(200));

        crate::log::log("paste thread: running NSAppleScript keystroke (in-process)");

        let source = NSString::from_str(
            "tell application \"System Events\" to keystroke \"v\" using command down",
        );

        let script = match NSAppleScript::initWithSource(NSAppleScript::alloc(), &source) {
            Some(s) => s,
            None => {
                crate::log::log("paste thread: ERROR — failed to create NSAppleScript");
                return;
            }
        };

        // Use raw msg_send! to handle nil returns safely (the generated binding
        // declares a non-optional Retained return which panics on nil).
        let mut error_dict: *mut AnyObject = std::ptr::null_mut();
        let result: *mut AnyObject = unsafe {
            msg_send![&*script, executeAndReturnError: &mut error_dict]
        };

        if result.is_null() || !error_dict.is_null() {
            crate::log::log(&format!(
                "paste thread: NSAppleScript failed (result_null={}, error_null={})",
                result.is_null(),
                error_dict.is_null()
            ));
        } else {
            crate::log::log("paste thread: NSAppleScript succeeded");
        }
    });
}
