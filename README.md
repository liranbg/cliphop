<div align="center">

# Cliphop

**A tiny, fast clipboard history manager for macOS - built in Rust.**

Press **Option+V** to instantly access your last clipboard entries.

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

> macOS will ask for **Accessibility** permission on first use — grant it in **System Settings > Privacy & Security > Accessibility**.

To uninstall:
```sh
rm -rf /Applications/Cliphop.app
```

## How It Works

1. Copy text as usual (**Cmd+C**)
2. Press **Option+V** — a popup appears at your cursor
3. Click an item or press its number key (**0-9**) to paste it

## Features

- Stores last **10** text clipboard entries in memory
- **Option+V** global hotkey shows a native popup menu at your cursor
- Automatic deduplication — re-copying moves the entry to the front
- Menu bar icon with clipboard history preview
- Settings dialog with live accessibility status and verbose logging toggle
- Runs as a pure menu bar agent — no Dock icon, no windows

## Permissions

Cliphop needs **Accessibility** access to simulate Cmd+V for pasting. macOS prompts you on first use.

> After rebuilding from source, macOS invalidates the permission. Remove and re-add Cliphop in System Settings > Privacy & Security > Accessibility.

## Roadmap

### v0.2 — Reliable Core
- [ ] History persistence across restarts
- [ ] Launch at login (toggle in Settings)
- [ ] Clear history

### v0.3 — Better UX
- [ ] Configurable hotkey
- [ ] Search/filter in popup menu
- [ ] Pin entries (snippets that never expire)

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
./scripts/release.sh 0.2.0
```

> When running the bare binary, Accessibility permission is tied to your **terminal app**. For proper handling, run as an `.app` bundle or grant your terminal Accessibility access.

## License

MIT
