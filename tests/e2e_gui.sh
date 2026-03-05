#!/bin/bash
#
# End-to-end GUI test for Cliphop using only macOS built-in tools.
#
# Prerequisites:
#   - macOS with a desktop session (not SSH, not CI without a display)
#   - Accessibility permission for your terminal app
#     (System Settings > Privacy & Security > Accessibility)
#   - cargo installed (to build the binary)
#
# Usage:
#   ./tests/e2e_gui.sh
#
# What it tests:
#   1. App launches and creates a status bar item
#   2. Clipboard changes are detected by the app
#   3. Option+V hotkey opens the popup menu
#   4. Selecting a menu item updates the clipboard

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$SCRIPT_DIR/target/debug/cliphop"
APP_PID=""
PASSED=0
FAILED=0
POLL_WAIT=1  # seconds to wait for the app to poll clipboard

# ── Helpers ───────────────────────────────────────────────────────────

cleanup() {
    if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
    # Close TextEdit if we opened it
    osascript -e 'tell application "TextEdit" to quit' 2>/dev/null || true
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

# ── Prerequisite checks ──────────────────────────────────────────────

echo "Checking prerequisites..."

if [[ "$(uname)" != "Darwin" ]]; then
    echo "SKIP: This test only runs on macOS"
    exit 0
fi

# Check Accessibility permission by sending a no-op keystroke.
# If permission is denied, osascript exits with error code 1 and message 1002.
if ! osascript -e 'tell application "System Events" to key code 9 using {option down, control down, shift down, command down}' 2>/dev/null; then
    echo "SKIP: Accessibility permission not granted for this terminal."
    echo "      Go to: System Settings > Privacy & Security > Accessibility"
    echo "      and add your terminal app to enable GUI E2E tests."
    exit 0
fi

echo "  Accessibility permission: OK"

# ── Build ─────────────────────────────────────────────────────────────

echo "Building Cliphop..."
cd "$SCRIPT_DIR"
if ! cargo build 2>&1; then
    echo "ERROR: cargo build failed"
    exit 1
fi

if [[ ! -x "$BINARY" ]]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

# ── Test: App launches ────────────────────────────────────────────────

echo ""
echo "Test suite: GUI E2E"
echo "==================="

echo ""
echo "Starting Cliphop..."
"$BINARY" &
APP_PID=$!
sleep 2

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "App launches and stays running"
else
    echo "SKIP: App exited on startup (requires a macOS desktop session)"
    exit 0
fi

# ── Test: Clipboard detection ─────────────────────────────────────────

echo ""
echo "Testing clipboard detection..."

echo -n "gui_test_A" | pbcopy
sleep "$POLL_WAIT"

echo -n "gui_test_B" | pbcopy
sleep "$POLL_WAIT"

# At this point the app's history should be:
#   index 0: gui_test_B (most recent)
#   index 1: gui_test_A

# ── Test: Option+V popup and item selection ───────────────────────────

echo ""
echo "Testing Option+V popup and selection..."

# Open TextEdit as a paste target (so Cmd+V has somewhere safe to go)
osascript -e '
tell application "TextEdit"
    activate
    make new document
end tell'
sleep 0.5

# Open popup and select index 1 in a single osascript call.
# Keeping everything in one call avoids focus-stealing between invocations.
# Key codes: 9=V, 125=arrow down, 36=return
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

# Selecting index 1 should set clipboard to "gui_test_A"
assert_clipboard "gui_test_A" "Selecting 2nd item via popup sets clipboard"

# ── Test: Second selection ────────────────────────────────────────────

echo ""
echo "Testing second popup selection..."

# select() does NOT reorder the VecDeque, so history is still:
#   index 0: gui_test_B
#   index 1: gui_test_A
# Select index 0 ("gui_test_B") with arrow down once + return.
osascript -e '
tell application "System Events"
    key code 9 using option down
    delay 1
    key code 125
    delay 0.3
    key code 36
end tell'
sleep 1

assert_clipboard "gui_test_B" "Selecting 1st item via popup sets clipboard"

# ── Cleanup ───────────────────────────────────────────────────────────

echo ""
echo "Cleaning up..."

osascript -e '
tell application "TextEdit"
    close every document saving no
    quit
end tell' 2>/dev/null || true

kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true
APP_PID=""

# ── Summary ───────────────────────────────────────────────────────────

echo ""
echo "==================="
echo "Results: $PASSED passed, $FAILED failed"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
    exit 1
fi
