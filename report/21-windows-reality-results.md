# Windows reality check: the macOS-built apps on real Windows 11 (Zenbook S16, Ryzen AI 9 HX 370)

**Run date:** 2026-08-08. **Environment** (full manifest:
`measurements/reruns/20260808-ten-framework-tri-platform/windows/environment.txt`),
verbatim machine line:
`machine=AMD Ryzen AI 9 HX 370 w/ Radeon 890M; 31 GiB; Windows 11 Home 25H2 (build 26200.7171); rustc/cargo 1.96.1; AMD Radeon(TM) 890M Graphics driver 32.0.13058.2`.
Builds ran under a VS 2022 dev shell pinned to MSVC toolset **14.34.31933**
(`link.exe` 14.34.31948.0 in the manifest) because the machine's default
14.44.35207 toolset is an incomplete stub with no cl.exe/link.exe
(operator-observed; the stub left no artifact) — this pin is load-bearing for
the vizia linker findings below. Primary display at `[dpi_scale_primary]` =
`200% (192 DPI)` — every window observation happened at 2× scale. WebView2
Runtime `pv=151.0.4129.59` (preflight-verified). Windows Defender's exclusion
state was **unqueryable** (`[defender_exclusions]` records only `unavailable`;
`Get-MpPreference` threw), so whether builds were real-time-scanned is
unknown. Canonical artifacts, all under
`measurements/reruns/20260808-ten-framework-tri-platform/windows/`:
`results.csv` (80-row aggregate), `runs/<app>/<variant>/` (108 variant runs:
80 default + 10 msmf-manifest + 16 wgpu-dx12 + 2 wgpu-gl), `runtime.csv` +
`runtime-samples.csv` + `runtime-notes.txt` (Phase C), `selftests/results.csv`
+ logs (Phase D), `babel-shots/`, `tray-shots/`; raw agent rows:
[data/windows-rows.md](data/windows-rows.md).

**Scope caveat up front:** one laptop, one integrated GPU and driver (Radeon
890M, 32.0.13058.2), one live desktop session is not "Windows". It CAN prove
compile/link status under MSVC 14.34, default-render-path behavior on this
AMD driver, workaround-ladder outcomes, window liveness at 200% DPI, and
runtime/selftest behavior for the apps that ran, and — via the supplemental
verification pass — install/launch/uninstall behavior of the seven WiX MSIs
on this machine. It CANNOT prove behavior on other GPUs/drivers (NVIDIA/Intel
untested), a complete 14.4x toolset, a clean desktop without this session's
hotkey registrations, Defender-mediated timing effects, or anything about
screen readers (Phase F pending).

## Headline 1 — compilation is the Windows fault line, and it is infrastructure, not Rust

**28 of 80 apps fail `cargo build --release` as-is** (vs 0 of 11 on Linux in
report/18) — including **three entire frameworks at 0/8**: tauri (the
`tauri-build` build script hard-fails on a missing `icons/icon.ico` before
any app code compiles), freya (`freya-skia-bindings` can't download its
prebuilt Skia — curl(3), malformed/missing URL for the Windows feature set —
and the source fallback panics without LLVM/clang-cl), and vizia (all Rust
compiles; every link dies LNK1120 on 5 unresolved `__std_*` externals in
skia-safe 0.93's prebuilt skia.lib — STL helpers added after toolset 14.34).
All three fall on packaging/distribution infrastructure — an icon resource, a
prebuilt-binary supply chain, a C++ ABI/STL mismatch — not on framework code.
Meanwhile the runbook's headline prediction ("the 8 nokhwa-pinned peek apps
fail to build") mostly **did NOT happen**: iced-peek and slint-peek built and
ran as-is, egui-peek and xilem-peek built as-is (their deaths were GPU, not
build), and the msmf-manifest rebuild then put all 4 buildable frameworks'
camera apps on screen. The peeks that did fail (gpui, dioxus, floem, plus the
tauri/freya/vizia framework-wide outages) failed on Apple-only dependencies
(`core-foundation` E0433, `objc2` compile_error) — never on nokhwa itself.

| Framework | Compiles | Default runs | Default path | Fix |
|---|---|---|---|---|
| iced | ✓ 8/8 | ✓ 8/8 | wgpu — zero surface errors; peek builds as-is | — (ladder never fired) |
| egui | ✓ 8/8 | 3/8 | wgpu/Vulkan: 4× `FailedToCreateSurfaceForAnyBackend` (clean exit 1); 1× hotkey **panic** | `WGPU_BACKEND=dx12` 4/4; tray unrescuable |
| gpui | 7/8¹ | ✓ 7/7 | in-house Direct3D 11 — no wgpu, zero flakiness | — |
| tauri | **✗ 0/8** | — | `icons/icon.ico` missing → build-script abort, ~69–82 s each | untried; fix would be one .ico (sources deliberately not patched) |
| xilem | ✓ 8/8 | 3/8 | 4× surface `.unwrap()` panic in masonry_winit; 1× hotkey **panic** | `WGPU_BACKEND=dx12` 4/4; tray unrescuable |
| slint | ✓ 8/8 | ✓ 8/8 | FemtoVG/GL — sidesteps the Vulkan lottery; peek builds as-is | — |
| dioxus | 7/8² | ✓ 7/7 | WebView2 151.0.4129.59, plain | — |
| freya | **✗ 0/8** | — | prebuilt-Skia download curl(3) + LLVM-less source fallback panic | would require installing LLVM — a demand no other framework makes |
| vizia | **✗ 0/8** | — | LNK1120: 5 unresolved `__std_*` vs prebuilt skia.lib under the 14.34 STL | no cargo knob exists; needs a complete newer toolset or source Skia |
| floem | 6/8² | **0/6** | every buildable app died on the surface `.unwrap()` | `WGPU_BACKEND=dx12` 6/6 |

¹ gpui-peek is the confirmed **permanent no-Windows-path** finding:
`core-foundation` fails E0433/E0432 (`std::os::unix`, `libc::PATH_MAX`) under
default AND msmf variants — no camera route exists.
² dioxus-peek, floem-babel, floem-peek all die on objc2's compile_error
("`objc2` only works on Apple platforms") — Apple-only branches reached from
their feature sets.

Aggregate: **36 alive+window / 28 build-failed / 16 died / 0 alive-no-window**
on the default env. iced and slint are the only perfect 8/8-build, 8/8-run
frameworks. Build-time shape: iced's core apps are the fastest clean builds
(~75–79 s), gpui the slowest (162–172 s, every app); the tauri/freya/vizia
failures still burn 42–82 s each because the dependency graph compiles before
the fatal step.

## Headline 2 — the default GPU path is flaky, not broken; the two real deaths are error-handling culture

Of the 16 default-env launch deaths, **14 are one identical error** —
`FailedToCreateSurfaceForAnyBackend` from wgpu's Vulkan surface creation
(egui-babel/fetch/grid/peek, xilem-babel/fetch/grid/peek,
floem-app/board/dash/fetch/grid/tray) — and **`WGPU_BACKEND=dx12` rescued all
14** (12 in results.csv's workaround column; egui-peek and xilem-peek carry
passing `wgpu-dx12` variant directories in `runs/` beyond their recorded
msmf-manifest workaround). But died-on-default is not a deterministic verdict:
the failure is **intermittent**, on four independent data points. (1) Phase A
itself: egui-app/board/dash and xilem-app/board/dash ran fine in the same
serial session, identical stacks, while their siblings died. (2) Phase C:
floem-dash — dead on default in Phase A — survived the full 5 s + 30×1 s
runtime sampling on the default env with empty stderr. (3) Phase D:
egui-fetch ran its complete selftest to `SELFTEST_DONE` on the default env;
egui-grid ran partway. (4) Phase D: xilem-babel/fetch/grid all launched on
default, and floem-grid/fetch went further and **passed their canonical
selftests on the default env**. iced, also wgpu-based, hit the error 0/8
times in the same sessions.

The 2 unrescued deaths are **not graphics at all**: egui-tray and xilem-tray
panic in `global-hotkey` registering Cmd+Shift+9 —
`AlreadyRegistered(HotKey { mods: Modifiers(SHIFT | SUPER), key: Digit9 … })`
— because Win+Shift+9 was already held on this desktop (Win+Shift+digit is an
OS taskbar shortcut on Windows 11). No backend variant can help; the panic
precedes rendering. All 7 built tray apps hit the same registration failure;
the split is culture, not capability: **egui-tray and xilem-tray `.unwrap()`
and die; iced/gpui/slint/floem/dioxus-tray log the failure and keep running**
— iced-tray went on to capture the campaign's only tray screenshot
(`tray-shots/iced-tray.png`) and detect the dark theme. The same split shows
inside the surface deaths: eframe returns the error and exits 1;
masonry_winit and floem `.unwrap()` and panic with exit 101.

## Headline 3 — everything runs, almost nothing can prove it: selftest and runtime reality

Of 50 selftest rows, exactly **2 hit the canonical
`SELFTEST DONE pass=N fail=0` marker with a clean exit — both floem**
(grid 14/14, fetch 10/10, exit 0, on the default env). Nearly everything else
*does the work but can't say so*: slint-grid/fetch ran their complete pass
sequences, saved .ppm snapshots, and exited 0 — but print underscore
`SELFTEST_DONE` with no counts (scored `no-marker`); egui-fetch,
gpui-grid/fetch, and dioxus-grid/fetch ran full sequences with visible
timings, printed underscore `SELFTEST_DONE`, and never exited (scored
`timeout`); iced-grid/fetch and the xilem suites never armed at all. The
selftest wiring is macOS-shaped — 25 of 50 rows are timeouts that mostly
reflect marker/exit heterogeneity, not broken apps.

The exception that delivered: **iced-babel produced the campaign's only babel
PNG** (`babel-shots/iced-babel.png`, 225,057 B) — the first Windows
text-rendering evidence in the corpus — plus a genuine defect reproduction:
its caret probe reported grapheme columns `[0, 1, 26, 27]`, but backspace
over the ZWJ family emoji yielded `(len 7) -> CORRUPTED/partial` — one
Backspace splits the cluster and leaves a dangling partial sequence, exactly
report/13's macOS finding, now confirmed on Windows (the paste round-trip
preserved the string). Every other babel app stayed alive but wrote no PNG.

Runtime (Phase C, 7 measured dashboards; per-core CPU convention): slint-dash
is the CPU outlier at **15.8% avg / 38% peak** vs 1.0–9.2% avg for everything
else — consistent with the macOS re-tessellation finding. gpui-dash is the
memory floor at **98 MiB** max RSS vs xilem-dash's 495 MiB and dioxus-dash's
489 MiB ceiling; dioxus-dash carried **6 stable msedgewebview2.exe helper
processes** (smallest binaries of the campaign at 5.0–6.5 MB exe, paid back
in helpers). And **notify-rust toasts WORK from a bare unsigned exe**:
dioxus-tray logged `[tray-notes] notification posted via notify-rust` —
directly refuting WINDOWS-RUN.md risk #4 (toasts "silently absent" without an
AUMID/Start-Menu shortcut).

## Packaging head-to-head

Phase E ran all 33 planned rows (`windows/packaging/results.csv` + logs;
per-row detail in [data/windows-rows.md](data/windows-rows.md)) — the
runbook's 33-row shape held exactly; one of its expectations did not (see
below). Of five packaging routes, **two produced artifacts**: cargo-packager's
WiX path (an MSI from every app that builds) and dx bundle (an NSIS exe). The
other three died uniformly on tool infrastructure, one error class per tool.

| Tool | Format | Built | Installed | Notes |
|---|---|---|---|---|
| cargo-bundle 0.11.0 | msi | **0/10** | — | all 7 buildable apps: `error: Failed to generate Component table` (`"<app>.exe" is not a valid value for column "KeyPath"`); 3 blocked by Phase A build failures |
| cargo-packager 0.11.8 | nsis | **0/10** | — | all 7 buildable apps: `Plugin not found, cannot call nsis_tauri_utils::SemverCompare` — plugin-path leakage, see below |
| cargo-packager 0.11.8 | wix | **7/7 buildable**¹ | **7/7** (launched 6/7, uninstalled 7/7)² | 2.2–7.3 MB MSIs; 0/3 unbuildable (tauri/freya/vizia have no exe to package) |
| tauri-cli 2.11.4 | msi + nsis | **0/2** | — | same `icons/icon.ico` build-script abort as Phase A — tauri's own bundlers never reached |
| dx (dioxus 0.7.10) | nsis exe³ | **1/1** | yes (silent) | 185.9 MB exe (bundled WebView2 offline installer); silent install + uninstall exit 0; **installed app failed the launch check** |

¹ The run-3 CSV records all 7 passed directly. Run 2 had scored iced-app as
a **proven false negative** — its committed macOS-era Packager.toml sets
`out-dir = "target/release/packager"` and the run-2 harness searched only the
default out-dir; the fixed harness honors the toml, and run 3 found the
4,632,576-byte MSI. Same toml, second finding, still standing: the CLI
`--formats wix` silently overrode its `formats = ["app", "dmg"]` —
cargo-packager built a Windows MSI from a macOS-era config.
² Verified by the supplemental pass recorded in
`windows/packaging/install-verify.csv` (`windows/verify-msi-install.ps1`,
elevated, mirroring the harness's silent install → installed-exe 8 s
launch-alive check → silent uninstall semantics): all 7 MSIs installed with
exit 0 and uninstalled with exit 0; 6/7 installed exes passed the launch
check — the single `no` is dioxus-app (see below). The harness's own install
step still failed inside the full run — see the session-state anomaly
paragraph below.
³ The runbook table says `dx bundle | msi` — **refuted**: dx ran makensis and
emitted `DioxusApp_0.1.0_x64-setup.exe`, and also warned `dx and dioxus
versions are incompatible!` (0.7.10 vs 0.7.9) before bundling anyway.

The two failure classes at 0/10 are both distribution-infrastructure, echoing
Headline 1. cargo-bundle's experimental MSI path fails before WiX enters the
picture — the same Component-table/KeyPath error for every framework, exactly
the historical fragility the runbook set it up to demonstrate. cargo-packager's
NSIS path is subtler and the genuine finding of the phase: it downloads its
**own** makensis into `%LOCALAPPDATA%\.cargo-packager\NSIS\`, but that makensis
resolves plugins from the machine's scoop NSIS install's plugin directory —
which lacks `nsis_tauri_utils`, the plugin cargo-packager's own generated
installer.nsi calls (line 178). A vendored compiler leaking the system's
plugin path turns a fully present NSIS toolchain into 0/10.

One harness defect survived every fix and deserves an honest record: in run
3, `Start-Process` on the **absolute** `$env:SystemRoot\System32\msiexec.exe`
still threw ERROR_BAD_EXE_FORMAT (`%1 is not a Win32 application`) before any
installer launched — yet the identical call (same dev shell, same redirects,
same working directory, a real MSI) could **not** be made to fail in an
isolated elevated session: it installed with exit 0. The root cause remains
undetermined; it is recorded as a session-state anomaly of the full harness
run, and the install/launch/uninstall columns were closed via the
supplemental `windows/verify-msi-install.ps1` pass instead
(`install-verify.csv`).

The dioxus installed-copy launch failure is now a **cross-installer
finding**. The dx NSIS cycle completed (silent install exit 0, installed exe
at `%LOCALAPPDATA%\Programs\DioxusApp\dioxus-app.exe`, silent uninstall exit
0) but its installed app failed the 8 s launch-alive check in both counted
harness runs, with no diagnostic in the dx logs — and the cargo-packager
**MSI** install of dioxus-app failed the identical check in the supplemental
pass, while all six other installed MSIs passed. Three independent
reproductions (2× dx NSIS install, 1× WiX MSI install) of installed
dioxus-app copies dying within 8 s, against the same binary running
alive+window from `target\release` (Phase A). Suspects — unproven:
working-directory-relative asset resolution or WebView2 bootstrap behavior in
the installed location.

Signing and SmartScreen: every produced artifact — the seven MSIs and the dx
exe — reports **NotSigned** from `Get-AuthenticodeSignature`, as designed (no
certificate exists here). The manual SmartScreen double-click observation was
**not performed**. Phase E evidence provenance: run 1 was voided by a harness
Notes-binding bug; run 2 exposed the msiexec and iced out-dir harness
defects; run 3 (elevated, fixes applied) is the on-disk `results.csv` quoted
here, and `install-verify.csv` from the supplemental elevated pass is the
evidence for the install, launch and uninstall columns.

## What this changes in earlier reports

- `dashboard.html`'s footer caveat — "Windows claims remain source-verified
  rather than locally tested — a full Windows campaign … is prepared and
  pending execution" — is discharged by this report. The measured reality it
  should now reflect: 36 of 80 apps reach a window as-is; the corpus's
  source-verified Windows columns said nothing about a 35% build-failure rate
  concentrated in packaging infrastructure.
- `report/13-text-i18n-results.md` is titled and scoped "(macOS)"; its
  Windows column was a hole. It now has its first measured Windows evidence —
  **iced only**: the babel gallery PNG (`babel-shots/iced-babel.png`) and a
  Windows reproduction of the editing matrix's iced row (backspace over the
  ZWJ family splits the cluster: `(len 7) -> CORRUPTED/partial`). The other
  nine frameworks' Windows text cells remain open (no PNGs produced).
- gpui's Windows reputation — `report/03-gpui.md`'s source-verified in-house
  D3D11 + DirectWrite column and `dashboard.html`'s gpui IME chip note
  "windows rough" — now has first measurements: 7/8 apps alive+window, zero
  render-path flakiness (its in-house D3D11 never touches the wgpu/Vulkan
  lottery), and the leanest dashboard of the round at 98 MiB. Better than
  "rough" for these workloads — with two hard scope limits: gpui-peek is a
  permanent no-Windows-path (core-foundation), and IME itself (the chip's
  actual subject) was not exercised.
- `WINDOWS-RUN.md`'s own expectations were wrong in both directions. Predicted
  and did not happen: "the 8 nokhwa-pinned peek apps fail to build" (4 of
  them built; 2 ran as-is), "freya-peek may build as-is" (dead in
  freya-skia-bindings before camera code was reached), risk #2's 16 dead
  WebView2 apps (preflight held; dioxus went 7/7), risk #4's silent
  notify-rust failure (a toast was posted), risk #7's gpui "crash or blank
  window" (the opposite). Not predicted and happened: tauri 0/8 on a missing
  .ico — the framework whose Windows story is its headline never reached
  WebView2 at all.

## Caveats

Single machine, single integrated GPU/driver (Radeon 890M 32.0.13058.2),
single live desktop session, 200% DPI — none of it generalizes to "Windows".
The MSVC 14.34 toolset pin is load-bearing: vizia's LNK1120 verdict might
differ under a complete 14.4x toolset (the STL symbols it lacks are exactly
post-14.34), and the machine's stub 14.44 install is operator-observed, not
artifact-backed. Defender's exclusion state was unqueryable, so all build
timings carry unknown scan overhead and compare to the M4 Pro numbers as
shapes only. `binary_stripped_bytes` does not exist on Windows — `pdb_bytes`
is recorded instead, so cross-platform size comparisons must state exe vs
exe+PDB. CPU percentages are per-core (documented in `runtime-notes.txt`) and
RSS is `WorkingSet64` — comparable to, but not identical with, macOS RSS.
wgpu's surface flakiness means every died-on-default row is a lower bound on
that binary's behavior, not a deterministic result; symmetrically, selftest
`timeout` rows reflect marker/exit heterogeneity, not necessarily broken
apps. Camera/mic consent prompts were never observed — the msmf peek runs
produced no evidence either way on first-run consent UX. The
Win+Shift+9 hotkey conflict is desktop-state-dependent: on a desktop without
that registration, egui-tray and xilem-tray would presumably not die, and the
error-handling split would stay invisible. **NVDA/accessibility (Phase F, semi-automated):
with NVDA 2026.1.1, egui-app and slint-app both announce their window, their
text input as an editor with placeholder ("What needs to be done?") and empty
state, and Add as a button — the AccessKit→UIA path works for real; iced-app,
the negative control, yields only a bare window title and zero widget
announcements across the whole focus walk. Task-list rows were never reached
by Tab in any app, so item-level exposure remains unexercised; speech logs in
`windows/nvda/`. The console window most apps pop is announced as a terminal
before the app window — launch noise a screen-reader user hits every time.**
