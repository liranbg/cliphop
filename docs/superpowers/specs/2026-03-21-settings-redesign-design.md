# Settings Dialog Redesign — Design Spec

## Goal

Improve visual clarity and semantic grouping of the Cliphop Settings dialog. This spec describes the **target state** — the changes to be implemented in `src/settings.rs`. It does not describe the current code.

## Approach

Option A — Cleaned-up NSAlert. Stay within the existing `NSAlert` + accessory view pattern. Fix alignment, labels, section membership, and spacing. No new ObjC classes required.

## Width

`W = 300.0` → `W = 340.0`. Accessory view widens by 40px. All x-positions that reference `W` update automatically.

## Section layout

Three sections top to bottom: **Accessibility**, **History**, **Logging**. Two separators: between Accessibility/History and between History/Logging.

---

### Accessibility section

**Currently contains:** status badge + "Request Access" button.

**After redesign, contains:**
1. ✅/⚠️ status badge — unchanged
2. "Request Access" button (hidden when trusted) — unchanged
3. **"Launch at login" checkbox** — moved here from History (currently at y=132 inside the History block)

Rationale: launch-at-login is a system-level startup behavior, not a clipboard-data setting. It belongs with Accessibility (system trust + startup behavior).

---

### History section

**Currently contains:** Items row + Launch at login checkbox + Clear History button (right-aligned, no label).

**After redesign, contains:**
1. "Items retained" label + text field + stepper + range hint — label text changes from "Items:" to "Items retained"
2. **"Clear all history" + "Clear…" button row** — a full-width two-element row: left-anchored `NSTextField` label "Clear all history" with `NSColor.systemRedColor` text color, right-anchored "Clear…" button wired to `ClearButtonTarget`. Replaces the current orphaned right-aligned button (no label).

"Launch at login" is **removed** from this section.

Rationale: the old "Clear History" button had no label and floated right with no visual anchor. A label+button row makes the destructive action legible and clearly grouped within History.

---

### Logging section

**Currently contains:** verbose checkbox + raw path string (no label).

**After redesign, contains:**
1. "Verbose logging" checkbox — unchanged
2. **"Log file: ~/.cliphop/log"** — adds "Log file: " prefix to the path string and indents the label ~20px from the left edge (aligned under the checkbox text). This is a read-only `NSTextField` label, not an editable field.

Rationale: the raw path string had no label identifying what it was.

---

## Layout coordinates (target)

Container height: `h = 260.0` (unchanged from current — row count is the same, just redistributed).

Proposed y-positions (y=0 at bottom, increasing upward):

```
260: container top
240: Accessibility header
218: status badge row
196: Launch at login checkbox    ← moved from History
184: separator (Ax / History)
164: History header
142: Items retained row
120: Clear all history + Clear… button row
108: separator (History / Logging)
88:  Logging header
66:  Verbose logging checkbox
44:  Log file path label           (x ≈ 20, width W−20; indented under checkbox text)
```

Row height: 22px. Section header height: 18px. Separator: 1px.

---

## What does NOT change

- `NSAlert` usage — no custom NSWindow/NSPanel
- All four ObjC class definitions: `SettingsTarget`, `OpenAccessibilityTarget`, `AccessibilityTimerTarget`, `ClearButtonTarget`
- `confirm_and_clear_history` logic
- Stepper → text field wiring
- Accessibility timer polling
- Post-`runModal()` read-back and save logic (verbose, history limit, launch-at-login)

## Files affected

- `src/settings.rs` only — layout coordinates, label strings, `W` constant
