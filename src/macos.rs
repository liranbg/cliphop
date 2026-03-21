use objc2::{msg_send, rc::Retained, runtime::NSObject};
use objc2_foundation::NSString;
use std::ffi::c_void;

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

// Accessibility framework (HIServices) — permission check and prompt
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    static kAXTrustedCheckOptionPrompt: *const c_void;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

// CoreFoundation helpers for building the options dictionary
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Returns whether the current process has Accessibility permission.
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Calls `AXIsProcessTrustedWithOptions` with `kAXTrustedCheckOptionPrompt`,
/// which prompts macOS to show the Accessibility permission dialog if not
/// already trusted.
pub fn request_accessibility_trust() -> bool {
    unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        let options = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            (&raw const kCFTypeDictionaryKeyCallBacks) as *const c_void,
            (&raw const kCFTypeDictionaryValueCallBacks) as *const c_void,
        );
        if options.is_null() {
            // Allocation failure: fall back to a plain trusted check (no prompt).
            return AXIsProcessTrusted();
        }
        let result = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        result
    }
}

/// Returns true if Cliphop is registered as a login item via SMAppService (macOS 13+).
/// Returns false if SMAppService is unavailable or the app is not registered.
pub fn launch_at_login_status() -> bool {
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(c"SMAppService") else {
            return false;
        };
        let service: Retained<NSObject> = msg_send![cls, mainAppService];
        // SMAppServiceStatusEnabled = 1
        let status: isize = msg_send![&*service, status];
        status == 1
    }
}

/// Registers (enabled=true) or unregisters (enabled=false) Cliphop as a login item.
/// Returns Err with a description if the call fails (e.g. bare binary, macOS < 13).
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    unsafe {
        let Some(cls) = objc2::runtime::AnyClass::get(c"SMAppService") else {
            return Err("SMAppService unavailable (requires macOS 13+)".to_string());
        };
        let service: Retained<NSObject> = msg_send![cls, mainAppService];

        let mut err_ptr: *mut NSObject = std::ptr::null_mut();
        let ok: bool = if enabled {
            msg_send![&*service, registerAndReturnError: &mut err_ptr]
        } else {
            msg_send![&*service, unregisterAndReturnError: &mut err_ptr]
        };

        if ok {
            Ok(())
        } else {
            let description = if err_ptr.is_null() {
                "unknown error".to_string()
            } else {
                let desc: Retained<NSString> = msg_send![&*err_ptr, localizedDescription];
                desc.to_string()
            };
            Err(description)
        }
    }
}
