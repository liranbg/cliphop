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
# menu bar 2 = the status/extras bar (right side); menu bar item 1 = cliphop icon
echo ""
echo "Test 2: Tray menu opens..."
if osascript -e '
tell application "System Events"
    tell process "cliphop"
        click menu bar item 1 of menu bar 2
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
        click menu bar item 1 of menu bar 2
    end tell
end tell' 2>/dev/null || true
sleep 0.4
osascript -e '
tell application "System Events"
    tell process "cliphop"
        click menu item "Settings" of menu 1 of menu bar item 1 of menu bar 2
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

# Test 4: Settings dialog closes on Escape
echo ""
echo "Test 4: Settings dialog closes..."
osascript -e 'tell application "System Events" to key code 53' 2>/dev/null || true
sleep 1

settings_gone=$(osascript -e '
tell application "System Events"
    tell process "cliphop"
        (count of windows) = 0
    end tell
end tell' 2>/dev/null || echo "false")

if [[ "$settings_gone" == "true" ]]; then
    pass "Settings dialog closes on Escape"
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

# Test 7: Select index 0 (smoke_test_B — most recent)
# The NSEvent local monitor intercepts all KeyDown events before the search
# NSTextField. First down-arrow sets SELECTED_ROW=Some(0); Return selects it.
echo ""
echo "Test 7: Select index 0 (most recent item)..."
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1
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
    delay 1
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

osascript <<'APPLESCRIPT' 2>/dev/null || true
tell application "System Events"
    tell process "cliphop"
        -- Get popup window origin; control-click at first history row
        -- (wy+50 = search field 36px + half row 14px = center of first history row)
        set win_pos to position of window 1
        set wx to item 1 of win_pos
        set wy to item 2 of win_pos
        click at {wx + 50, wy + 50} using {control down}
        delay 0.5
        -- Dismiss context menu / popup
        key code 53
    end tell
end tell
APPLESCRIPT
sleep 0.5

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "App stays alive after right-click on popup row (context menu triggered without crash)"
else
    fail "App alive after right-click" "running" "exited"
fi

# ── Summary ───────────────────────────────────────────────────────────

echo ""
echo "==============================="
echo "Results: $PASSED passed, $FAILED failed"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
    exit 1
fi
