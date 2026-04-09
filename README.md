<div align="center">

# Cliphop

**A tiny, fast clipboard history manager for macOS - built in Rust.**

Press your hotkey (**Option+V** by default) to instantly access your clipboard history, search it, and pin entries that never expire.

No Dock icon. No bloat. Just your clipboard, always within reach.

[![CI](https://github.com/liranbg/cliphop/actions/workflows/ci.yml/badge.svg)](https://github.com/liranbg/cliphop/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

</div>

---

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/liranbg/cliphop/main/scripts/install.sh | sh
```

Then launch it:

```sh
open -a Cliphop
```

> **Note:** Cliphop is not notarized by Apple, so macOS Gatekeeper will block it on first launch with a warning about an unidentified developer. To allow it, run:
> ```sh
> xattr -cr /Applications/Cliphop.app
> ```
> Then open the app again. This is a one-time step.

> macOS will ask for **Accessibility** permission on first use — grant it in **System Settings > Privacy & Security > Accessibility**.

To uninstall:
```sh
rm -rf /Applications/Cliphop.app
```

## How It Works

1. Copy text as usual (**Cmd+C**)
2. Press your hotkey (**Option+V** by default) — a popup appears at your cursor
3. Type to filter, click an item to paste it, or press **Enter** to paste the most recent

Right-click any item to **Pin** it (pinned entries never expire) or delete it.

## Features

- Stores up to **50** text clipboard entries, persisted across restarts
- Configurable global hotkey (default **Option+V**) — change it in Settings
- **Search/filter** in the popup — type to narrow results instantly
- **Pin entries** — snippets that stay forever, separate from rolling history
- Automatic deduplication — re-copying moves the entry to the front
- Tray icon with clipboard history; click any item to paste it directly
- Settings: launch at login, hotkey, history limit, verbose logging
- Runs as a pure menu bar agent — no Dock icon, no windows

## Permissions

Cliphop needs **Accessibility** access to simulate Cmd+V for pasting. macOS prompts you on first use.

> After rebuilding from source, macOS invalidates the permission. Remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility.

## Roadmap

### v0.2 — Reliable Core ✓
- [x] History persistence across restarts
- [x] Launch at login (toggle in Settings)
- [x] Clear history

### v0.3 — Better UX ✓
- [x] Configurable hotkey
- [x] Search/filter in popup
- [x] Pin entries (snippets that never expire)

### v0.4 — Rich Content
- [ ] Image & file clipboard support
- [ ] Paste as plain text (strip formatting)
- [ ] Paste transformations (uppercase, lowercase, trim)

### v0.5 — Power User
- [ ] Folder/group support
- [ ] Self-update mechanism
- [ ] iCloud sync

## Development

**Requirements:** macOS, Rust 1.85+ (edition 2024)

```sh
cargo build --release
cargo run --release
```

Build a distributable DMG:

```sh
./scripts/build-dmg.sh
```

Create a release (bumps version in `Cargo.toml`, commits, tags, and pushes — CI builds the DMG and publishes a GitHub release):

```sh
./scripts/release.sh 0.3.0
```

> When running the bare binary, Accessibility permission is tied to your **terminal app**. For proper handling, run as an `.app` bundle or grant your terminal Accessibility access.

## License

MIT
