# Changelog

## Unreleased

### Fixed
- Windows-style `\r\n` line endings in clipboard entries now display as a single ↩ symbol instead of two; standalone `\r` and `\n` continue to display as one ↩ each
- Pressing Enter in the popup with a search query that matches no items no longer pastes the first history item; the keypress is now ignored when all rows are filtered out
- Dock icon no longer stays visible after deleting all items through the popup context menu; activation policy is now properly restored to Accessory mode
- Typing the letter 'p' in the popup search field now works; previously the pin/unpin key handler consumed the event even when no row was selected
- Hotkey recorder in Settings now correctly captures Space, Tab, Comma, and function keys (F1–F12); previously these were silently rejected because NSEvent character values were not mapped to the key names that the parser expects
- Paste after Pin/Delete/Unpin now reliably targets the correct app; on multi-iteration popup sessions the second+ call to show_popup would save Cliphop as the "previous" app, causing focus restoration to go to Cliphop instead of the original target
- Failed hotkey re-registration no longer leaves the app without a working hotkey; the previous hotkey is restored if the new one cannot be registered
