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
2. App launches and stays alive after 2 seconds
3. Process appears in running processes — via `osascript -e 'tell application "System Events" to (name of processes) contains "cliphop"'` (read-only query, no Accessibility required)
4. App does NOT appear in the Dock — `osascript` queries activation policy to confirm `LSUIElement = true` is in effect
5. Log file created at `~/.cliphop/log`
6. Clipboard polling works — `pbcopy` two distinct values with 1.5s gap each; assert app stays alive throughout
7. Cleanup: `kill` the app PID

**Constraints:**
- No `System Events` UI scripting (no clicks, no keystrokes)
- No Accessibility permission required at any point
- Exits 0 if all assertions pass, 1 if any fail

---

## Part 2 — `tests/smoke_usability.sh`

**Purpose:** Drive the full UI via osascript to verify all interactive capabilities.

**Preamble — Accessibility wait loop:**
- Check `tell application "System Events" to UI elements enabled`
- If not enabled: print human-readable instructions pointing to System Settings > Privacy & Security > Accessibility, then poll every 3 seconds until granted
- Never triggers a password prompt itself — just waits for the user to grant permission externally
- Once confirmed, proceed with tests

**Test cases:**

1. App launches (fresh or reuses running instance)
2. **Tray menu opens** — click cliphop's menu bar item via `System Events`; assert menu is visible
3. **Settings dialog opens** — click "Settings..." menu item; assert window containing version/accessibility info appears
4. **Settings dialog closes** — press Escape or click OK; assert dialog is dismissed
5. **Clipboard history populated** — `pbcopy` two distinct values with 1s gap each
6. **Option+V popup opens** — `key code 9 using option down`; assert popup menu appears
7. **Item selection sets clipboard** — navigate down + Return; assert `pbpaste` matches expected value
8. **Second selection** — repeat for the other history item; assert `pbpaste` matches
9. Cleanup: quit TextEdit (paste target), kill app

**Constraints:**
- Requires Accessibility permission — enforced by upfront wait loop, never silently skipped
- Uses a TextEdit window as a safe paste target
- Exits 0 if all assertions pass, 1 if any fail

---

## Shared Conventions

- Same `pass()` / `fail()` / summary output style as existing `e2e_gui.sh`
- `POLL_WAIT=1` variable for timing (overridable)
- `trap cleanup EXIT` for safe teardown
- Both scripts start with `set -euo pipefail`
- Both skip gracefully on non-macOS (`uname` check)

---

## File Layout

```
tests/
  smoke_visibility.sh   ← Part 1, no Accessibility
  smoke_usability.sh    ← Part 2, Accessibility required
  e2e_gui.sh            ← existing comprehensive E2E (unchanged)
```
