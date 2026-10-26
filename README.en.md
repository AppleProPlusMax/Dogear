# Dogear

[中文](README.md)

A small, offline clipboard manager for Windows. What you copy stays findable: summon it with a shortcut, type to search, and press Enter to paste.

This is version 0.1.6. History, paste, screenshots, and on-device text recognition are already usable. The stack is Tauri 2, Vue 3, TypeScript, and SQLite. The design notes are in [docs/design.md](docs/design.md). Released under the [Apache-2.0](LICENSE) license.

## What it does

- Keeps text, links, code, and images. Closing the window does not throw them away.
- A favorite is not removed by the history limit, and clearing history leaves favorites in place.
- Records which app a copy came from. You can hide that label, or ignore copies from chosen apps.
- Code can be shown formatted, compacted, or as the original text. Colors found in text are listed on their own.
- Images are read with the Windows OCR engine on this PC. Nothing is sent to a cloud service.
- The interface can be switched between Simplified Chinese and English.

Clipboard contents stay on this computer. The app does not connect to the network by default, and it has no telemetry.

## Install

Download a build from [Releases](https://github.com/AppleProPlusMax/Dogear/releases).

- `Dogear_0.1.6_x64_zh-CN.msi` is the one to use. It replaces an older copy of the same product, and removes a previous setup.exe install.
- `Dogear_0.1.6_x64-setup.exe` is the NSIS installer.

The packages are not code-signed. If Windows shows a warning, choose to run it anyway. WebView2 is required; Windows 11 usually already has it.

## Use

The window stays hidden at startup. The icon sits in the tray. Minimize sends it to the taskbar, and close sends it to the tray. To quit, right-click the tray icon and choose Quit.

| Action | Default |
| --- | --- |
| Show or hide | Alt+V |
| Move the selection | ↑ ↓ |
| Paste | Enter |
| Paste as plain text | Shift+Enter |
| Favorite | Ctrl+P |
| Delete the current item | Delete |
| Screenshot | Alt+C |
| Hide the window | Esc |

The summon and screenshot shortcuts can be changed in Settings. History keeps the latest 200 items that are not favorites, or 500 or 1000 if you change it.

## Run from source

You need Node.js 20.19 or newer, Rust, the MSVC build tools, and WebView2.

```
npm install
npm run tauri dev
```

OCR samples:

```
cargo test probe_ocr_samples --manifest-path src-tauri/Cargo.toml -- --nocapture
```
