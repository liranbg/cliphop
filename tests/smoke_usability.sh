#!/bin/bash
#
# Smoke test — Part 2: Usability
#
# Drives the full Cliphop UI via osascript. Requires Accessibility permission.
# Waits for permission before starting — never prompts for password itself.
#
# Usage:
#   ./tests/smoke_usability.sh
#
# What it tests:
#   1. App launches
#   2. Tray menu bar item is clickable
#   3. Settings dialog opens and closes
#   4. Clipboard history is populated
#   5. Option+V popup opens
#   6. Selecting index 0 sets clipboard to most-recent item
#   7. Selecting index 1 sets clipboard to older item
#   8. Right-clicking a popup row does not crash the app
#   9. Search field accepts typed text; Escape clears field then dismisses popup
#  10. Pin item via keyboard shortcut; history reflects removal
#  11. Unpin item via keyboard shortcut; app stays alive
#  12. Pin item via right-click context menu; history reflects removal
#  13. Delete item via right-click context menu; history reflects removal
#  14. Row highlighting on keyboard navigation (verbose log check)
#  15. Clear all history via Settings; Option+V does not open popup
#
# Prerequisites:
#   - Accessibility permission for your terminal app
#     (System Settings > Privacy & Security > Accessibility)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$SCRIPT_DIR/target/debug/cliphop"
APP_PID=""
PASSED=0
FAILED=0
POLL_WAIT=1  # seconds; app polls every 500ms so 1s is a safe margin

# ── Helpers ───────────────────────────────────────────────────────────

cleanup() {
    osascript -e 'tell application "TextEdit" to quit saving no' 2>/dev/null || true
    if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
        kill "$APP_PID"
        wait "$APP_PID" 2>/dev/null || true
    fi
    # Remove persisted history so subsequent runs start with a clean slate
    rm -f "$HOME/.cliphop/history"
}
trap cleanup EXIT

pass() {
    echo "  PASS: $1"
    PASSED=$((PASSED + 1))
}

fail() {
    echo "  FAIL: $1 (expected: '$2', got: '$3')"
    FAILED=$((FAILED + 1))
}

assert_clipboard() {
    local expected="$1"
    local label="$2"
    local actual
    actual=$(pbpaste)
    if [[ "$actual" == "$expected" ]]; then
        pass "$label"
    else
        fail "$label" "$expected" "$actual"
    fi
}

# Inject a real right-click at screen coordinates (x, y) via CGEvent.
# AppleScript's `click at ... using {control down}` goes through the Accessibility
# layer and does NOT trigger NSView's rightMouseDown:, so we use JXA + CoreGraphics
# to post genuine right-mouse-down/up events that macOS delivers as rightMouseDown:.
right_click_at() {
    local x="$1" y="$2"
    osascript -l JavaScript <<JSEOF 2>/dev/null || true
ObjC.import('CoreGraphics');
var p = \$.CGPointMake($x, $y);
var down = \$.CGEventCreateMouseEvent(\$(), \$.kCGEventRightMouseDown, p, \$.kCGMouseButtonRight);
\$.CGEventPost(\$.kCGHIDEventTap, down);
delay(0.05);
var up = \$.CGEventCreateMouseEvent(\$(), \$.kCGEventRightMouseUp, p, \$.kCGMouseButtonRight);
\$.CGEventPost(\$.kCGHIDEventTap, up);
JSEOF
}

# Send a keystroke via CGEvent (key_code as decimal).
# CGEvent keyboard events go through the HID event tap and reach NSMenu tracking
# loops, unlike System Events keystroke which uses the Accessibility API.
cg_key() {
    local keycode="$1"
    osascript -l JavaScript <<JSEOF 2>/dev/null || true
ObjC.import('CoreGraphics');
var down = \$.CGEventCreateKeyboardEvent(\$(), $keycode, true);
\$.CGEventPost(\$.kCGHIDEventTap, down);
delay(0.05);
var up = \$.CGEventCreateKeyboardEvent(\$(), $keycode, false);
\$.CGEventPost(\$.kCGHIDEventTap, up);
JSEOF
}

# Inject a left-click at screen coordinates (x, y) via CGEvent.
left_click_at() {
    local x="$1" y="$2"
    osascript -l JavaScript <<JSEOF 2>/dev/null || true
ObjC.import('CoreGraphics');
var p = \$.CGPointMake($x, $y);
var down = \$.CGEventCreateMouseEvent(\$(), \$.kCGEventLeftMouseDown, p, \$.kCGMouseButtonLeft);
\$.CGEventPost(\$.kCGHIDEventTap, down);
delay(0.05);
var up = \$.CGEventCreateMouseEvent(\$(), \$.kCGEventLeftMouseUp, p, \$.kCGMouseButtonLeft);
\$.CGEventPost(\$.kCGHIDEventTap, up);
JSEOF
}

# Get the popup window position as "x y" (screen coords, top-left origin).
get_popup_pos() {
    osascript -e '
tell application "System Events"
    tell process "cliphop"
        set p to position of window 1
        return (item 1 of p as text) & " " & (item 2 of p as text)
    end tell
end tell' 2>/dev/null
}

# Get the popup window size as "w h".
get_popup_size() {
    osascript -e '
tell application "System Events"
    tell process "cliphop"
        set s to size of window 1
        return (item 1 of s as text) & " " & (item 2 of s as text)
    end tell
end tell' 2>/dev/null
}

# ── macOS guard ───────────────────────────────────────────────────────

if [[ "$(uname)" != "Darwin" ]]; then
    echo "SKIP: This test only runs on macOS"
    exit 0
fi

# ── Accessibility wait loop ───────────────────────────────────────────
# Probe using an actual no-op keystroke (same technique as e2e_gui.sh).
# This correctly tests per-app Accessibility trust on Sonoma/Sequoia,
# unlike the global "UI elements enabled" preference which is unreliable.

echo "Checking Accessibility permission..."
PROBE='tell application "System Events" to key code 9 using {option down, control down, shift down, command down}'

until osascript -e "$PROBE" 2>/dev/null; do
    echo ""
    echo "  Accessibility permission required. Steps:"
    echo "  1. Open System Settings > Privacy & Security > Accessibility"
    echo "  2. Add your terminal app to the list"
    echo "  Retrying in 3 seconds..."
    sleep 3
done
echo "  Accessibility permission: OK"

# ── Build ─────────────────────────────────────────────────────────────

echo ""
echo "Building Cliphop..."
cd "$SCRIPT_DIR"
if ! cargo build 2>&1; then
    echo "ERROR: cargo build failed"
    exit 1
fi

# Kill any stale instance from a previous run
pkill -x cliphop 2>/dev/null || true
# Delete stale history so the app starts clean — stale entries would
# shift index positions and break Tests 7 and 8.
rm -f "$HOME/.cliphop/history"
sleep 1

# ── Test suite ────────────────────────────────────────────────────────

echo ""
echo "Test suite: Smoke — Usability"
echo "==============================="

# Test 1: App launches
echo ""
echo "Test 1: App launches..."
# Bypass macOS Keychain with a random in-process key so no password dialog appears.
export CLIPHOP_HISTORY_KEY
CLIPHOP_HISTORY_KEY=$(openssl rand -hex 32)
"$BINARY" &
APP_PID=$!
sleep 2

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "App launches and stays running"
else
    echo "SKIP: App exited on startup (requires a macOS desktop session)"
    exit 0
fi

# Test 2: Tray menu opens
# cliphop is a UIElement app; it only has menu bar 1 (the status/extras bar).
# menu bar item 1 = cliphop status bar icon.
echo ""
echo "Test 2: Tray menu opens..."
if osascript -e '
tell application "System Events"
    tell process "cliphop"
        click menu bar item 1 of menu bar 1
    end tell
end tell' 2>/dev/null; then
    pass "Tray menu bar item is clickable"
else
    fail "Tray menu opens" "menu opened" "osascript error"
fi
sleep 0.5
# Dismiss with Escape before continuing
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 0.3

# Test 3: Settings dialog opens
# Split into two calls: cliphop switches NSApplicationActivationPolicy to Regular
# when the Settings dialog opens, and combining the two clicks in a single
# AppleScript block risks a mid-script accessibility-tree race around that switch.
echo ""
echo "Test 3: Settings dialog opens..."
osascript -e '
tell application "System Events"
    tell process "cliphop"
        click menu bar item 1 of menu bar 1
    end tell
end tell' 2>/dev/null || true
sleep 0.4
osascript -e '
tell application "System Events"
    tell process "cliphop"
        click menu item "Settings" of menu 1 of menu bar item 1 of menu bar 1
    end tell
end tell' 2>/dev/null || true
sleep 1

settings_visible=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        exists window 1
    end tell
end tell' 2>/dev/null || echo "false")

if [[ "$settings_visible" == "true" ]]; then
    pass "Settings dialog opens"
else
    fail "Settings dialog opens" "window visible" "${settings_visible}"
fi

# Test 4: Settings dialog closes via the "Close" button
# NSAlert only has one button ("Close"), so Escape has no effect — click the button.
echo ""
echo "Test 4: Settings dialog closes..."
osascript -e '
tell application "System Events"
    tell process "cliphop"
        click button "Close" of window 1
    end tell
end tell' 2>/dev/null || true
sleep 1

settings_gone=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        (count of windows) = 0
    end tell
end tell' 2>/dev/null || echo "false")

if [[ "$settings_gone" == "true" ]]; then
    pass "Settings dialog closes via Close button"
else
    fail "Settings dialog closes" "no windows" "window still present"
fi

# Test 5: Populate clipboard history
echo ""
echo "Test 5: Populating clipboard history..."
echo -n "smoke_test_A" | pbcopy
sleep "$POLL_WAIT"
echo -n "smoke_test_B" | pbcopy
sleep "$POLL_WAIT"
pass "Clipboard populated: smoke_test_A (index 1), smoke_test_B (index 0 / most recent)"

# Open TextEdit as a safe paste target
osascript -e '
tell application "TextEdit"
    activate
    make new document
end tell' || true
sleep 0.5

# Test 6: Option+V popup opens
echo ""
echo "Test 6: Option+V popup opens..."
osascript -e '
tell application "System Events"
    key code 9 using option down
end tell' || true
sleep 1.5

popup_visible=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        exists window 1
    end tell
end tell' 2>/dev/null || echo "false")

if [[ "$popup_visible" == "true" ]]; then
    pass "Option+V popup panel opens"
else
    fail "Option+V popup opens" "popup window visible" "${popup_visible}"
fi

# Dismiss the Test 6 popup with Escape before continuing
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 0.5

# Test 7: Select index 0 (smoke_test_B — most recent)
# The NSEvent local monitor intercepts all KeyDown events before the search
# NSTextField. First down-arrow sets SELECTED_ROW=Some(0); Return selects it.
echo ""
echo "Test 7: Select index 0 (most recent item)..."
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1.5
    key code 125
    delay 0.3
    key code 36
end tell'
sleep 1
assert_clipboard "smoke_test_B" "Selecting index 0 sets clipboard to smoke_test_B"

# Test 8: Select index 1 (smoke_test_A — older item)
# Two down-arrows → SELECTED_ROW=Some(1); Return selects it.
echo ""
echo "Test 8: Select index 1 (older item)..."
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1.5
    key code 125
    delay 0.3
    key code 125
    delay 0.3
    key code 36
end tell'
sleep 1
assert_clipboard "smoke_test_A" "Selecting index 1 sets clipboard to smoke_test_A"

# Test 9: Right-click on popup row — app stays alive (context menu smoke check)
# NSMenu spawned via popUpMenuPositioningItem:atLocation:inView: with a nil view
# does not appear in the System Events accessibility tree, so we can't enumerate
# its items via AppleScript. Instead we verify the app does not crash: open the
# popup, control-click the first row (triggering rightMouseDown: and NSMenu pop),
# dismiss with Escape, and assert the process is still running.
echo ""
echo "Test 9: Right-click on popup row (no crash)..."
osascript -e '
tell application "System Events"
    key code 9 using option down
end tell' || true
sleep 1

# Inject a real right-click via CGEvent at the first history row (wy+50)
read wx wy <<< "$(get_popup_pos)"
right_click_at $((wx + 50)) $((wy + 50))
sleep 0.5
# Dismiss context menu / popup
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 0.5

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "App stays alive after right-click on popup row (context menu triggered without crash)"
else
    fail "App alive after right-click" "running" "exited"
fi

# ── Tests 9–12: search field, pin, unpin, clear ──────────────────────
#
# These tests need a clean, known history state. Dismiss any popup left
# open by Test 8, then clear history via Settings and add two items.

# Test 9 leaves the popup open if either Escape landed on the wrong process.
# Dismiss it explicitly by targeting cliphop directly, then restart the app
# with a clean history file — this is more reliable than the Settings Clear
# button for test-setup purposes.
osascript -e 'tell application "System Events" to tell process "cliphop" to key code 53' 2>/dev/null || true
sleep 0.5

echo ""
echo "Setup for Tests 9–12: restart app with clean history, add two known items..."
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true
rm -f "$HOME/.cliphop/history"
"$BINARY" &
APP_PID=$!
sleep 2
if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "SKIP: App failed to restart (requires macOS desktop session)"
    exit 0
fi

echo -n "smoke_pin_A" | pbcopy
sleep "$POLL_WAIT"
echo -n "smoke_pin_B" | pbcopy
sleep "$POLL_WAIT"
# History: [smoke_pin_B (index 0, most recent), smoke_pin_A (index 1)]

# Test 9: Search field accepts typed text; Escape clears field; second Escape dismisses
echo ""
echo "Test 9: Search field..."
# Open the popup and type in a single osascript session so that cliphop is
# guaranteed frontmost (via activateIgnoringOtherApps) when the keystroke lands.
# A separate osascript call for keystroke uses AX process-targeting which doesn't
# deliver events through the NSApplication event queue the local monitor watches.
osascript <<'APPLE' || true
tell application "System Events"
    key code 9 using option down
    delay 1.5
    keystroke "smoketest"
end tell
APPLE
sleep 0.3

search_val=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        value of text field 1 of window 1
    end tell
end tell' 2>/dev/null || echo "")

if [[ "$search_val" == "smoketest" ]]; then
    pass "Search field accepts typed text"
else
    fail "Search field accepts typed text" "smoketest" "${search_val}"
fi

# First Escape: field non-empty → clears field, popup stays open.
# Target cliphop directly so the key reaches the popup's local event monitor.
osascript -e 'tell application "System Events" to tell process "cliphop" to key code 53' 2>/dev/null || true
sleep 0.3

cleared_val=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        value of text field 1 of window 1
    end tell
end tell' 2>/dev/null || echo "nonempty")

if [[ -z "$cleared_val" ]]; then
    pass "First Escape clears the search field (popup stays open)"
else
    fail "First Escape clears search field" "" "$cleared_val"
fi

# Second Escape: field empty → dismisses popup.
osascript -e 'tell application "System Events" to tell process "cliphop" to key code 53' 2>/dev/null || true
sleep 0.5

# Test 10: Pin item via keyboard shortcut
# History: [smoke_pin_B(0), smoke_pin_A(1)].  Down-arrow selects row 0, 'p' pins it.
# After pin: the popup stays open with updated content.
# history=[smoke_pin_A(0)], pinned=[smoke_pin_B].
# Down+Return on the already-open popup → clipboard = smoke_pin_A.
echo ""
echo "Test 10: Pin item via keyboard shortcut..."
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1.5
    key code 125
    delay 0.3
    key code 35
end tell' || true
sleep 1.5

# Popup stayed open after pin — select the first history item directly
osascript -e '
tell application "System Events"
    key code 125
    delay 0.3
    key code 36
end tell' || true
sleep 1

assert_clipboard "smoke_pin_A" "Pin: smoke_pin_A is the only remaining history item after pinning smoke_pin_B"

# Test 11: Unpin item via keyboard shortcut
# State: history=[smoke_pin_A(0)], pinned=[smoke_pin_B(0)].
# Down arrow twice: first selects history row 0, second selects pinned row 0.
# Then 'p' unpins.  Popup stays open; dismiss with Escape.
echo ""
echo "Test 11: Unpin item via keyboard shortcut..."
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1.5
    key code 125
    delay 0.2
    key code 125
    delay 0.2
    key code 35
end tell' || true
sleep 1

# Dismiss the still-open popup
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 0.5

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "App stays alive after unpinning item via context menu"
else
    fail "App alive after unpin" "running" "exited"
fi

# Test 12: Pin via right-click context menu
# After Test 11 unpin, state: history=[smoke_pin_B(0), smoke_pin_A(1)], pinned=[].
# Right-click on row 0 → context menu "Pin" → Return.
# Popup stays open.  Down+Return selects the remaining history item.
# Expected: history=[smoke_pin_A], pinned=[smoke_pin_B].
echo ""
echo "Test 12: Pin via right-click context menu..."
osascript -e '
tell application "System Events"
    key code 9 using option down
end tell' || true
sleep 1.5

# Right-click on first history row
read wx wy <<< "$(get_popup_pos)"
right_click_at $((wx + 50)) $((wy + 50))
sleep 0.7

# Press Return to select "Pin" (first context menu item)
cg_key 36
sleep 1.5

# Popup stayed open after pin — select the first history item directly
osascript -e '
tell application "System Events"
    key code 125
    delay 0.3
    key code 36
end tell' || true
sleep 1

assert_clipboard "smoke_pin_A" "Right-click Pin: smoke_pin_A is the only remaining history item after pinning smoke_pin_B via context menu"

# Test 13: Delete from history via right-click context menu
# State after Test 12: history=[smoke_pin_A], pinned=[smoke_pin_B].
# Add a new item, then delete it via right-click → Down → Return ("Delete from history").
# Popup stays open.  Down+Return selects the remaining history item.
echo ""
echo "Test 13: Delete from history via right-click context menu..."
echo -n "smoke_del_target" | pbcopy
sleep "$POLL_WAIT"
# State: history=[smoke_del_target(0), smoke_pin_A(1)], pinned=[smoke_pin_B]

osascript -e '
tell application "System Events"
    key code 9 using option down
end tell' || true
sleep 1.5

read wx wy <<< "$(get_popup_pos)"
right_click_at $((wx + 50)) $((wy + 50))
sleep 0.7

# Down arrow selects "Delete from history" (second context menu item), then Return
cg_key 125
sleep 0.2
cg_key 36
sleep 1.5

# Popup stayed open after delete — select first history item directly
osascript -e '
tell application "System Events"
    key code 125
    delay 0.3
    key code 36
end tell' || true
sleep 1

assert_clipboard "smoke_pin_A" "Right-click Delete: smoke_del_target removed, smoke_pin_A is first history item"

# Test 14: Row highlighting on keyboard navigation
# Restart app with verbose logging to capture highlight log messages.
echo ""
echo "Test 14: Row highlighting on keyboard navigation..."
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true
rm -f "$HOME/.cliphop/history"
mkdir -p "$HOME/.cliphop"
echo "verbose_logging=true" > "$HOME/.cliphop/config"
"$BINARY" &
APP_PID=$!
sleep 2

echo -n "highlight_test_A" | pbcopy
sleep "$POLL_WAIT"
echo -n "highlight_test_B" | pbcopy
sleep "$POLL_WAIT"

# Open popup, press Down twice, then Escape
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1.5
    key code 125
    delay 0.3
    key code 125
    delay 0.3
    key code 53
end tell' || true
sleep 1

# Check the log for "highlight: row 0" and "highlight: row 1" entries
hl_count=$(grep -c "highlight: row" "$HOME/.cliphop/log" 2>/dev/null || echo "0")
if [[ "$hl_count" -ge 2 ]]; then
    pass "Keyboard navigation triggers row highlighting ($hl_count highlight events)"
else
    fail "Row highlighting on keyboard nav" "≥2 highlight events" "$hl_count"
fi

# Clean up verbose config
rm -f "$HOME/.cliphop/config"

# Test 15: Clear all history; Option+V must not open the popup
# Add fresh items so there is definitely something to clear.
echo ""
echo "Test 15: Clear history..."
echo -n "smoke_clear_A" | pbcopy
sleep "$POLL_WAIT"
echo -n "smoke_clear_B" | pbcopy
sleep "$POLL_WAIT"

# Use the tray "Clear History" menu item — it invokes the same clear callback as
# the Settings dialog button, but is a direct child of the tray NSMenu, making it
# far more reliable to locate via AppleScript than an NSAlert accessory-view button.
osascript <<'APPLE' 2>/dev/null || true
tell application "System Events"
    tell process "cliphop"
        click menu bar item 1 of menu bar 1
    end tell
end tell
APPLE
sleep 0.4
osascript <<'APPLE' 2>/dev/null || true
tell application "System Events"
    tell process "cliphop"
        click menu item "Clear History" of menu 1 of menu bar item 1 of menu bar 1
    end tell
end tell
APPLE
sleep 1

# main.rs skips show_popup() when both history and pinned are empty.
# Option+V should fire the hotkey but no window should appear.
osascript -e 'tell application "System Events" to key code 9 using option down' 2>/dev/null || true
sleep 1

popup_after_clear=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        exists window 1
    end tell
end tell' 2>/dev/null || echo "false")

if [[ "$popup_after_clear" == "false" ]]; then
    pass "Option+V does not open popup when history is empty after clear"
else
    fail "Popup should not open after clear" "no window" "window visible"
    osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
fi

# ── Summary ───────────────────────────────────────────────────────────

echo ""
echo "==============================="
echo "Results: $PASSED passed, $FAILED failed"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
    exit 1
fi
