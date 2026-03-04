use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};


unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Returns whether the current process has Accessibility permission.
pub fn is_accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Logs the current Accessibility permission status (called once at startup).
pub fn check_accessibility() {
    let trusted = is_accessibility_trusted();
    crate::log::log(&format!("AXIsProcessTrusted() = {}", trusted));
    if !trusted {
        crate::log::log(
            "WARNING: Accessibility NOT granted — paste will not work! \
             If you recently rebuilt, remove and re-add Cliphop in \
             System Settings > Privacy & Security > Accessibility.",
        );
    }
}

/// Opens System Settings → Privacy & Security → Accessibility pane.
pub fn open_accessibility_settings() {
    let url_string = NSString::from_str(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    );
    if let Some(url) = NSURL::URLWithString(&url_string) {
        let workspace = NSWorkspace::sharedWorkspace();
        workspace.openURL(&url);
    }
}
