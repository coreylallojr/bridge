# Bridge

A local, full-screen command center for Windows, built with **Tauri 2 + Rust** and a React renderer.

- **Tiles:** one-click links, apps, folders and scripts, grouped and searchable (command palette).
- **Start my day:** a routine that opens your apps and places their windows on your monitors (zones and workspaces).
  The first run learns each app's window, and later runs reuse an open window instead of opening another.
- **Widgets, themes and settings:** live widgets (routine, recent, PC stats), several themes, density, font scale,
  motion and quality controls.
- **Reactor:** an animated arc-reactor centerpiece with a clock. Calm motion gives a still glow.
- **Shell:** a global hotkey (default `Alt+Space`), tray icon, single instance, and a `bridge …` CLI over a named pipe.
- **Local only:** no cloud services and no telemetry. Fonts and Three.js are bundled.

## Your data

Your profile lives in `%APPDATA%\Bridge\profile.json`, never in the repo. On first run Bridge seeds it from
[examples/starter-profile.json](examples/starter-profile.json). Edit tiles in the app or in that file.

## Build

Requirements: Windows 10/11 with WebView2, Node 20+, and Rust (stable). `scripts/rust.mjs` adds
`~/.cargo/bin` and `%LOCALAPPDATA%\Programs\winlibs\mingw64\bin` to PATH. That is for the GNU toolchain; the MSVC
toolchain works too.

```sh
npm install
npm run tauri:build   # vite build + release exe (the first build is slow)
npm start             # run it
```

## Tests

```sh
npm test              # renderer unit tests (Vitest)
npm run rust:test     # Rust workspace tests
npm run typecheck
```

## Layout

- `src/renderer/`: React UI
- `src-tauri/core`: pure logic + tests
- `src-tauri/sys`: launching, clipboard, icons, URL metadata, Start-menu apps, file drops
- `src-tauri/shell`: window modes, tray, hotkey, single instance, CLI pipe
- `src-tauri/src`: app wiring and API

## License and credit

Bridge is © 2026 Corey Lallo and is **source-available** under the
[PolyForm Noncommercial License 1.0.0](LICENSE.md):

- **Allowed:** using, studying, modifying and sharing it for personal, hobby, research, educational or other
  noncommercial purposes.
- **Required:** keep the [NOTICE](NOTICE) file and its `Required Notice:` line, and credit the original author
  ("Bridge by Corey Lallo, github.com/coreylallojr/bridge") in any copy or fork.
- **Not allowed without written permission:** selling it, bundling it in a commercial product, or using it for
  commercial work.

For commercial licensing, open an issue on this repo.

Before you publish a fork, run `npm run check:public`. It blocks workplace strings, real emails, user paths and
Domo IDs.
