# Smoke Tests Design

**Date:** 2026-03-25
**Topic:** Two-part osascript-driven smoke test suite for Cliphop

---

## Overview

Split the smoke test into two independent shell scripts so that the no-permission tests can always run cleanly, and the Accessibility-dependent tests explicitly wait for permission before starting — never prompting mid-test.

---

## Part 1 — `tests/smoke_visibility.sh`

**Purpose:** Verify the app is alive and observable without any Accessibility permission.

**Test cases:**

1. Binary exists at `target/debug/cliphop`
2. App launches and stays alive after 2 seconds — verified via `kill -0 $PID`
3. Process appears in running processes — via `pgrep -x cliphop` (plain shell, no `System Events`, no Accessibility or Automation consent needed)
4. App does NOT appear in the Dock — check via `lsappinfo list | grep -A5 cliphop | grep -i "LSUIElement"` or assert process has no dock tile using `lsappinfo` flags; this avoids any `System Events` call
5. Log file created at `~/.cliphop/log`
6. Clipboard polling works — `pbcopy` two distinct values with `$POLL_WAIT` seconds gap each (see Timing below); assert app stays alive throughout via `kill -0 $PID`
7. Cleanup: `kill $APP_PID`, `wait $APP_PID 2>/dev/null`

**Explicitly avoided:**
- No `tell application "System Events" ...` of any kind (avoids Automation consent dialog on first run)

---

## Part 2 — `tests/smoke_usability.sh`

**Purpose:** Drive the full UI via osascript to verify all interactive capabilities.

**Preamble — Accessibility wait loop:**
- Probe using an actual no-op keystroke: `osascript -e 'tell application "System Events" to key code 9 using {option down, control down, shift down, command down}'` and check exit code — same technique as `e2e_gui.sh`
- This correctly tests per-app Accessibility trust (not the global `UI elements enabled` preference, which is unreliable on Sonoma/Sequoia)
- If probe fails: print human-readable instructions ("Go to System Settings > Privacy & Security > Accessibility and add your terminal"), then poll every 3 seconds until the probe passes
- Never calls `AXIsProcessTrustedWithOptions` with the prompt flag — no password prompt is triggered by the test itself

**Test cases:**

1. App launches (fresh start; kill any existing instance first)
2. **Tray menu opens** — `tell application "System Events" to click menu bar item` for cliphop's status bar item; assert menu is visible
3. **Settings dialog opens** — click "Settings..." menu item; assert window containing version/accessibility info appears
4. **Settings dialog closes** — press Escape; assert dialog is dismissed
5. **Clipboard history populated** — `pbcopy` two distinct values (`smoke_test_A`, `smoke_test_B`) with `$POLL_WAIT` seconds gap each
6. **Option+V popup opens** — `key code 9 using option down`; assert popup panel appears
7. **Item selection sets clipboard (index 0)** — send one `key code 125` (down-arrow) then Return; assert `pbpaste` matches `smoke_test_B` (most recent item, index 0)
8. **Item selection sets clipboard (index 1)** — re-open popup; send two `key code 125` then Return; assert `pbpaste` matches `smoke_test_A` (older item, index 1)
9. **Pin context menu visible** — re-open popup; right-click (or `control-click`) first row via `System Events`; assert a context menu with "Pin" appears
10. Cleanup: quit TextEdit, kill app, `rm -f ~/.cliphop/history` (prevents stale history affecting subsequent runs)

**Known behavior notes:**
- The popup is an `NSPanel` (not `NSMenu`) with a local `NSEvent` monitor that captures `KeyDown` events at the application level, before they reach any first responder (including the search `NSTextField`). Down-arrow events are therefore never consumed by the text field.
- First down-arrow → `SELECTED_ROW = Some(0)` (index 0, most recent). One down-arrow + Return selects the most recent item.
- Second down-arrow → `SELECTED_ROW = Some(1)` (index 1, older). Two down-arrows + Return selects the second item.
- The search `NSTextField` receives focus on popup open and accepts typed characters, but no live-filtering logic is currently implemented — typed text does not hide rows.

---

## Shared Conventions

- Same `pass()` / `fail()` / summary output style as existing `e2e_gui.sh`
- `POLL_WAIT=1` (seconds) — used consistently in both scripts; app polls every 500ms so 1s is a safe margin
- `trap cleanup EXIT` for safe teardown in both scripts
- Both scripts start with `set -euo pipefail`
- Both skip gracefully on non-macOS (`uname` check at top)
- Test values use `smoke_test_A` / `smoke_test_B` prefix to distinguish from real clipboard contents

---

## File Layout

```
tests/
  smoke_visibility.sh   ← Part 1, no Accessibility or Automation permission needed
  smoke_usability.sh    ← Part 2, Accessibility required, explicit wait loop
  e2e_gui.sh            ← existing comprehensive E2E (unchanged)
```
