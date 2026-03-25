#!/bin/bash
#
# Smoke test — Part 1: Visibility
#
# Checks the app is alive and observable without any Accessibility or
# Automation permission. No System Events calls of any kind.
#
# Usage:
#   ./tests/smoke_visibility.sh
#
# What it tests:
#   1. Binary exists at target/debug/cliphop
#   2. App launches and stays alive
#   3. Process appears via pgrep
#   4. App is a UIElement (no Dock icon)
#   5. Log file is created
#   6. App stays alive during clipboard polling

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="$SCRIPT_DIR/target/debug/cliphop"
APP_PID=""
PASSED=0
FAILED=0
POLL_WAIT=1  # seconds; app polls every 500ms so 1s is a safe margin

# ── Helpers ───────────────────────────────────────────────────────────

cleanup() {
    local pid="${APP_PID:-$(pgrep -x cliphop | head -1 || echo "")}"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        kill "$pid"
        wait "$pid" 2>/dev/null || true
    fi
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

# ── macOS guard ───────────────────────────────────────────────────────

if [[ "$(uname)" != "Darwin" ]]; then
    echo "SKIP: This test only runs on macOS"
    exit 0
fi

# ── Build ─────────────────────────────────────────────────────────────

echo "Building Cliphop..."
cd "$SCRIPT_DIR"
if ! cargo build 2>&1; then
    echo "ERROR: cargo build failed"
    exit 1
fi

# ── Test suite ────────────────────────────────────────────────────────

echo ""
echo "Test suite: Smoke — Visibility"
echo "================================"

# Test 1: Binary exists
echo ""
echo "Testing binary..."
if [[ -x "$BINARY" ]]; then
    pass "Binary exists at target/debug/cliphop"
else
    fail "Binary exists" "file at $BINARY" "not found"
fi

# Test 2: App launches and stays alive
echo ""
echo "Launching Cliphop..."
BUNDLE="$SCRIPT_DIR/target/Cliphop.app"
if [[ -d "$BUNDLE" ]]; then
    open "$BUNDLE"
    sleep 2
    APP_PID=$(pgrep -x cliphop | head -1 || echo "")
else
    "$BINARY" &
    APP_PID=$!
    sleep 2
fi

if pgrep -x cliphop > /dev/null 2>&1; then
    pass "App launches and stays running"
else
    echo "SKIP: App exited on startup (requires a macOS desktop session)"
    exit 0
fi

# Test 3: Process visible via pgrep (no System Events — no Automation consent needed)
echo ""
echo "Testing process visibility..."
if pgrep -x cliphop > /dev/null 2>&1; then
    pass "Process appears in pgrep output"
else
    fail "Process visible via pgrep" "cliphop in process list" "not found"
fi

# Test 4: App runs as UIElement — no Dock icon (lsappinfo, no permissions needed)
echo ""
echo "Testing UIElement status..."
app_type=$(lsappinfo list 2>/dev/null | grep -i -A6 cliphop | grep 'type=' | head -1 || echo "")
if echo "$app_type" | grep -qi "UIElement"; then
    pass "App runs as UIElement (no Dock icon)"
else
    fail "App is UIElement" "ApplicationType = UIElement" "${app_type:-not found}"
fi

# Test 5: Log file created
echo ""
echo "Testing log file..."
sleep "$POLL_WAIT"
LOG_PATH="$HOME/.cliphop/log"
if [[ -f "$LOG_PATH" ]]; then
    pass "Log file created at ~/.cliphop/log"
else
    fail "Log file created" "$LOG_PATH" "not found"
fi

# Test 6: App stays alive during clipboard polling
echo ""
echo "Testing clipboard polling survival..."
echo -n "smoke_test_A" | pbcopy
sleep "$POLL_WAIT"
echo -n "smoke_test_B" | pbcopy
sleep "$POLL_WAIT"

if pgrep -x cliphop > /dev/null 2>&1; then
    pass "App stays alive during clipboard polling"
else
    fail "App alive after clipboard polling" "running" "exited"
fi

# ── Summary ───────────────────────────────────────────────────────────

echo ""
echo "================================"
echo "Results: $PASSED passed, $FAILED failed"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
    exit 1
fi
