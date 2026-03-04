# Cliphop

A lightweight clipboard history manager for macOS, built in Rust.

Cliphop runs in the background as a menu bar app, tracking your last 10 clipboard text entries. Press **Option+V** to show a popup menu at your cursor and quickly paste any previous item.

## Features

- Stores last 10 text clipboard entries in memory
- Global hotkey (**Option+V**) shows a native popup menu at cursor
- Select items by clicking or pressing **0-9** on the keyboard
- Automatic deduplication — re-copying text moves it to the front
- Menu bar icon with clipboard history preview
- Settings dialog with accessibility status, verbose logging toggle, and log file path
- One-click link to open System Settings for Accessibility permissions

## Usage

1. Copy text as usual (Cmd+C)
2. Press **Option+V** to open the clipboard history popup
3. Click an item or press its number key (**0-9**) to paste it into the active application
4. Click the menu bar icon to browse your clipboard history, open Settings, or quit

## Permissions

Cliphop requires **Accessibility** access to simulate Cmd+V for pasting. macOS will prompt you to grant this on first use via System Settings > Privacy & Security > Accessibility.

> **Note:** After rebuilding the binary, macOS invalidates the Accessibility permission. You must remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility.

## Architecture

```text
src/
  main.rs         — Event loop (tao), wires everything together
  clipboard.rs    — NSPasteboard polling + history (VecDeque)
  hotkey.rs       — Global hotkey registration (Option+V)
  popup.rs        — Native NSMenu popup at cursor
  paste.rs        — Cmd+V simulation via CoreGraphics keyboard events (CGEvent)
  tray.rs         — Menu bar icon via NSStatusItem
  log.rs          — File-based logger (~/.cliphop/log)
  settings.rs     — Settings dialog (NSAlert)
  macos.rs        — macOS platform helpers (accessibility, System Settings)
```

## Roadmap

- [ ] Pin entries
- [ ] Configurable stack size
- [ ] Clear stack
- [ ] Configurable hot-key
- [ ] Self update

## Development

### Requirements

- macOS
- Rust 1.85+ (edition 2024)
- Accessibility permission (macOS will prompt on first paste)

### Build & Run

```sh
cargo build --release
cargo run --release
```

> **Note:** When running the bare binary (e.g. `./target/release/cliphop`), the Accessibility permission is associated with your **terminal app**, not Cliphop itself. For proper permission handling, run Cliphop as an `.app` bundle (see below) or grant your terminal Accessibility access.

#### Building a DMG for distribution

```sh
./scripts/build-dmg.sh
```

This builds a release binary, bundles it as `Cliphop.app`, and creates `target/Cliphop.dmg`.

## License

MIT
