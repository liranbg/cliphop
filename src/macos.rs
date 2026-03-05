use std::ffi::c_void;

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
        let result = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        result
    }
}
