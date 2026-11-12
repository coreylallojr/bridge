# Bridge performance baseline (Electron M2), 2026-11-03

Measured with `scripts/bench.ps1`:
- a temp `BRIDGE_HOME` holding the demo profile;
- `BRIDGE_DRY_LAUNCH=1` and `BRIDGE_WINDOW=1600x900`;
- only the process tree the bench launched.

Raw rows are appended to `docs/perf-results.jsonl`.

| Metric | Electron M2 | Tauri P1 (measured) | Tauri target |
|---|---|---|---|
| Cold start → window visible (median) | **5 946 ms** | 2 430 ms (2 075–3 705; noisy machine) | < 800 ms |
| Processes | 5 | 8 (1 Rust + 7 WebView2) | — |
| Working set, whole tree | **742 MB** | 540 MB | ≤ 40% of baseline |
| Private memory, whole tree | **419 MB** | 318 MB | ≤ 40% of baseline |
| CPU while visible (% of one core) | **180%** | 60% (Rust itself: 0.3%) | < 15% focused; ~0 when idle and blurred |
| CPU while hidden (externally hidden; see note) | 2.2% | 41% | ~0% |
| `bridge` pipe round trip (median) | 7 ms | 1.4 ms | < 150 ms |
| Size on disk | **361 MB** | **7.5 MB** | < 15 MB |

## What the numbers say
- **The biggest drain is CPU, not the runtime.** The reactor renders every frame at the monitor's refresh rate while
  Bridge is visible, using ~1.8 cores. Rust doesn't fix this. The reactor needs:
  - a frame cap (30 fps when focused);
  - a pause when pinned behind other windows (occluded);
  - a still frame when idle.
- **Cold start (~6 s)** is mostly Electron booting Chromium + Node from 361 MB of files on this PC (real-time AV scanning
  makes that worse).
- **Memory:** WebView2 is Chromium too. An empty Tauri spike window measured **188 MB private / 404 MB working set**,
  but most of WebView2's working set is shared with Edge, which is already running. So the memory win is real but not 10×.
  The big wins are startup, disk size, no Node process, and direct Win32 calls.

## Phase 0 spike results (Tauri 2.12 on this PC, `spike/tauri-hello`, throwaway)
- **(a) Build:** works per-user, no admin. It needs rustup with the GNU host plus the WinLibs MinGW zip (for
  `dlltool`/`as`) on PATH. The first full release build took 21 min; incremental builds take ~7 min with fat LTO.
  Dev builds will use a faster profile.
- **(b) Acrylic, frameless and transparent:** `apply_acrylic` → Ok.
- **(c) Drag-drop:** run with `dragDropEnabled: false`.
  - Edge-style URL drops arrive as `text/uri-list` + `text/plain`.
  - Explorer file drops arrive as `File` objects. Their real paths reach Rust through
    `chrome.webview.postMessageWithAdditionalObjects` → `ICoreWebView2File::Path` (verified: `C:/Windows/win.ini`).
- **(d) Testing:** Playwright `chromium.connectOverCDP` attaches via
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=N`. To test file drops, use the raw CDP
  `Input.dispatchDragEvent` with `files`; Playwright's `setInputFiles` uploads copies that aren't on disk.
- **(e) Executable:** 4.1 MB.
- **(f) Network:** a page-title fetch through a corporate network works with `reqwest` (native-tls/schannel, so it trusts
  corporate roots).

## Phase 1 notes (2026-11-03)
- **CPU:** the remaining CPU is all the reactor's WebGL (WebView2 renderer + GPU process on Intel UHD). Two fixes landed:
  - a 30 fps cap with 12 fps when blurred;
  - a fixed 0.6 MP internal render budget, scaled up by the browser.

  Together they took visible CPU from 190% to 60% of a core. The next levers are an idle cap (drop to ~12 fps after a
  minute without input) and lighter bloom.
- **The "hidden" row overstates real use.** The bench hides the window from outside (`ShowWindow`), so WebView2
  doesn't know it's hidden. Bridge's own hide goes through Tauri, which tells WebView2, and the reactor then pauses.
- **Memory:** WebView2 is Chromium too, so memory dropped only ~25%. The big wins are startup (2.4×), disk size (48×),
  no Node process, and direct Win32 calls.
