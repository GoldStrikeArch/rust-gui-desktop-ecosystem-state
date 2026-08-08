# Raw Windows campaign rows (per app, verbatim — for report/21 synthesis)

Run date 2026-08-08 (WINDOWS-RUN.md Phases A–E complete; Phase E's MSI install
verification was closed by a supplemental elevated pass — see the Packaging section;
Phase F not performed). Environment header, verbatim from
`measurements/reruns/20260808-ten-framework-tri-platform/windows/environment.txt` line 1:

```
machine=AMD Ryzen AI 9 HX 370 w/ Radeon 890M; 31 GiB; Windows 11 Home 25H2 (build 26200.7171); rustc/cargo 1.96.1; AMD Radeon(TM) 890M Graphics driver 32.0.13058.2
```

- **Defender:** the exclusion state was UNQUERYABLE on this machine — the
  `[defender_exclusions]` block in environment.txt records only the single word
  `unavailable` (the block is emitted by `windows/capture-environment.ps1` when the
  `Get-MpPreference` query throws; the operator-observed failure code was 0x800106ba —
  Defender service not accessible — but that code itself is NOT captured in any committed
  artifact). Consequence: whether builds were being real-time-scanned is unknown; build
  timings carry that caveat.
- **DPI:** `[dpi_scale_primary]` = `200% (192 DPI)` — every visible-window and screenshot
  observation happened at 2× scale.
- **WebView2:** `[webview2_runtime] pv=151.0.4129.59` (preflight-verified; all dioxus apps
  ran against it — no tauri app built, see below).
- **Toolchain:** rustc/cargo 1.96.1; `[link_exe]` = `Microsoft (R) Incremental Linker
  Version 14.34.31948.0`. Builds ran under a VS 2022 dev shell pinned to MSVC toolset
  **14.34.31933** (evidenced by the `...\VC\Tools\MSVC\14.34.31933\bin\HostX64\x64\link.exe`
  invocations in the vizia build logs) because the default 14.44.35207 toolset installed on
  this machine is an incomplete stub with no cl.exe/link.exe (operator note; the stub itself
  left no artifact). This pin is load-bearing for the vizia linker findings below: the
  `__std_*` helpers vizia's prebuilt Skia libraries reference were added to the MSVC STL
  after 14.34.
- **Other header blocks:** git rev `5243b25d3e462af9516f7bb24b6bc36296d40cd2`
  (dirty_lines=4), pwsh 7.6.4, timezone `Russian Standard Time ((UTC+03:00) Москва,
  Санкт-Петербург)`.
- **Canonical artifacts** (all under
  `measurements/reruns/20260808-ten-framework-tri-platform/windows/`): `results.csv`
  (80-row aggregate), `runs/<app>/<variant>/` (result.tsv, build.log, app-stderr.log,
  app-stdout.log, run.log, windows.tsv — 108 variant runs total: 80 default + 10
  msmf-manifest + 16 wgpu-dx12 + 2 wgpu-gl), `runtime.csv` + `runtime-samples.csv` +
  `runtime-notes.txt` + `runtime-logs/` (Phase C), `selftests/results.csv` +
  `selftests/*.log` (Phase D), `packaging/results.csv` (33 rows) +
  `packaging/install-verify.csv` (7 rows, supplemental install verification) +
  `packaging/logs/` (Phase E), `babel-shots/` (iced-babel.png, 225,057 B), `tray-shots/`
  (iced-tray.png, 47,111 B).
- **Row provenance:** per-variant fields below are from `runs/<app>/<variant>/result.tsv`
  (which wins over results.csv on any disagreement); `workaround_variant` /
  `workaround_result` / `default_result` are the results.csv aggregate columns and are
  recorded on the default row. `exit_code: na` = still alive at the 10 s kill;
  `compile_ok: reused` = variant re-ran the already-built default binary with an env
  override. Sizes are bytes. There is no `binary_stripped_bytes` on Windows — `pdb_bytes`
  (the separate debug-symbol file) is recorded instead.
- **Selftest log capture caveat:** non-ASCII app output in `selftests/*.log` is mojibake'd
  (UTF-8 bytes decoded through a legacy codepage by the capture pipeline, e.g. `С‚РђР¤`);
  quoted lines below are verbatim including that mangling. PNG evidence is unaffected.

## iced

```yaml
framework: iced
app:
  default: {app: iced-app, variant: default, compile_ok: yes, clean_build_secs: 76.77, binary_bytes: 11130880, pdb_bytes: 5378048, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: iced-babel, variant: default, compile_ok: yes, clean_build_secs: 77.58, binary_bytes: 11565056, pdb_bytes: 5550080, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=225057", status: timeout, notes: "no exit within 60s; png_bytes=225057"}
board:
  default: {app: iced-board, variant: default, compile_ok: yes, clean_build_secs: 74.93, binary_bytes: 11237376, pdb_bytes: 5443584, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: iced-dash, variant: default, compile_ok: yes, clean_build_secs: 79.21, binary_bytes: 11481088, pdb_bytes: 5566464, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 1.4, peak_cpu_pct: 11.0, max_rss_mib: 220, helper_procs: 0}
fetch:
  default: {app: iced-fetch, variant: default, compile_ok: yes, clean_build_secs: 86.90, binary_bytes: 12793344, pdb_bytes: 6057984, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "selftests/iced-fetch.log is empty — no selftest markers at all"}
grid:
  default: {app: iced-grid, variant: default, compile_ok: yes, clean_build_secs: 75.00, binary_bytes: 11281920, pdb_bytes: 5435392, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "only line emitted: BUILD_MS 37.59"}
peek:
  default: {app: iced-peek, variant: default, compile_ok: yes, clean_build_secs: 125.51, binary_bytes: 19508224, pdb_bytes: 7376896, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: alive+window, default_result: alive+window, notes: ""}
  msmf-manifest: {app: iced-peek, variant: msmf-manifest, compile_ok: yes, clean_build_secs: 54.76, binary_bytes: 19635200, pdb_bytes: 7426048, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", notes: ""}
  selftest: {suite: peek, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 90s;", log: "selftests/iced-peek.log is empty"}
tray:
  default: {app: iced-tray, variant: default, compile_ok: yes, clean_build_secs: 109.66, binary_bytes: 14852096, pdb_bytes: 6483968, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 60s;", log: "hotkey register FAILED AlreadyRegistered; theme-changed: Dark; screenshot saved tray-shots/iced-tray.png"}
```

iced-babel selftest evidence (verbatim from `selftests/iced-babel.log`, encoding-mangled
by the log capture — the ZWJ family emoji's backspace corruption is the finding):

```
selftest caret columns over a|family|b: [0, 1, 26, 27]
selftest backspace over family: "aРЃРЇРЎРё\u{200d}РЃРЇРЎР№\u{200d}РЃРЇРЎР·\u{200d}" (len 7) -> CORRUPTED/partial
selftest select 16x right: Some("[MIXED] Mixed: H")
selftest select 22x right (into Arabic): Some("[MIXED] Mixed: Hello С„в••Р¦")
selftest paste round-trip: "start в•«Р№в•«Р¬в•«РҐв•«Р­ С„в••Р¦С‡РҐРњ РЃРЇРЎРё\u{200d}РЃРЇРЎР№\u{200d}РЃРЇРЎР·\u{200d}РЃРЇРЎР¶"
screenshot: saved C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\measurements\reruns\20260808-ten-framework-tri-platform\windows\babel-shots\iced-babel.png
```

FINDINGS / FRICTION:
- Cleanest framework of the campaign: 8/8 built, 8/8 defaults alive+window — and iced is
  wgpu-based, yet hit zero of the Vulkan surface deaths that took down egui/xilem/floem
  defaults on the same GPU/driver. The `ICED_BACKEND=tiny-skia` ladder rung never fired.
- iced-peek built AS-IS on Windows (the runbook expected the nokhwa `input-avfoundation`
  pin to break all non-freya peeks) and also under the msmf variant; both ran alive+window.
  Only 4 of 10 peek apps got that far at all.
- iced-babel is the only babel app in the whole campaign that produced screenshot evidence
  (`babel-shots/iced-babel.png`, 225,057 B) — the DirectWrite/harfrust gallery evidence for
  report-13. Its caret log `[0, 1, 26, 27]` and the backspace-over-ZWJ-family result
  `(len 7) -> CORRUPTED/partial` document that one Backspace splits the ZWJ emoji cluster
  and leaves a corrupted partial sequence; the paste round-trip preserved the string.
- iced-tray survived the hotkey failure that killed egui-tray/xilem-tray: it logged
  `shell setup FAILED: hotkey register: HotKey already registered` and kept running, then
  captured the only tray screenshot of the campaign (`tray-shots/iced-tray.png`) and
  detected the dark theme.
- Friction: neither grid nor fetch selftest ever armed on Windows (grid printed only
  `BUILD_MS 37.59`, fetch printed nothing), and no iced selftest exits on its own — every
  suite ended in the harness timeout kill.

## egui

```yaml
framework: egui
app:
  default: {app: egui-app, variant: default, compile_ok: yes, clean_build_secs: 88.89, binary_bytes: 14050304, pdb_bytes: 5795840, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: egui-babel, variant: default, compile_ok: yes, clean_build_secs: 93.88, binary_bytes: 33043456, pdb_bytes: 5795840, run_alive_10s: no, visible_window: no, exit_code: 1, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=1)"}
  wgpu-dx12: {app: egui-babel, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 33043456, pdb_bytes: 5795840, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=0", status: timeout, notes: "no exit within 60s; png_bytes=0", log: "selftests/egui-babel.log is empty — app stayed alive on default env but produced no PNG"}
board:
  default: {app: egui-board, variant: default, compile_ok: yes, clean_build_secs: 93.53, binary_bytes: 14089216, pdb_bytes: 5804032, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: egui-dash, variant: default, compile_ok: yes, clean_build_secs: 93.46, binary_bytes: 14457344, pdb_bytes: 5869568, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 4.3, peak_cpu_pct: 11.0, max_rss_mib: 346, helper_procs: 0}
fetch:
  default: {app: egui-fetch, variant: default, compile_ok: yes, clean_build_secs: 101.57, binary_bytes: 16247296, pdb_bytes: 6885376, run_alive_10s: no, visible_window: no, exit_code: 1, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=1)"}
  wgpu-dx12: {app: egui-fetch, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 16247296, pdb_bytes: 6885376, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "full pass sequence on DEFAULT env incl. SEARCH_STALE_DROP, DOWNLOAD_CANCELLED received_bytes=2359296, FLAKY_RESULT try=3 status=200, ending in SELFTEST_DONE (underscore marker, no pass counts, no exit)"}
grid:
  default: {app: egui-grid, variant: default, compile_ok: yes, clean_build_secs: 95.60, binary_bytes: 14175744, pdb_bytes: 5836800, run_alive_10s: no, visible_window: no, exit_code: 1, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=1)"}
  wgpu-dx12: {app: egui-grid, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 14175744, pdb_bytes: 5836800, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "partial run on default env: BUILD_MS 36.75, SELFTEST_START, 3 FILTER_MS lines, SELFTEST_SCROLL_BEGIN/DONE — no sort, no done marker"}
peek:
  default: {app: egui-peek, variant: default, compile_ok: yes, clean_build_secs: 109.79, binary_bytes: 15104000, pdb_bytes: 6238208, run_alive_10s: no, visible_window: no, exit_code: 1, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=1)"}
  msmf-manifest: {app: egui-peek, variant: msmf-manifest, compile_ok: yes, clean_build_secs: 87.92, binary_bytes: 15238144, pdb_bytes: 6295552, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", notes: ""}
  wgpu-dx12: {app: egui-peek, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 15104000, pdb_bytes: 6238208, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: "extra ladder rung not reflected in results.csv (single workaround column holds msmf-manifest)"}
  selftest: {suite: peek, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 90s;", log: "selftests/egui-peek.log is empty"}
tray:
  default: {app: egui-tray, variant: default, compile_ok: yes, clean_build_secs: 97.35, binary_bytes: 14495232, pdb_bytes: 5984256, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-gl, workaround_result: died, default_result: died, notes: "process exited before 10s (exit=101) | process exited before 10s (exit=101) | process exited before 10s (exit=101)"}
  wgpu-dx12: {app: egui-tray, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 14495232, pdb_bytes: 5984256, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "WGPU_BACKEND=dx12", notes: "process exited before 10s (exit=101)"}
  wgpu-gl: {app: egui-tray, variant: wgpu-gl, compile_ok: reused, clean_build_secs: "", binary_bytes: 14495232, pdb_bytes: 5984256, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "WGPU_BACKEND=gl", notes: "process exited before 10s (exit=101)"}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=101", status: fail, notes: "", log: "same AlreadyRegistered panic as Phase A"}
```

VERBATIM ERROR — egui-babel/fetch/grid/peek default `app-stderr.log` (identical line in all
four; eframe surrenders the error instead of panicking, hence exit=1):

```
Error: Wgpu(CreateSurfaceError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) }))
```

VERBATIM ERROR — egui-tray default `app-stderr.log` (identical under wgpu-dx12 and wgpu-gl;
this is an app-level panic before any rendering, so the backend ladder could never help):

```
thread 'main' (34624) panicked at src\main.rs:213:41:
register Cmd+Shift+9: AlreadyRegistered(HotKey { mods: Modifiers(SHIFT | SUPER), key: Digit9, id: 570425358 })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

FINDINGS / FRICTION:
- wgpu default-vs-dx12 ladder: 4 of 8 egui defaults (babel, fetch, grid, peek) died at
  launch with `FailedToCreateSurfaceForAnyBackend` while app/board/dash — same eframe/wgpu
  stack, same session — ran fine. `WGPU_BACKEND=dx12` rescued all four (4/4).
- Intermittency evidence: egui-fetch, which died on default in Phase A, later completed its
  ENTIRE Phase D selftest on the default env (through `SELFTEST_DONE`), and egui-grid ran
  partway — the Vulkan surface failure is flaky, not deterministic.
- egui-tray's death is not graphics: it panics registering Cmd+Shift+9
  (Modifiers(SHIFT | SUPER) — Win+Shift+9), which is already registered on this desktop
  (Win+Shift+digit is an OS taskbar shortcut on Windows 11). egui-tray is one of only two
  apps (with xilem-tray) that treat hotkey registration as fatal; five other frameworks'
  tray apps logged the same failure and kept running.
- egui-babel's binary is 33,043,456 B — the largest of the campaign, 2.3× its sibling apps
  (bundled babel font payload).
- Selftest marker friction: egui prints `SELFTEST_DONE` (underscore, no pass counts) and
  never exits — every surviving suite ended in the harness timeout, and babel produced no
  PNG (png_bytes=0) despite staying alive 60 s.

## gpui

```yaml
framework: gpui
app:
  default: {app: gpui-app, variant: default, compile_ok: yes, clean_build_secs: 163.05, binary_bytes: 10431488, pdb_bytes: 4435968, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: gpui-babel, variant: default, compile_ok: yes, clean_build_secs: 165.32, binary_bytes: 10972672, pdb_bytes: 4591616, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=0", status: timeout, notes: "no exit within 60s; png_bytes=0", log: "selftests/gpui-babel.log is empty"}
board:
  default: {app: gpui-board, variant: default, compile_ok: yes, clean_build_secs: 172.35, binary_bytes: 10527232, pdb_bytes: 4468736, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: gpui-dash, variant: default, compile_ok: yes, clean_build_secs: 162.28, binary_bytes: 10734080, pdb_bytes: 4517888, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 3.3, peak_cpu_pct: 16.0, max_rss_mib: 98, helper_procs: 0}
fetch:
  default: {app: gpui-fetch, variant: default, compile_ok: yes, clean_build_secs: 167.62, binary_bytes: 13224448, pdb_bytes: 5787648, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "full sequence: debounce (1 SEARCH_SENT for 'amber'), stale-drop, cancel, DL_DONE 8388608, ending in SELFTEST_DONE (underscore, no exit)"}
grid:
  default: {app: gpui-grid, variant: default, compile_ok: yes, clean_build_secs: 164.34, binary_bytes: 10781696, pdb_bytes: 4509696, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "BUILD_MS 34; filter pass (FILTER_MS 0.03–7.34); sort pass (SORT_MS 11.92–38.83); long scroll; ends SELFTEST_DONE (underscore, no exit)"}
peek:
  default: {app: gpui-peek, variant: default, compile_ok: no, clean_build_secs: 72.33, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed: error[E0433]: cannot find `unix` in `os`"}
  msmf-manifest: {app: gpui-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 5.79, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed: error[E0433]: cannot find `unix` in `os`"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: gpui-tray, variant: default, compile_ok: yes, clean_build_secs: 166.54, binary_bytes: 11970560, pdb_bytes: 4902912, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 60s;", log: "[tray-notes] tray icon created (NSStatusItem); [tray-notes] hotkey register FAILED: HotKey already registered"}
```

VERBATIM ERROR — gpui-peek default `build.log` (the permanent no-Windows-path finding: the
app's camera path depends directly on Apple's core-foundation, which cannot compile on
Windows; identical failure — 5.79 s — under the msmf variant):

```
error[E0433]: cannot find `unix` in `os`
  --> C:\Users\M.Pertsev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\core-foundation-0.10.0\src\filedescriptor.rs:19:14
   |
19 | use std::os::unix::io::{AsRawFd, RawFd};
   |              ^^^^ could not find `unix` in `os`

error[E0432]: unresolved import `libc::PATH_MAX`
  --> C:\Users\M.Pertsev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\core-foundation-0.10.0\src\url.rs:22:28
   |
22 | use libc::{c_char, strlen, PATH_MAX};
   |                            ^^^^^^^^ no `PATH_MAX` in the root

error: could not compile `core-foundation` (lib) due to 2 previous errors
```

FINDINGS / FRICTION:
- 7/8 built and every built default ran alive+window — gpui's in-house Direct3D 11 renderer
  had zero surface flakiness (it doesn't touch wgpu/Vulkan on Windows). gpui-dash was also
  the leanest dashboard measured: 98 MiB max RSS.
- gpui-peek is the confirmed permanent no-Windows-path finding: `core-foundation` (both
  0.10.0 and 0.9.4 compiled in the same build) fails with E0433/E0432 under default AND
  msmf variants — no camera route exists.
- Slowest builds of the campaign: 162–172 s clean for every gpui app, roughly 2× the iced/
  dioxus times.
- gpui-grid and gpui-fetch ran their full selftest sequences with visible timings but use
  the underscore `SELFTEST_DONE` marker and never exit — recorded as timeout, not pass.
- gpui-tray's log says `tray icon created (NSStatusItem)` on Windows — a macOS-worded log
  label for whatever it actually created here; hotkey registration failed AlreadyRegistered
  (handled, app kept running); no tray screenshot was produced.

## tauri

```yaml
framework: tauri
app:
  default: {app: tauri-app, variant: default, compile_ok: no, clean_build_secs: 68.81, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: error: failed to run custom build command for `tauri-app v0.1.0 (C:\\Users\\M.Pertsev\\Desktop\\workspace\\OSS\\rust-gui-desktop-ecosystem-state\\apps\\tauri-app)`"}
babel:
  default: {app: tauri-babel, variant: default, compile_ok: no, clean_build_secs: 69.20, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
board:
  default: {app: tauri-board, variant: default, compile_ok: no, clean_build_secs: 68.98, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
dash:
  default: {app: tauri-dash, variant: default, compile_ok: no, clean_build_secs: 69.24, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
  runtime: {avg_cpu_pct: DIED, peak_cpu_pct: "", max_rss_mib: "", helper_procs: "", note: "runtime.csv label is DIED but no binary ever existed — build-failed, not a runtime death"}
fetch:
  default: {app: tauri-fetch, variant: default, compile_ok: no, clean_build_secs: 79.19, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
grid:
  default: {app: tauri-grid, variant: default, compile_ok: no, clean_build_secs: 69.43, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
peek:
  default: {app: tauri-peek, variant: default, compile_ok: no, clean_build_secs: 23.03, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed (same build-script error)"}
  msmf-manifest: {app: tauri-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 7.46, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed (same build-script error)"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: tauri-tray, variant: default, compile_ok: no, clean_build_secs: 81.52, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed (same build-script error)"}
  selftest: {suite: tray, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
```

VERBATIM ERROR — tauri-app default `build.log` (the tauri-build build-script failure; the
decisive line is the last stdout line before the abort — identical mechanism in all 8 apps):

```
error: failed to run custom build command for `tauri-app v0.1.0 (C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\tauri-app)`

Caused by:
  process didn't exit successfully: `C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\tauri-app\target\release\build\tauri-app-f95adede08b0c9ef\build-script-build` (exit code: 1)
  ...
  cargo:rustc-env=TAURI_ENV_TARGET_TRIPLE=x86_64-pc-windows-msvc
  package.metadata does not exist
  `icons/icon.ico` not found; required for generating a Windows Resource file during tauri-build
```

FINDINGS / FRICTION:
- 0/8 built. Every tauri app fails in `tauri-build`'s build script before any Rust of the
  app compiles: `` `icons/icon.ico` not found; required for generating a Windows Resource
  file during tauri-build ``. The apps were authored on macOS with RGBA PNG icons only
  (the macOS cohort's icon friction, mirrored); Windows additionally demands an .ico and
  makes it a hard build error even with bundling inactive.
- Consequence: the whole WebView2 column for tauri is empty — the framework whose Windows
  story is its headline never reached the runtime. This is an as-is finding; sources were
  deliberately not patched mid-campaign (the fix would be one .ico file).
- The failure costs ~69–82 s per app because the full dependency graph (tauri 2.x, wry,
  tao) compiles before the app's own build script runs and aborts.

## xilem

```yaml
framework: xilem
app:
  default: {app: xilem-app, variant: default, compile_ok: yes, clean_build_secs: 85.05, binary_bytes: 12328448, pdb_bytes: 5836800, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: xilem-babel, variant: default, compile_ok: yes, clean_build_secs: 82.05, binary_bytes: 12342272, pdb_bytes: 5836800, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: xilem-babel, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 12342272, pdb_bytes: 5836800, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=0", status: timeout, notes: "no exit within 60s; png_bytes=0", log: "only line: INFO on Windows with AMD GPUs use premultiplied blitting even on opaque surface — launched on default env, no PNG"}
board:
  default: {app: xilem-board, variant: default, compile_ok: yes, clean_build_secs: 84.07, binary_bytes: 12519424, pdb_bytes: 6033408, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: xilem-dash, variant: default, compile_ok: yes, clean_build_secs: 82.24, binary_bytes: 12296704, pdb_bytes: 5828608, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 3.0, peak_cpu_pct: 17.0, max_rss_mib: 495, helper_procs: 0}
fetch:
  default: {app: xilem-fetch, variant: default, compile_ok: yes, clean_build_secs: 89.17, binary_bytes: 13872128, pdb_bytes: 6459392, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: xilem-fetch, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 13872128, pdb_bytes: 6459392, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "only the premultiplied-blitting INFO line — launched on default env, selftest never armed"}
grid:
  default: {app: xilem-grid, variant: default, compile_ok: yes, clean_build_secs: 78.83, binary_bytes: 12432896, pdb_bytes: 5935104, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: xilem-grid, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 12432896, pdb_bytes: 5935104, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "BUILD_MS 26.880 + premultiplied-blitting INFO — launched on default env (vs Phase A default death), then no selftest markers"}
peek:
  default: {app: xilem-peek, variant: default, compile_ok: yes, clean_build_secs: 100.86, binary_bytes: 13246464, pdb_bytes: 6639616, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  msmf-manifest: {app: xilem-peek, variant: msmf-manifest, compile_ok: yes, clean_build_secs: 36.68, binary_bytes: 13379584, pdb_bytes: 6688768, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", notes: ""}
  wgpu-dx12: {app: xilem-peek, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 13246464, pdb_bytes: 6639616, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: "extra ladder rung not reflected in results.csv (single workaround column holds msmf-manifest)"}
  selftest: {suite: peek, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 90s;", log: "[peek] gallery loaded: 200 thumbnails"}
tray:
  default: {app: xilem-tray, variant: default, compile_ok: yes, clean_build_secs: 92.32, binary_bytes: 12030976, pdb_bytes: 6082560, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-gl, workaround_result: died, default_result: died, notes: "process exited before 10s (exit=101) | process exited before 10s (exit=101) | process exited before 10s (exit=101)"}
  wgpu-dx12: {app: xilem-tray, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 12030976, pdb_bytes: 6082560, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "WGPU_BACKEND=dx12", notes: "process exited before 10s (exit=101)"}
  wgpu-gl: {app: xilem-tray, variant: wgpu-gl, compile_ok: reused, clean_build_secs: "", binary_bytes: 12030976, pdb_bytes: 6082560, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "WGPU_BACKEND=gl", notes: "process exited before 10s (exit=101)"}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=101", status: fail, notes: "", log: "same AlreadyRegistered panic as Phase A"}
```

VERBATIM ERROR — xilem-babel default `app-stderr.log` (identical panic site in xilem-fetch,
xilem-grid, xilem-peek defaults):

```
thread 'main' (26744) panicked at C:\Users\M.Pertsev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\masonry_winit-0.4.0\src\event_loop_runner.rs:971:6:
called `Result::unwrap()` on an `Err` value: WgpuCreateSurfaceError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

VERBATIM ERROR — xilem-tray default `app-stderr.log` (identical under wgpu-dx12/wgpu-gl):

```
thread 'main' (24564) panicked at src\shell.rs:329:10:
called `Result::unwrap()` on an `Err` value: AlreadyRegistered(HotKey { mods: Modifiers(SHIFT | SUPER), key: Digit9, id: 570425358 })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

FINDINGS / FRICTION:
- Same wgpu ladder shape as egui: 4 of 8 defaults (babel, fetch, grid, peek) died in
  masonry_winit's surface unwrap; `WGPU_BACKEND=dx12` rescued all four; app/board/dash ran
  fine on default in the same phase.
- Intermittency: xilem-grid/fetch/babel all LAUNCHED on default env in Phase D (each log
  carries vello's `INFO on Windows with AMD GPUs use premultiplied blitting even on opaque
  surface` line; xilem-grid also printed `BUILD_MS 26.880`) — the same binaries+env that
  died at launch in Phase A.
- xilem-tray mirrors egui-tray exactly: fatal `.unwrap()` on Win+Shift+9 hotkey
  registration (`AlreadyRegistered`), unrecoverable by any backend, selftest fail exit=101.
- No xilem selftest produced a single suite marker beyond BUILD_MS — grid/fetch armed
  nothing visible, babel produced no PNG; only xilem-peek (msmf) showed suite progress
  (`gallery loaded: 200 thumbnails`) before its timeout.
- xilem-dash's 495 MiB max RSS is the highest of the seven measured dashboards.

## slint

```yaml
framework: slint
app:
  default: {app: slint-app, variant: default, compile_ok: yes, clean_build_secs: 102.32, binary_bytes: 14769152, pdb_bytes: 4927488, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: slint-babel, variant: default, compile_ok: yes, clean_build_secs: 102.70, binary_bytes: 14786048, pdb_bytes: 4919296, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=0", status: timeout, notes: "no exit within 60s; png_bytes=0", log: "selftests/slint-babel.log is empty"}
board:
  default: {app: slint-board, variant: default, compile_ok: yes, clean_build_secs: 107.83, binary_bytes: 15346176, pdb_bytes: 5255168, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: slint-dash, variant: default, compile_ok: yes, clean_build_secs: 102.33, binary_bytes: 12403712, pdb_bytes: 4280320, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 15.8, peak_cpu_pct: 38.0, max_rss_mib: 153, helper_procs: 0}
fetch:
  default: {app: slint-fetch, variant: default, compile_ok: yes, clean_build_secs: 108.88, binary_bytes: 16268288, pdb_bytes: 5541888, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: no-marker, notes: "exit=0", log: "full sequence (STALE_DROPPED, DL cancel at progress=0.33, FLAKY_OK attempt 3, SNAPSHOT_SAVED verify-snapshot.ppm 1400x1120) ending in SELFTEST_DONE — underscore marker, clean exit 0"}
grid:
  default: {app: slint-grid, variant: default, compile_ok: yes, clean_build_secs: 103.85, binary_bytes: 14991872, pdb_bytes: 4984832, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: no-marker, notes: "exit=0", log: "full sequence (SELFTEST_ARMED, 8 FILTER_MS, 3 SORT_MS, SELECTION, COL1_WIDTH_AFTER_DRAG 260, SNAPSHOT_SAVED verify-snapshot.ppm 2000x1280, AUTOSCROLL_DONE, ROWDATA_CALLS_TOTAL 3092) ending in SELFTEST_DONE — underscore marker, clean exit 0"}
peek:
  default: {app: slint-peek, variant: default, compile_ok: yes, clean_build_secs: 107.79, binary_bytes: 15194112, pdb_bytes: 5140480, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: alive+window, default_result: alive+window, notes: ""}
  msmf-manifest: {app: slint-peek, variant: msmf-manifest, compile_ok: yes, clean_build_secs: 69.90, binary_bytes: 15326208, pdb_bytes: 5189632, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", notes: ""}
  selftest: {suite: peek, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 90s;", log: "[peek] gallery: 200 thumbs decoded + downscaled in 243 ms (4 threads)"}
tray:
  default: {app: slint-tray, variant: default, compile_ok: yes, clean_build_secs: 107.31, binary_bytes: 15219200, pdb_bytes: 5156864, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 60s;", log: "global hotkey register failed: HotKey already registered (handled; app kept running)"}
```

FINDINGS / FRICTION:
- 8/8 built, 8/8 defaults alive+window — with iced, one of only two frameworks with a
  perfect Phase A. Slint's FemtoVG/GL path sidestepped the wgpu Vulkan surface lottery
  entirely.
- slint-dash is the CPU outlier of the measured dashboards: 15.8% average / 38% peak
  (per-core semantics) versus 1.0–9.2% for everything else — consistent with the
  macOS-cohort finding that its Path elements re-tessellate every tick.
- Selftest heterogeneity, slint edition: grid and fetch both ran their COMPLETE pass
  sequences and are the only non-floem suites that exit cleanly (exit=0) — but they print
  bare `SELFTEST_DONE` (underscore, no pass counts), so the harness scores them `no-marker`
  rather than pass. Both also saved .ppm snapshots as side evidence.
- slint-peek built and ran as-is AND under msmf; its selftest decoded 200 thumbnails in
  243 ms on 4 threads but, like every peek app, never exits (timeout).
- slint-tray handled the Win+Shift+9 `AlreadyRegistered` failure gracefully (logged,
  kept running); no tray screenshot was produced.
- slint-fetch's status strings in the log show the capture mojibake (`HTTP 500 С‚РђР¤ click
  Retry`) — app-side those are arrow glyphs.

## dioxus

```yaml
framework: dioxus
app:
  default: {app: dioxus-app, variant: default, compile_ok: yes, clean_build_secs: 80.82, binary_bytes: 5047808, pdb_bytes: 3887104, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
babel:
  default: {app: dioxus-babel, variant: default, compile_ok: yes, clean_build_secs: 82.04, binary_bytes: 5157376, pdb_bytes: 3969024, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "exit= png_bytes=0", status: timeout, notes: "no exit within 60s; png_bytes=0", log: "selftests/dioxus-babel.log is empty"}
board:
  default: {app: dioxus-board, variant: default, compile_ok: yes, clean_build_secs: 81.94, binary_bytes: 5171200, pdb_bytes: 3952640, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
dash:
  default: {app: dioxus-dash, variant: default, compile_ok: yes, clean_build_secs: 81.37, binary_bytes: 5221888, pdb_bytes: 3969024, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  runtime: {avg_cpu_pct: 9.2, peak_cpu_pct: 26.0, max_rss_mib: 489, helper_procs: 6}
fetch:
  default: {app: dioxus-fetch, variant: default, compile_ok: yes, clean_build_secs: 86.48, binary_bytes: 6316032, pdb_bytes: 4444160, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "full sequence (search 20 results, CT cancelled, DL cancelled at 1966080/8388608, FLAKY success attempt 3) ending in SELFTEST_DONE — underscore marker, no exit"}
grid:
  default: {app: dioxus-grid, variant: default, compile_ok: yes, clean_build_secs: 84.27, binary_bytes: 5339136, pdb_bytes: 4018176, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: timeout, notes: "no exit within 120s; tree killed", log: "full sequence (BUILD_MS 40.3, FILTER_MS, SORT_MS, SELFTEST_SELECTED 21, NAME_COL_W 300, SCROLL_TOP 2799478) ending in SELFTEST_DONE — underscore marker, no exit"}
peek:
  default: {app: dioxus-peek, variant: default, compile_ok: no, clean_build_secs: 29.95, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed: error: `objc2` only works on Apple platforms. Pass `--target aarch64-apple-darwin` or similar to compile for macOS."}
  msmf-manifest: {app: dioxus-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 41.23, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed: same objc2 compile_error"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: dioxus-tray, variant: default, compile_ok: yes, clean_build_secs: 95.26, binary_bytes: 6543872, pdb_bytes: 4198400, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: alive+window, notes: ""}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 60s; [selftest] paste_image -> No clipboard image: The clipboard contents were not available in the requested format or the clipboard is empty.; [selftest] wrote C:\\Users\\M5453~1.PER\\AppData\\Local\\Temp\\dx-tray-selftest.txt; [selftest] about window requested"}
```

dioxus-tray selftest log, verbatim (`selftests/dioxus-tray.log`):

```
[tray-notes] clipboard image failed: The clipboard contents were not available in the requested format or the clipboard is empty.
[selftest] paste_image -> No clipboard image: The clipboard contents were not available in the requested format or the clipboard is empty.
[selftest] wrote C:\Users\M5453~1.PER\AppData\Local\Temp\dx-tray-selftest.txt
[tray-notes] notification posted via notify-rust
[selftest] about window requested
```

FINDINGS / FRICTION:
- 7/8 built and every built default ran alive+window on WebView2 pv 151.0.4129.59 —
  dioxus is the only webview framework that reached the Windows runtime at all
  (tauri: 0/8 built).
- Smallest binaries of the campaign (5.0–6.5 MB exe + ~4 MB PDB) — the webview payoff.
  The cost shows at runtime: dioxus-dash carried 6 msedgewebview2.exe helper processes and
  489 MiB max RSS (main + helpers summed), second-highest of the measured dashboards.
- dioxus-tray posted a real Windows toast via notify-rust (`[tray-notes] notification
  posted via notify-rust`), wrote its temp-file evidence, and opened its about window —
  the richest tray selftest outcome of the campaign; clipboard image paste correctly
  reported an empty clipboard.
- dioxus-peek fails to build under BOTH variants on the `objc2` compile_error (see the
  floem section for the verbatim block — identical error): even with msmf camera routing,
  something in the peek dependency graph still drags Apple-only objc2 onto Windows.
- Both grid and fetch selftests ran their full sequences but print underscore
  `SELFTEST_DONE` and never exit — timeouts, not passes; babel stayed alive but produced
  no PNG.

## freya

```yaml
framework: freya
app:
  default: {app: freya-app, variant: default, compile_ok: no, clean_build_secs: 42.89, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: error: failed to run custom build command for `freya-skia-bindings v0.98.1`"}
babel:
  default: {app: freya-babel, variant: default, compile_ok: no, clean_build_secs: 42.85, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
board:
  default: {app: freya-board, variant: default, compile_ok: no, clean_build_secs: 42.54, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
dash:
  default: {app: freya-dash, variant: default, compile_ok: no, clean_build_secs: 41.77, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  runtime: {avg_cpu_pct: DIED, peak_cpu_pct: "", max_rss_mib: "", helper_procs: "", note: "runtime.csv label is DIED but no binary ever existed — build-failed, not a runtime death"}
fetch:
  default: {app: freya-fetch, variant: default, compile_ok: no, clean_build_secs: 50.61, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
grid:
  default: {app: freya-grid, variant: default, compile_ok: no, clean_build_secs: 42.15, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
peek:
  default: {app: freya-peek, variant: default, compile_ok: no, clean_build_secs: 67.36, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  msmf-manifest: {app: freya-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 10.28, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed: same freya-skia-bindings error"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: freya-tray, variant: default, compile_ok: no, clean_build_secs: 45.91, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same freya-skia-bindings error"}
  selftest: {suite: tray, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
```

VERBATIM ERROR — freya-app default `build.log`: a two-stage failure. Stage 1, the prebuilt
Skia download, dies on a malformed URL (curl exit 3); stage 2, the from-source fallback,
dies looking for LLVM:

```
  TRYING TO DOWNLOAD AND INSTALL SKIA BINARIES: 0.98.1/b5756d8613bf27909a64-x86_64-pc-windows-msvc-gl-jpegd-jpege-svg-textlayout-vulkan-webpd-webpe
  cargo:rerun-if-env-changed=SKIA_BINARIES_URL
    FROM: https://github.com/marc2332/rust-skia/releases/download/0.98.1/skia-binaries-b5756d8613bf27909a64-x86_64-pc-windows-msvc-gl-jpegd-jpege-svg-textlayout-vulkan-webpd-webpe.tar.gz
  DOWNLOAD AND INSTALL FAILED: curl error code: "3"
  curl stderr: "curl: (3) URL using bad/illegal format or missing URL\r\n"
  STARTING A FULL BUILD
```

```
  --- stderr
  Checking for "C:\\Program Files\\LLVM\\bin\\clang-cl.exe"
  Checking for "C:\\LLVM\\bin\\clang-cl.exe"
  Checking for "C:\\Users\\M.Pertsev\\scoop\\apps\\llvm\\current\\bin\\clang-cl.exe"

  thread 'main' (30396) panicked at C:\Users\M.Pertsev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\freya-skia-bindings-0.98.1\build_support\platform\windows.rs:40:13:
  Unable to locate LLVM installation
  note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

FINDINGS / FRICTION:
- 0/8 built (including freya-peek, the one peek the runbook expected might build as-is).
  Every app dies identically inside `freya-skia-bindings v0.98.1`'s build script.
- The failure is freya's own Skia fork infrastructure, in two independent stages: (1) the
  marc2332/rust-skia prebuilt-binaries download URL for this Windows feature set is
  malformed or missing — curl(3) "URL using bad/illegal format or missing URL" — so the
  prebuilt path that carried the macOS cohort does not exist for Windows; (2) the fallback
  full-source Skia build hard-requires LLVM/clang-cl at three probed paths and panics when
  none exists. Building freya on this machine would require installing LLVM — a toolchain
  demand no other framework in the cohort makes.
- The macOS-cohort risk note ("pins the app ... to a prebuilt-binary download on first
  build", stack-rows.md) converted into a full 8/8 Windows build outage.

## vizia

```yaml
framework: vizia
app:
  default: {app: vizia-app, variant: default, compile_ok: no, clean_build_secs: 54.38, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: error: linking with `link.exe` failed: exit code: 1120"}
babel:
  default: {app: vizia-babel, variant: default, compile_ok: no, clean_build_secs: 50.70, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
board:
  default: {app: vizia-board, variant: default, compile_ok: no, clean_build_secs: 50.16, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
dash:
  default: {app: vizia-dash, variant: default, compile_ok: no, clean_build_secs: 50.85, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
  runtime: {avg_cpu_pct: DIED, peak_cpu_pct: "", max_rss_mib: "", helper_procs: "", note: "runtime.csv label is DIED but no binary ever existed — build-failed, not a runtime death"}
fetch:
  default: {app: vizia-fetch, variant: default, compile_ok: no, clean_build_secs: 61.51, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
grid:
  default: {app: vizia-grid, variant: default, compile_ok: no, clean_build_secs: 52.34, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
peek:
  default: {app: vizia-peek, variant: default, compile_ok: no, clean_build_secs: 70.39, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed: same LNK1120"}
  msmf-manifest: {app: vizia-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 34.88, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed: same LNK1120"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: vizia-tray, variant: default, compile_ok: no, clean_build_secs: 61.17, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: same LNK1120"}
  selftest: {suite: tray, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
```

VERBATIM ERROR — vizia-app default `build.log` (representative LNK2019 lines plus the
LNK1120 close; the full set is 5 unresolved `__std_*` symbols: `__std_find_last_trivial_2`,
`__std_min_element_f`, `__std_max_element_f`, `__std_minmax_element_f`, `__std_search_1`):

```
skunicode_icu.lib(icu.SkLoadICU.obj) : error LNK2019: unresolved external symbol __std_find_last_trivial_2 referenced in function "class std::basic_string<wchar_t,struct std::char_traits<wchar_t>,class std::allocator<wchar_t> > __cdecl get_module_path(struct HINSTANCE__ *)" (?get_module_path@@YA?AV?$basic_string@_WU?$char_traits@_W@std@@V?$allocator@_W@2@@std@@PEAUHINSTANCE__@@@Z)
skia.lib(core.SkStroke.obj) : error LNK2019: unresolved external symbol __std_min_element_f referenced in function "private: bool __cdecl SkPathStroker::ptInQuadBounds(struct SkPoint const * const,struct SkPoint const &)const " (?ptInQuadBounds@SkPathStroker@@AEBA_NQEBUSkPoint@@AEBU2@@Z)
skia.lib(core.SkGlyph.obj) : error LNK2019: unresolved external symbol __std_minmax_element_f referenced in function "public: void __cdecl SkGlyph::ensureIntercepts(float const * const,float,float,float *,int *,class SkArenaAlloc *)" (?ensureIntercepts@SkGlyph@@QEAAXQEBMMMPEAMPEAHPEAVSkArenaAlloc@@@Z)
skia.lib(skia.SkSLErrorReporter.obj) : error LNK2019: unresolved external symbol __std_search_1 referenced in function "public: void __cdecl SkSL::ErrorReporter::error(class SkSL::Position,class std::basic_string_view<char,struct std::char_traits<char> >)" (?error@ErrorReporter@SkSL@@QEAAXVPosition@2@V?$basic_string_view@DU?$char_traits@D@std@@@std@@@Z)
C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\vizia-app\target\release\deps\vizia_app.exe : fatal error LNK1120: 5 unresolved externals
```

FINDINGS / FRICTION:
- 0/8 built. All Rust code compiles; every app dies at the final link with the identical 5
  unresolved `__std_*` externals out of skia-safe 0.93's PREBUILT skia.lib/skunicode_icu.lib.
- This is where the campaign's toolset pin is load-bearing: the `__std_*` vector-algorithm
  helpers are MSVC STL internals introduced after toolset 14.34; vizia's prebuilt Skia
  binaries were compiled against a newer STL, and the campaign links with pinned 14.34.31933
  (the machine's default 14.44.35207 install being a cl.exe/link.exe-less stub). Prebuilt
  C++ static libraries + mismatched MSVC STL = link death that no cargo knob fixes.
- Contrast with freya: both frameworks fail on their Skia binary-distribution
  infrastructure, but in opposite stages — freya can't OBTAIN its Skia binaries (download +
  no LLVM), vizia obtains them and can't LINK them.
- The failures are expensive (50–70 s per app) because the full ~21 MB Skia link is
  attempted every time.

## floem

```yaml
framework: floem
app:
  default: {app: floem-app, variant: default, compile_ok: yes, clean_build_secs: 132.28, binary_bytes: 18121728, pdb_bytes: 7204864, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-app, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 18121728, pdb_bytes: 7204864, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
babel:
  default: {app: floem-babel, variant: default, compile_ok: no, clean_build_secs: 75.29, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: none, workaround_result: not_run, default_result: build-failed, notes: "build failed: error: `objc2` only works on Apple platforms. Pass `--target aarch64-apple-darwin` or similar to compile for macOS."}
  selftest: {suite: babel, expected: "exit=0 png>10240B", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
board:
  default: {app: floem-board, variant: default, compile_ok: yes, clean_build_secs: 128.84, binary_bytes: 18264576, pdb_bytes: 7262208, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-board, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 18264576, pdb_bytes: 7262208, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
dash:
  default: {app: floem-dash, variant: default, compile_ok: yes, clean_build_secs: 133.66, binary_bytes: 18348544, pdb_bytes: 7286784, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-dash, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 18348544, pdb_bytes: 7286784, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  runtime: {avg_cpu_pct: 1.0, peak_cpu_pct: 11.0, max_rss_mib: 327, helper_procs: 0, note: "Phase C ran on DEFAULT env (runtime-sample.ps1 sets no WGPU_BACKEND) and floem-dash survived the full 5s+30x1s sampling with empty stderr — key intermittency data point vs its Phase A default death"}
fetch:
  default: {app: floem-fetch, variant: default, compile_ok: yes, clean_build_secs: 135.04, binary_bytes: 20083200, pdb_bytes: 7983104, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-fetch, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 20083200, pdb_bytes: 7983104, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: fetch, expected: "SELFTEST DONE pass=10 fail=0", observed: "SELFTEST DONE pass=10 fail=0", status: pass, notes: "exit=0", log: "full sequence: stale-drop gen=2, DL_DONE bytes=8388608, FLAKY_OK attempts=3 — canonical marker + clean exit"}
grid:
  default: {app: floem-grid, variant: default, compile_ok: yes, clean_build_secs: 126.88, binary_bytes: 18429952, pdb_bytes: 7286784, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-grid, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 18429952, pdb_bytes: 7286784, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: grid, expected: "SELFTEST DONE pass=14 fail=0", observed: "SELFTEST DONE pass=14 fail=0", status: pass, notes: "exit=0", log: "BUILD_MS 33.77; FILTER_MS 0.81–8.17; SORT (24.67/5.43/4.94 ms); SELECT; RESIZE col=id width=125; WINDOW first=10000 y=260000 — canonical marker + clean exit"}
peek:
  default: {app: floem-peek, variant: default, compile_ok: no, clean_build_secs: 113.42, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: msmf-manifest, workaround_result: build-failed, default_result: build-failed, notes: "build failed: same objc2 compile_error"}
  msmf-manifest: {app: floem-peek, variant: msmf-manifest, compile_ok: no, clean_build_secs: 21.45, binary_bytes: 0, pdb_bytes: 0, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", notes: "build failed: same objc2 compile_error"}
  selftest: {suite: peek, expected: "exit=0", observed: "", status: no-binary, notes: "release exe missing (run-cohort builds first)"}
tray:
  default: {app: floem-tray, variant: default, compile_ok: yes, clean_build_secs: 131.02, binary_bytes: 20294656, pdb_bytes: 8089600, run_alive_10s: no, visible_window: no, exit_code: 101, env_overrides: "", workaround_variant: wgpu-dx12, workaround_result: alive+window, default_result: died, notes: "process exited before 10s (exit=101)"}
  wgpu-dx12: {app: floem-tray, variant: wgpu-dx12, compile_ok: reused, clean_build_secs: "", binary_bytes: 20294656, pdb_bytes: 8089600, run_alive_10s: yes, visible_window: yes, exit_code: na, env_overrides: "WGPU_BACKEND=dx12", notes: ""}
  selftest: {suite: tray, expected: "exit=0", observed: "exit=", status: timeout, notes: "no exit within 60s;", log: "theme-changed: dark; shell setup FAILED: hotkey register: HotKey already registered; screenshot: FAILED: program not found"}
```

VERBATIM ERROR — floem-babel default `build.log` (identical in floem-peek and dioxus-peek;
the objc2 compile_error that keeps Apple-only dependency branches out of Windows builds):

```
error: `objc2` only works on Apple platforms. Pass `--target aarch64-apple-darwin` or similar to compile for macOS.
       (If you're absolutely certain that you're using GNUStep, you can specify that with the `gnustep-x-y` Cargo feature instead).
   --> C:\Users\M.Pertsev\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\objc2-0.6.4\src\lib.rs:219:1
    |
219 | compile_error!("`objc2` only works on Apple platforms. Pass `--target aarch64-apple-darwin` or similar to compile for macOS.\n(If you're absolutely certain that you're using GNUStep, you can specify that with the `gnustep-x-y` Cargo feature instead).");

error: could not compile `objc2` (lib) due to 1 previous error
```

VERBATIM ERROR — floem-app default `app-stderr.log` (identical panic site in floem-board/
dash/fetch/grid/tray defaults; floem-tray first logged `theme-changed: dark`, then died the
same way):

```
thread 'main' (28992) panicked at C:\Users\M.Pertsev\.cargo\git\checkouts\floem-ab9be4e01bb293da\778bb5f\src\app\handle.rs:100:71:
called `Result::unwrap()` on an `Err` value: SurfaceCreationError(CreateSurfaceError { inner: Hal(FailedToCreateSurfaceForAnyBackend({})) })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

FINDINGS / FRICTION:
- Worst default-launch record of the wgpu frameworks: ALL 6 buildable floem apps died on
  the default Vulkan surface path in Phase A (egui/xilem each had 3 survivors). All 6 were
  rescued by `WGPU_BACKEND=dx12` (6/6).
- Yet floem is the ONLY framework in the campaign whose selftests hit the canonical done
  markers: `SELFTEST DONE pass=14 fail=0` (grid) and `SELFTEST DONE pass=10 fail=0` (fetch),
  both with clean exit=0 — 2 of the 2 canonical selftest passes recorded on Windows belong
  to floem, running on the default env (a further intermittency data point on top of the
  Phase C floem-dash run).
- floem-babel and floem-peek cannot build on Windows at this git rev: the objc2
  compile_error — an Apple-only dependency branch reached from the babel/peek feature set —
  fails the build before any floem code compiles.
- floem-tray survived the Win+Shift+9 hotkey failure gracefully (dual-muda behavior noted
  in the runbook), detected the dark theme, but its screenshot capability failed with
  `screenshot: FAILED: program not found` — the app shells out to an external capture
  program that doesn't exist on Windows, so no floem tray-shot exists.
- Heaviest native binaries that actually ran: 18.1–20.3 MB exe + 7.2–8.1 MB PDB, and the
  longest build times after gpui (127–135 s).

## Cross-cutting

- **Build failures: 28 of 80 apps** (35%) failed `cargo build --release` as-is on Windows —
  the three all-fail frameworks tauri (8/8, missing icons/icon.ico), freya (8/8,
  freya-skia-bindings download+LLVM), vizia (8/8, LNK1120 `__std_*` vs prebuilt Skia), plus
  dioxus-peek, floem-babel, floem-peek (objc2) and gpui-peek (core-foundation). Note: all
  three all-fail frameworks fail on PACKAGING/DISTRIBUTION infrastructure (icon resource,
  prebuilt-binary supply chain, C++ ABI/STL mismatch), not on Rust code.
- **wgpu Vulkan surface flakiness** — `FailedToCreateSurfaceForAnyBackend` on the AMD
  Radeon 890M (driver 32.0.13058.2) is intermittent, with four independent data points:
  1. Phase A same-framework divergence: egui-app/board/dash and xilem-app/board/dash ran
     fine while egui-babel/fetch/grid/peek and xilem-babel/fetch/grid/peek died at launch
     in the same serial session, on identical stacks.
  2. Phase C: floem-dash — dead on default in Phase A — completed the full 5 s + 30×1 s
     runtime sampling on the default env (runtime-sample.ps1 sets no WGPU_BACKEND), with
     empty stderr and 1.0% average CPU.
  3. Phase D: egui-fetch — dead on default in Phase A — ran its complete selftest to
     `SELFTEST_DONE` on the default env; egui-grid ran partway; egui-babel/peek stayed
     alive to timeout.
  4. Phase D: xilem-grid/fetch/babel all launched on the default env (vello INFO line,
     BUILD_MS) despite Phase A default deaths; floem-grid/fetch went further and PASSED
     their canonical selftests on the default env.
  Ladder outcome: 12 defaults died on the surface error (egui×4, xilem×4 incl. peek,
  floem×6 minus the 2 hotkey deaths = egui-babel/fetch/grid/peek, xilem-babel/fetch/grid/
  peek, floem-app/board/dash/fetch/grid/tray — 14 died total, 12 surface + 2 hotkey);
  `WGPU_BACKEND=dx12` rescued 12/12 surface deaths. `WGPU_BACKEND=gl` was reached only by
  the 2 hotkey deaths (egui-tray, xilem-tray) and could not help — their panic is not
  graphics. iced, also wgpu-based, never hit the surface error (0/8).
- **Tray hotkey pre-registration:** every tray app's Cmd+Shift+9 registration —
  `HotKey { mods: Modifiers(SHIFT | SUPER), key: Digit9, id: 570425358 }` — failed with
  `AlreadyRegistered` on this desktop (Win+Shift+digit is an OS-level taskbar shortcut on
  Windows 11). Split outcome: egui-tray and xilem-tray treat it as fatal (panic, exit 101,
  selftest fail); iced/slint/gpui/floem/dioxus-tray log the failure and keep running.
- **Selftest marker heterogeneity:** the harness's canonical marker is
  `SELFTEST DONE pass=N fail=0` + exit 0. Only floem (grid, fetch) matches both. slint
  runs everything, exits 0, but prints underscore `SELFTEST_DONE` → scored `no-marker`.
  egui-fetch, gpui-grid/fetch, dioxus-grid/fetch run everything, print underscore
  `SELFTEST_DONE`, and never exit → scored `timeout`. iced-grid/fetch and the xilem suites
  never armed at all. Babel: only iced produced a PNG (225,057 B) — and still timed out.
  Nothing but floem exits AND says the right words.
- **Runtime CPU semantics:** runtime.csv percentages are PER-CORE (can exceed 100 on this
  24-logical-core machine; same convention as macOS `ps -o %cpu`) — see `runtime-notes.txt`
  for the Win32_PerfFormattedData derivation and the msedgewebview2 helper attribution.
  Measured dashboards, avg%/peak%/maxRSS(MiB)/helpers: floem 1.0/11/327/0,
  iced 1.4/11/220/0, xilem 3.0/17/495/0, gpui 3.3/16/98/0, egui 4.3/11/346/0,
  dioxus 9.2/26/489/6, slint 15.8/38/153/0; tauri/freya/vizia have no numbers (runtime.csv
  says `DIED`, but strictly they never had a binary — build-failed).
- **No `binary_stripped_bytes` on Windows:** MSVC binaries don't carry strippable symbol
  sections; symbols live in the separate PDB, so the harness records `pdb_bytes` instead.
  Cross-platform size comparisons must use exe+PDB or exe alone, stated explicitly.
- **results.csv vs runs/ tree:** results.csv's single workaround column under-reports two
  apps — egui-peek and xilem-peek each also have a passing `wgpu-dx12` variant directory in
  addition to the recorded msmf-manifest workaround. All other values match the per-variant
  result.tsv files exactly.
- **Window evidence:** every alive+window run has a `windows.tsv` row (pid, hwnd, logical
  size, title — e.g. `Babel (iced)` 813×636); died/build-failed runs have empty files.

## Packaging (Phase E)

Run date 2026-08-08, `windows/package-windows.ps1` → 33 rows in
`windows/packaging/results.csv` + per-invocation logs under `windows/packaging/logs/`
(tool-probe logs record the versions: cargo-bundle v0.11.0, cargo-packager 0.11.8,
dx = dioxus 0.7.10 (57d6794), tauri-cli 2.11.4). CSV columns: app, tool, format, status,
artifact_bytes, install_ok, launch_after_install, uninstall_ok, signed, notes. The `app`
apps only — the runbook's 33-row shape (cargo-bundle msi ×10, cargo-packager nsis ×10 +
wix ×10, tauri-cli msi+nsis ×2, dx bundle ×1) held exactly.

**Run provenance / harness-fix history:** Phase E was executed three times. Run 1 was
VOIDED — a Notes-column binding bug in `package-windows.ps1` corrupted its CSV (its
failure classes matched run 2's; nothing from run 1 is quoted anywhere). Run 2 exposed
two harness defects (wrong `msiexec` resolution; wrong out-dir searched for iced — both
detailed below), fixed in `windows/package-windows.ps1` (absolute
`$env:SystemRoot\System32\msiexec.exe`; honoring a pre-existing Packager.toml
`out-dir`). Run 3 (elevated, 2026-08-08, fixes applied) is the on-disk evidence below:
iced-app's wix row flipped to `passed` (the out-dir fix found its MSI), all other
tallies were unchanged — and the msiexec failure REPRODUCED even with the absolute path
(see the caveat below), so the install/launch/uninstall columns were closed by the
supplemental `windows/verify-msi-install.ps1` pass, recorded in
`packaging/install-verify.csv`.

**RESOLVED CAVEAT — `install_ok: no` on the seven wix rows in results.csv is a
HARNESS-SESSION anomaly, not evidence about the MSIs.** In both counted full-harness
runs (2 and 3), `Start-Process` for msiexec threw ERROR_BAD_EXE_FORMAT before the
process ever launched — `This command cannot be run due to the error: %1 is not a Win32
application` (recorded in the CSV in this machine's Russian locale as `%1 не является
приложением Win32`) — in run 3 even with the absolute
`$env:SystemRoot\System32\msiexec.exe` path. The anomaly could NOT be reproduced in an
isolated elevated session: the identical `Start-Process` call (same dev shell, same
redirects, same working directory, a real MSI) installed with exit 0. Root cause
undetermined; recorded honestly as a session-state anomaly of the full harness run.
Resolution: `windows/verify-msi-install.ps1` re-ran silent install → installed-exe 8 s
launch-alive check → silent uninstall for all 7 wix MSIs in an elevated session,
mirroring the harness semantics; its artifact is `packaging/install-verify.csv`:
**7/7 install_ok (exit 0), 6/7 launch_after_install, 7/7 uninstall_ok** — the single
launch `no` is dioxus-app (see the dx section).

### cargo-bundle msi ×10 — 0/10

```yaml
tool: cargo-bundle
version: cargo-bundle v0.11.0
format: msi
rows:
  iced-app:   {status: failed, error_class: component-table}
  egui-app:   {status: failed, error_class: component-table}
  gpui-app:   {status: failed, error_class: component-table}
  tauri-app:  {status: failed, error_class: "phase-A build failure (tauri-build: icons/icon.ico)"}
  xilem-app:  {status: failed, error_class: component-table}
  slint-app:  {status: failed, error_class: component-table}
  dioxus-app: {status: failed, error_class: component-table}
  freya-app:  {status: failed, error_class: "phase-A build failure (freya-skia-bindings v0.98.1)"}
  vizia-app:  {status: failed, error_class: "phase-A build failure (LNK1120)"}
  floem-app:  {status: failed, error_class: component-table}
```

VERBATIM ERROR — all seven buildable apps fail identically (quoted from
`logs/iced-app-cargo-bundle.log`, ANSI color codes stripped — the raw CSV notes field
carries them, e.g. `[1m[31merror:[0m[1m Failed to generate Component table`; the
Caused-by line names each app's own exe):

```
    Finished `release` profile [optimized] target(s) in 0.20s
warning: MSI bundle support is still experimental.
    Bundling Iced Tasks.msi
error: Failed to generate Component table
  Caused by: "iced-app.exe" is not a valid value for column "KeyPath"
```

tauri-app/freya-app/vizia-app never reach the MSI stage — cargo-bundle's preceding
`cargo build` dies on their Phase A errors.

### cargo-packager nsis ×10 — 0/10

```yaml
tool: cargo-packager
version: cargo-packager 0.11.8
format: nsis
rows:
  iced-app:   {status: failed, error_class: makensis-plugin, notes: "committed (macOS-era) Packager.toml"}
  egui-app:   {status: failed, error_class: makensis-plugin, config_created: packager.toml}
  gpui-app:   {status: failed, error_class: makensis-plugin, config_created: packager.toml}
  tauri-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  xilem-app:  {status: failed, error_class: makensis-plugin, config_created: packager.toml}
  slint-app:  {status: failed, error_class: makensis-plugin, config_created: packager.toml}
  dioxus-app: {status: failed, error_class: makensis-plugin, config_created: packager.toml}
  freya-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  vizia-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  floem-app:  {status: failed, error_class: makensis-plugin, config_created: packager.toml}
```

VERBATIM ERROR — all seven buildable apps fail identically in makensis (quoted from
`logs/iced-app-cargo-packager-nsis.log`):

```
 INFO Running makensis.exe to produce C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\iced-app\target\release\packager\iced-app_0.1.0_x64-setup.exe
ERROR Error running makensis.exe: failed to run command: "C:\\Users\\M.Pertsev\\AppData\\Local\\.cargo-packager\\NSIS\\makensis.exe" "-V2" "C:\\Users\\M.Pertsev\\Desktop\\workspace\\OSS\\rust-gui-desktop-ecosystem-state\\apps\\iced-app\\target\\release\\packager\\.cargo-packager\\nsis\\x64\\installer.nsi"
stdout: Plugin directories:
  C:\Users\M.Pertsev\scoop\apps\nsis\current\Plugins\x86-unicode

stderr: Plugin not found, cannot call nsis_tauri_utils::SemverCompare
Error in script "C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\iced-app\target\release\packager\.cargo-packager\nsis\x64\installer.nsi" on line 178 -- aborting creation process
```

This is a genuine plugin-path-leakage fragility finding, not a missing tool:
cargo-packager downloaded its OWN makensis into `%LOCALAPPDATA%\.cargo-packager\NSIS\`,
but that makensis resolves plugins from the machine's scoop NSIS install
(`scoop\apps\nsis\current\Plugins\x86-unicode` — the only entry in its printed plugin
directories), which does not contain `nsis_tauri_utils` — the plugin cargo-packager's own
generated installer.nsi calls at line 178. Vendored compiler + leaked system plugin path
= 0/10 with a full NSIS toolchain present.

### cargo-packager wix ×10 — 7/10 passed (7/7 buildable; installs verified separately)

```yaml
tool: cargo-packager
version: cargo-packager 0.11.8
format: wix
rows:
  iced-app:   {status: passed, artifact_bytes: 4632576, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, notes: "committed (macOS-era) Packager.toml; out-dir honored in run 3", artifact: target\release\packager\iced-app_0.1.0_x64_en-US.msi}
  egui-app:   {status: passed, artifact_bytes: 6094848, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\egui-app_0.1.0_x64_en-US.msi}
  gpui-app:   {status: passed, artifact_bytes: 4501504, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\gpui-app_0.1.0_x64_en-US.msi}
  tauri-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  xilem-app:  {status: passed, artifact_bytes: 5054464, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\xilem-app_0.1.0_x64_en-US.msi}
  slint-app:  {status: passed, artifact_bytes: 7270400, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\slint-app_0.1.0_x64_en-US.msi}
  dioxus-app: {status: passed, artifact_bytes: 2166784, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\dioxus-app_0.1.0_x64_en-US.msi}
  freya-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  vizia-app:  {status: failed, error_class: "no release exe (phase-A build failure) — nothing to package"}
  floem-app:  {status: passed, artifact_bytes: 7028736, install_ok: "no (harness msiexec anomaly — see caveat)", signed: NotSigned, config_created: packager.toml, artifact: target\packager\floem-app_0.1.0_x64_en-US.msi}
```

**Run 2's iced-app `failed` row was a PROVEN FALSE NEGATIVE, corrected in run 3.** Run
2's harness recorded "packager exited 0 but no wix artifact found", while
`logs/iced-app-cargo-packager-wix.log` ends:

```
 INFO Finished packaging 1 package at:
        C:\Users\M.Pertsev\Desktop\workspace\OSS\rust-gui-desktop-ecosystem-state\apps\iced-app\target\release\packager\iced-app_0.1.0_x64_en-US.msi
```

iced-app is the only `app` with a committed Packager.toml (macOS-era), which sets
`out-dir = "target/release/packager"`; the run-2 harness searched only its default
out-dir (`target/packager`) and missed the MSI. The fixed harness honors the toml, and
run 3 records the row as `passed` (4,632,576 B) — the CSV tally and the true tally now
agree at **7/7 buildable apps** (0/3 unbuildable). Second finding from the same toml,
still standing: it declares `formats = ["app", "dmg"]`, and the CLI `--formats wix`
silently overrode it — cargo-packager built a Windows MSI from a macOS-era config
without complaint.

Per-app supplemental install verification (`packaging/install-verify.csv`, elevated
`windows/verify-msi-install.ps1` pass — silent install → 8 s launch-alive check →
silent uninstall, mirroring the harness semantics):

```yaml
tool: verify-msi-install.ps1 (supplemental, cargo-packager wix MSIs)
rows:
  iced-app:   {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\Iced Tasks\iced-app.exe'}
  egui-app:   {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\egui-app\egui-app.exe'}
  gpui-app:   {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\gpui-app\gpui-app.exe'}
  xilem-app:  {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\xilem-app\xilem-app.exe'}
  slint-app:  {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\slint-app\slint-app.exe'}
  dioxus-app: {install_exit: 0, install_ok: yes, launch_after_install: no, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\dioxus-app\dioxus-app.exe'}
  floem-app:  {install_exit: 0, install_ok: yes, launch_after_install: yes, uninstall_exit: 0, uninstall_ok: yes, installed_exe: 'C:\Program Files\floem-app\floem-app.exe'}
```

Tally: **7/7 install_ok (exit 0), 6/7 launch_after_install, 7/7 uninstall_ok**. The
single launch `no` — dioxus-app — matches the dx-installed copy's failure exactly (see
the dx section's cross-installer finding).

### tauri-cli msi+nsis ×2 — 0/2

```yaml
tool: tauri-cli
version: tauri-cli 2.11.4
invocation: cargo tauri build --ci --no-sign --bundles msi,nsis
rows:
  tauri-app/msi:  {status: failed, error_class: "phase-A build failure (tauri-build: icons/icon.ico)"}
  tauri-app/nsis: {status: failed, error_class: "phase-A build failure (tauri-build: icons/icon.ico)"}
```

Same wall as Phase A: `cargo tauri build` recompiles the app, and tauri-build's build
script aborts on `` `icons/icon.ico` not found; required for generating a Windows
Resource file during tauri-build `` (verbatim tail in `logs/tauri-app-tauri.log`) before
either bundler is reached. tauri's own packaging pipeline — its headline feature — was
never exercised on Windows.

### dx bundle ×1 — 1/1 built and installed, but as an NSIS exe, not the expected msi

```yaml
tool: dx
version: dioxus 0.7.10 (57d6794)
invocation: dx bundle --release
rows:
  dioxus-app: {format: nsis, status: passed, artifact_bytes: 185951400, install_ok: yes, launch_after_install: no, uninstall_ok: yes, signed: NotSigned, artifact: target\dx\dioxus-app\bundle\windows\nsis\DioxusApp_0.1.0_x64-setup.exe, installed_exe: C:\Users\M.Pertsev\AppData\Local\Programs\DioxusApp\dioxus-app.exe, notes: "expected msi; dx produced an exe installer instead"}
```

- The runbook table says `dx bundle | msi` — REFUTED: dx ran makensis and emitted
  `DioxusApp_0.1.0_x64-setup.exe`, an NSIS installer, 185,951,400 B (the 5.0 MB Phase A
  exe plus a bundled WebView2 offline installer — ~36× every cargo-packager MSI).
- dx also logged `ERROR 🚫dx and dioxus versions are incompatible! • dx version: 0.7.10 •
  dioxus versions: [0.7.9]` — then bundled anyway (exit 0).
- Full silent cycle: `/S` install exit 0, installed exe found at
  `%LOCALAPPDATA%\Programs\DioxusApp\dioxus-app.exe`, silent uninstall exit 0 — the only
  results.csv row with a completed install/uninstall (the wix rows' cycles are recorded
  in `install-verify.csv`).
- **POST-INSTALL LAUNCH CHECK FAILED** (`launch_after_install: no`), reproduced in both
  counted harness runs (2 and 3), with no error detail in the dx logs — and now a
  **CROSS-INSTALLER finding**: the cargo-packager wix MSI's installed dioxus-app copy
  failed the identical 8 s launch-alive check in the supplemental pass
  (`install-verify.csv`) while the other six installed MSIs passed. Three independent
  reproductions (2× dx NSIS install, 1× WiX MSI install) of installed dioxus-app copies
  failing the launch check, against the same binary running alive+window from
  `target\release` (Phase A). Suspects — unproven: working-directory-relative asset
  resolution or WebView2 bootstrap behavior in the installed location.

FINDINGS / FRICTION:
- Per-tool tallies: cargo-bundle msi **0/10**; cargo-packager nsis **0/10**;
  cargo-packager wix **7/7 buildable** (7 recorded in run 3; 0/3 unbuildable) and — via
  the supplemental pass — **7/7 installed, 6/7 launched, 7/7 uninstalled**; tauri-cli
  **0/2**; dx **1/1** (wrong format vs the runbook; install yes / launch no / uninstall
  yes). Net: two of five routes produce artifacts at all, and every produced artifact
  came from buildable apps' existing release exes.
- The runbook's parenthetical "(cargo-bundle's msi path is historically fragile — that's
  the point)" was confirmed at 0/10 with one uniform error (Component table / KeyPath),
  and its risk #11 ("WiX/NSIS toolchain fragility in cargo-packager — failures are rows")
  materialized entirely on the NSIS side; the WiX side went 7/7.
- Phase A build failures cascade: tauri/freya/vizia contribute 11 of the 33 rows as
  automatic packaging failures (3× cargo-bundle, 3× cargo-packager nsis, 3× cargo-packager
  wix, plus tauri's 2 tauri-cli rows) — no tool was actually exercised on those rows.
- Signing: every produced artifact (7 MSIs + the dx exe) reports **NotSigned** from
  `Get-AuthenticodeSignature` — as designed; no certificate exists in this environment.
  The SmartScreen double-click observation is a manual step and was NOT performed.

## NVDA (Phase F)

Performed 2026-08-08 with NVDA 2026.1.1 (scoop portable copy, ru_RU system locale — role
names are announced in Russian; widget names/labels come through in the apps' English).
Probe: launch the release binary, then a scripted focus walk (Tab ×8, Down ×4, Shift+Tab)
driven by SendKeys while NVDA logs at debug level; the verbatim `Speaking [...]` utterances
are filtered into `measurements/reruns/20260808-ten-framework-tri-platform/windows/nvda/`
(`<app>-speech.log`, one file per app + README.txt with the protocol). Semi-automated, not a
blind-user study; the operator also listened along.

- **egui-app — POSITIVE.** NVDA announced the window (`'Tasks (egui)', 'окно'`), the text
  input as an editor with its placeholder and empty state (`'редактор', 'What needs to be
  done?', … 'пусто'`), and Add as a button (`'Add', 'кнопка'`). Tab cycled editor ↔ Add;
  the focus walk never reached the task-list rows, so whether list items are exposed as
  focusable/announceable elements remains unexercised.
- **slint-app — POSITIVE.** Identical texture: `'Tasks (Slint)', 'окно'`, then
  `'редактор', 'What needs to be done?'` and `'Add', 'кнопка'` alternating across the Tab
  walk. Same task-list caveat as egui.
- **iced-app — NEGATIVE CONTROL CONFIRMED.** NVDA announced only the bare window title
  (`'Tasks (iced)'`) with no role; the entire focus walk produced zero widget
  announcements from inside the app — no editor, no button, no focusable elements. The
  corpus's claim that stable iced ships no accessibility integration is now
  screen-reader-verified on Windows.

Cross-cutting notes: the console window every non-tauri app pops (missing
`windows_subsystem="windows"`) is itself announced by NVDA as a terminal before the real
window — double focus noise a screen-reader user would hit on every launch. The probe's
key presses also leaked into NVDA's own first-run telemetry dialog on the iced run
(answered "no") — visible in that log as noise, kept verbatim. Not probed: xilem, vizia,
freya (two of three don't build here), dialogs, menus, or live-region updates.
