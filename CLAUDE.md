# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Cliphop is a macOS clipboard manager menu bar app written in Rust. It runs as a background agent (`LSUIElement = true`) with no Dock icon. It polls the clipboard every 500ms, maintains a history of up to 10 entries, and shows a popup menu on `Option+V` to select a previous entry for pasting.

## Build & Run

```bash
# Build
cargo build

# Build release
cargo build --release

# Run directly (development)
cargo run

# Check for errors without building
cargo check

# Code formatting
cargo fmt

# Run clippy lints
cargo clippy
```

## Testing

```bash
# Run unit tests (display formatting, truncation logic)
cargo test --lib

# Run integration tests (clipboard polling, selection, E2E flow)
# MUST use --test-threads=1 since tests share the system clipboard
cargo test --test clipboard_integration -- --test-threads=1

# Run all tests
cargo test -- --test-threads=1
```

```bash
# Run visibility smoke test (no Accessibility permission needed)
./tests/smoke_visibility.sh

# Run usability smoke test (requires Accessibility permission — prompts you to grant it)
./tests/smoke_usability.sh
```

```bash
# Run GUI E2E test (builds app, launches it, drives UI via osascript)
# Requires: Accessibility permissions for Terminal, macOS only
./tests/e2e_gui.sh
```

**Notes:**
- Integration tests write to the system clipboard — your clipboard contents will be overwritten during the test run
- All clipboard-touching tests must run serially (single-threaded) to avoid races on the shared system clipboard
- The GUI E2E test (`tests/e2e_gui.sh`) uses only macOS built-in tools (`osascript`, `pbcopy`, `pbpaste`) — no third-party dependencies
- The GUI E2E test requires Accessibility permissions for Terminal (System Settings > Privacy & Security > Accessibility)

## Architecture

The app is a single-threaded Cocoa/macOS app using `tao`'s event loop. All modules operate on the main thread. `MainThreadMarker` is obtained once at startup and passed to modules that need it.

**Event loop flow:**

1. `main.rs` — initializes all components and runs the `tao` event loop with a 500ms `WaitUntil` poll interval
2. On each `ResumeTimeReached` tick, `ClipboardHistory::poll()` checks `NSPasteboard.changeCount` for changes
3. On `Option+V` hotkey press (detected via `GlobalHotKeyEvent` channel), the popup is shown
4. Selecting an item writes it back to the pasteboard and calls `simulate_paste()` which synthesizes `Cmd+V` via CoreGraphics keyboard events

**Module responsibilities:**

- `main.rs` — initializes logging, accessibility check, hotkey, clipboard history, and tray; runs the event loop
- `clipboard.rs` — `ClipboardHistory`: polls `NSPasteboard`, deduplicates, caps at 10 items, writes selection back to pasteboard
- `hotkey.rs` — `Hotkey`: registers `Option+V` global hotkey via `GlobalHotKeyManager` (must stay alive)
- `popup.rs` — `show_popup()`: builds an `NSMenu` with clipboard items, pops it up at cursor position using a custom `PopupTarget` ObjC class to capture the selection via a thread-local; manages focus save/restore so paste targets the correct window
- `paste.rs` — `simulate_paste()`: spawns a background thread, polls until the target app regains focus (up to 200ms), then posts `Cmd+V` via CoreGraphics (`CGEvent` with `CGEventFlagCommand`)
- `tray.rs` — `Tray`: creates the menu bar status item (`doc.on.clipboard` SF Symbol), displays clipboard history as read-only items, plus Settings and Quit menu items
- `log.rs` — file-based logger writing to `~/.cliphop/log` (truncated on startup); supports verbose toggle via `set_verbose()`/`is_verbose()` backed by `AtomicBool`; `log_path()` returns the current log file path
- `settings.rs` — settings dialog GUI and app logic: `SettingsTarget` ObjC class for the "Settings..." tray menu item; shows an `NSAlert` dialog with version, live-updating accessibility status (with "Request Access" button via `OpenAccessibilityTarget`), a verbose-logging checkbox, and the log file path; `AccessibilityTimerTarget` polls `is_accessibility_trusted()` every 2s via `NSRunLoopCommonModes` to update the status label and button during the modal dialog; a `SETTINGS_WINDOW` thread-local prevents duplicate dialogs; restores `NSApplicationActivationPolicy::Accessory` after dialog closes to hide the Dock icon
- `macos.rs` — thin FFI bridge to macOS system calls: `is_accessibility_trusted()` (wraps `AXIsProcessTrusted`), `request_accessibility_trust()` (wraps `AXIsProcessTrustedWithOptions` with `kAXTrustedCheckOptionPrompt` to trigger the OS prompt), `launch_at_login_status()` / `set_launch_at_login()` (wraps `SMAppService` for login-item registration); callers use these without touching C/ObjC FFI directly

## ObjC Classes

Custom Objective-C classes defined with `define_class!` macro:
- `PopupTarget` (`popup.rs`) — handles menu item click callbacks, stores selection in thread-local
- `SettingsTarget` (`settings.rs`) — action target for the "Settings..." tray menu item
- `OpenAccessibilityTarget` (`settings.rs`) — action target for the "Request Access" button; delegates to `crate::macos::request_accessibility_trust()` and `crate::macos::open_accessibility_settings()`
- `AccessibilityTimerTarget` (`settings.rs`) — 2-second timer callback that polls `crate::macos::is_accessibility_trusted()` and live-updates the settings dialog status label and button visibility

## Key Dependencies

- `tao` — cross-platform event loop (used here for its macOS/Cocoa integration)
- `global-hotkey` — system-wide hotkey registration
- `objc2` / `objc2-app-kit` / `objc2-foundation` — safe Rust bindings to macOS Objective-C frameworks
- `core-graphics` — keyboard event types (imported for CGEvent compatibility)

## Separation of Concerns

**`macos.rs` is a thin FFI bridge — not an app logic module.** It wraps macOS C/ObjC system calls behind safe Rust functions. It must NOT contain:

- UI state
- Any GUI or application logic

**Rule of thumb:** if it wraps a C/ObjC/Swift system API behind a safe Rust function, it goes in `macos.rs`. If it defines behavior, UI, or ObjC classes for a feature, it goes in the feature module (e.g. `settings.rs`, `popup.rs`).

## macOS-Specific Notes

- Requires **Accessibility permissions** at runtime for CoreGraphics paste simulation to work (`CGEvent` HID posting)
- `AXIsProcessTrusted()` checks permission status; if denied, paste will silently fail
- `AXIsProcessTrustedWithOptions()` with `kAXTrustedCheckOptionPrompt` triggers the macOS permission dialog
- After rebuilding the binary, the Accessibility permission is invalidated — the user must remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility
- The app is a pure status bar agent; no window is created
- `NSView` is `MainThreadOnly` — always use `mtm.alloc()` instead of `NSView::alloc()`
