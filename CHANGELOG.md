# Changelog

## Unreleased

### Fixed
- Pressing Enter in the popup with a search query that matches no items no longer pastes the first history item; the keypress is now ignored when all rows are filtered out
- Dock icon no longer stays visible after deleting all items through the popup context menu; activation policy is now properly restored to Accessory mode
- Typing the letter 'p' in the popup search field now works; previously the pin/unpin key handler consumed the event even when no row was selected
