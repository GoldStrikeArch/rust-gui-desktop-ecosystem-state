# WINDOWS-RUN — the Windows measurement campaign runbook

> The whole study so far ran on one macOS M4 Pro plus one headless Linux
> container round; every Windows statement in the corpus is `source-verified`
> only. This runbook executes the full campaign on a real x64 Windows machine
> and feeds the results back into the cohort harness as a first-class
> `windows` arm (validated by `scripts/cohort.py`, rows emitted by
> `scripts/generate-evidence-manifest.py`).
>
> **Machine assumptions:** x64 (`x86_64-pc-windows-msvc`), Windows 11, webcam
> + microphone present, admin rights, ~40 GB free disk (80 crates × separate
> `target/` dirs), internet for the first builds. Time budget: **~2 working
> days** on the Windows machine + ~half a day back on the Mac.

## Ground rules (read first)

1. **Serial always.** Every build, launch, sample, selftest, and packaging
   step runs one app at a time — mixing runs corrupts CPU numbers, the
   fetcher server's `/flaky` counter, and window attribution.
2. **As-is before variant.** The first build/run of every app happens on the
   unmodified tree (`windows-campaign~1`). Expected failures (the eight
   nokhwa-pinned peek apps; gpui-peek forever) are **findings — record them,
   don't fix them.** Workaround variants come after, clearly labeled.
3. **Evidence before workaround.** `windows/environment.txt` is written
   before the first app runs. Every retry lands in its own
   `runs/<app>/<variant>/` directory; nothing is overwritten.
4. **Encoding discipline.** All harness scripts write UTF-8 without BOM with
   LF endings. Do not hand-edit result files in Notepad. PowerShell 7
   (`pwsh`) only — never Windows PowerShell 5.1.
5. **Failures are findings; infrastructure failures are aborts.** An app
   that won't build or paint is data. A missing `result.tsv` (script bug,
   disk full) is an abort — fix and re-run that app before continuing.

## Phase P — prepare (Windows machine, ~1–2 h including installs)

Install, in order:

1. **PowerShell 7**: `winget install Microsoft.PowerShell`
2. **VS 2022 Build Tools** with the "Desktop development with C++" workload
   (MSVC v143 + Windows 11 SDK): `winget install Microsoft.VisualStudio.2022.BuildTools`
   then add the C++ workload via the installer UI.
3. **Rust**: `winget install Rustlang.Rustup`, then
   `rustup toolchain install 1.96.1` (the repo's `rust-toolchain.toml` pins
   it; the harness always invokes `rustup run 1.96.1`).
4. **Python 3.11+**: `winget install Python.Python.3.12`
5. **Git**: `winget install Git.Git`
6. **WebView2 Runtime** — preinstalled on Windows 11; verify in pre-flight.
7. Packaging tools (Phase E only):
   `cargo install cargo-bundle --locked`,
   `cargo install cargo-packager --locked`,
   `cargo install dioxus-cli --locked` (0.7.10 — watch for a Deno `dx`
   shadowing PATH), WiX v3 (`winget install WiXToolset.WiXToolset`),
   NSIS (`winget install NSIS.NSIS`).

Manual system toggles (each is **recorded** by `capture-environment.ps1`):

- Settings → Privacy & security → Camera / Microphone → allow desktop apps.
- Settings → System → For developers → Developer Mode ON.
- **Decide the Defender question deliberately:** adding the repo to
  exclusions (`Add-MpPreference -ExclusionPath <repo>`) stabilizes build
  timings but means timings describe an excluded machine. Either choice is
  valid; **it is captured verbatim in `environment.txt` and must be disclosed
  in the report.**
- Enable long paths:
  `Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -Value 1`

Clone (CRLF discipline is mandatory — CSV/TSV evidence is byte-hashed):

```powershell
git clone -c core.autocrlf=false -c core.longpaths=true <repo-url> gui-ecosystem-research
cd gui-ecosystem-research
git checkout windows-campaign~1   # the pre-nokhwa-split commit: as-is tree
pwsh windows/preflight.ps1        # must be all [ok] before continuing
```

**Abort criteria:** any preflight `[FAIL]`.

## Phase 0 — cohort + environment evidence

The cohort directory is created **on the Mac** (`cohort.py init` uses macOS
system probes and the Mac stays the single `cohort.json` writer):

```bash
# Mac — ALREADY DONE for this campaign; the skeleton is committed:
#   measurements/reruns/20260808-ten-framework-tri-platform/cohort.json
# (a future campaign would: python3 scripts/cohort.py init --id <new-id>)
```

On Windows (after `git pull`):

```powershell
$COHORT = "measurements/reruns/20260808-ten-framework-tri-platform"
pwsh windows/capture-environment.ps1 -Cohort $COHORT
Get-Content "$COHORT/windows/environment.txt" -Head 2   # sanity: machine= + run_date=
```

**Abort criteria:** the `machine=` line has any empty component.

## Phase A — full-80 as-is build + launch + visible-window (~4–8 h)

```powershell
pwsh windows/run-cohort.ps1 -Cohort $COHORT
```

What it does per app, strictly serially: timed locked release build → binary
+ PDB sizes → launch → 10 s alive check → visible-window check (P/Invoke
`EnumWindows`; WebView2 helper processes attributed for tauri/dioxus) → kill
tree → `runs/<app>/default/result.tsv`. On a dead default it walks the
workaround ladder automatically (`ICED_BACKEND=tiny-skia` for iced;
`WGPU_BACKEND=dx12` then `gl` for wgpu frameworks), each retry in its own
variant directory. Ends by aggregating `windows/results.csv`.

**Expected findings (do not "fix"):**

- The 8 nokhwa-pinned peek apps (iced/egui/tauri/xilem/slint/dioxus/vizia/
  floem) fail to build (`input-avfoundation` pinned) — the recorded as-is
  result. `freya-peek` may build as-is: freya's `camera` feature wraps nokhwa
  with the per-target `input-native` backend.
- `gpui-peek` fails permanently (direct AVFoundation; no Windows path).
- Even under the Phase-B msmf variant, apps whose *source* calls
  AVFoundation-only helpers (`nokhwa_check`/`nokhwa_initialize`) may still
  fail to compile — that per-app outcome is data; sources are not patched.
- Any tauri/dioxus app dying without WebView2 would be an environment error —
  preflight rules this out.

**Abort criteria:** any selected app with no `result.tsv` at all (the script
exits 1 and lists them).

## Phase B — nokhwa msmf variant (~1 h)

```powershell
git checkout windows-campaign     # adds the target-gated nokhwa split (input-msmf on Windows)
pwsh windows/run-cohort.ps1 -Cohort $COHORT -Only *-peek -Variant msmf-manifest
```

Re-probes the peek apps under the Windows-gated `input-msmf` feature.
`gpui-peek` stays failed — permanent finding. The aggregator merges these
rows into `windows/results.csv` without touching the other 70.

## Phase C — runtime sampling (~20 min)

```powershell
pwsh windows/runtime-sample.ps1 -Cohort $COHORT
```

10 dashboards × (5 s warmup + 30×1 s samples), CPU/RSS incl. WebView2
helpers, emitting `windows/runtime.csv` + `windows/runtime-samples.csv` in
the exact schema `cohort.py validate_runtime_samples` enforces. CPU-percent
semantics (per-core convention, matching macOS `ps`) are documented in the
script and in `runtime-notes.txt`.

## Phase D — selftests (~1–2 h)

```powershell
pwsh windows/selftest-run.ps1 -Cohort $COHORT
```

Order: **grid** (expect `SELFTEST DONE pass=14 fail=0`) → **fetch** (rebuilds
`tools/fetcher-server` from source — the committed target/ is Mach-O — serves
on `FETCHER_PORT=7878`, strictly serial clients, expect `pass=10 fail=0`) →
**babel** (`BABEL_SELFTEST` + `BABEL_SHOT` PNGs into `windows/babel-shots/`
— this is the DirectWrite/harfrust evidence that closes the report-13 Windows
hole) → **tray** (`TRAY_SELFTEST` + shots; watch for the winrt-notification
and floem dual-muda behaviors) → **peek** (msmf variants; camera/mic consent
prompts may appear on first run — allow them; that first-prompt behavior is
itself worth a note).

## Phase E — packaging head-to-head (~2–4 h)

```powershell
pwsh windows/preflight.ps1 -Packaging   # gates the five tools
pwsh windows/package-windows.ps1 -Cohort $COHORT
```

33 rows into `windows/packaging/results.csv`:

| Tool | Formats | Apps | Rows |
|---|---|---|---|
| cargo-bundle | msi | all 10 | 10 |
| cargo-packager | nsis | all 10 | 10 |
| cargo-packager | wix | all 10 | 10 |
| tauri-cli | msi + nsis | tauri-app | 2 |
| dx bundle | msi | dioxus-app | 1 |

Per row: build installer → silent install → launch installed exe + window
check → uninstall → `Get-AuthenticodeSignature` (expected `NotSigned` — no
cert, by design; SmartScreen behavior on double-click is a **manual
observation** worth one note per tool). Failed rows are findings, not aborts
(cargo-bundle's msi path is historically fragile — that's the point).

## Phase F — stretch: AccessKit UIA reality (manual, ~30 min)

The corpus claims UIA adapters (egui, slint, vizia, freya, xilem) but never
exercised a screen reader. Install NVDA, open `egui-app`, `slint-app`,
`iced-app` (negative control — no a11y): can NVDA read the task list and
buttons? One paragraph of notes per app into the PR description or directly
into `report/data/windows-rows.md` later. No script; honest manual notes.

## Phase G — hand back to the Mac

On Windows:

```powershell
git add "$COHORT/windows" ; git commit -m "windows: campaign results" ; git push
```

On the Mac:

```bash
git pull
python3 scripts/cohort.py record --cohort measurements/reruns/20260808-ten-framework-tri-platform --key windows \
  --artifact measurements/reruns/20260808-ten-framework-tri-platform/windows/results.csv \
  --started <ISO8601Z> --command "pwsh windows/run-cohort.ps1 -Cohort measurements/reruns/20260808-ten-framework-tri-platform"
python3 scripts/cohort.py record --cohort measurements/reruns/20260808-ten-framework-tri-platform --key windows-packaging \
  --artifact measurements/reruns/20260808-ten-framework-tri-platform/windows/packaging/results.csv \
  --started <ISO8601Z> --command "pwsh windows/package-windows.ps1 -Cohort measurements/reruns/20260808-ten-framework-tri-platform"
python3 scripts/generate-evidence-manifest.py --cohort measurements/reruns/20260808-ten-framework-tri-platform
python3 -m unittest scripts.test_cohort_validation   # from scripts/: python3 -m unittest test_cohort_validation
```

Then author, mirroring the Linux pair:

- `report/data/windows-rows.md` — raw per-app rows, verbatim errors,
  environment header (the hand-authored ground truth).
- `report/21-windows-reality-results.md` — the narrative: default render
  paths on real D3D, the DirectWrite babel gallery, WebView2 story,
  tray/toast/hotkey reality, the msmf variant table, packaging head-to-head,
  the gpui-peek no-Windows-path finding, NVDA notes. Scope caveat up top:
  one machine, one GPU/driver is not "Windows".
- Dashboard section + `shell-facade.html` Windows column — after the report
  exists, never before.

## What will probably break (risk-ordered)

1. **9 peek apps fail as-is** — by design; the finding. gpui-peek permanent.
2. **WebView2 Runtime absent/broken** → 16 dead tauri/dioxus apps
   (preflight gates this).
3. **PowerShell 5.1 or CRLF sneaking in** → corrupted evidence bytes
   (preflight gates pwsh 7 + git config).
4. **notify-rust toasts silently absent** — the Windows backend
   (tauri-winrt-notification) needs an AUMID/Start-Menu shortcut that bare
   exes don't have. Mirror of the macOS unbundled-binary finding; record it.
5. **floem's dual muda (0.17 + 0.19)** — the macOS finding is that version
   divergence is load-bearing; watch floem-app/floem-tray menu behavior on
   Win32 closely.
6. **global-hotkey `RegisterHotKey`** — `Cmd+Shift+9` maps to Win/Ctrl
   differently; collisions with reserved Win+ combos possible.
7. **gpui 0.2.2 D3D11 path** — IME "rough", `register_url_scheme`
   unimplemented; a crash or blank window here is a headline result either
   way.
8. **wry `dragDropEnabled`** — the corpus's one Windows behavioral claim
   (apps/tauri-dash/FRICTION.md): HTML5 DnD vs native file drop mutually
   exclusive. Directly testable in tauri-dash/tauri-tray.
9. **MAX_PATH** in deep target dirs (preflight enables long paths).
10. **Defender** quarantining fresh unsigned exes mid-run → timing skew or
    false DIED results (the recorded exclusion decision).
11. **WiX/NSIS toolchain fragility** in cargo-packager — failures are rows.
12. **Perf-counter name collisions** — the sampler matches on `IDProcess`,
    not instance names, for this reason.
13. **wgpu on old GPU drivers** → the dx12→gl ladder exists for this.

## Comparability caveats (bake into the report)

- Build times: different CPU class than the M4 Pro — compare *shapes*
  (relative ranking, incremental vs clean), never absolute seconds.
- `binary_stripped_bytes` does not exist on Windows: MSVC puts symbols in
  PDBs, so `binary_bytes` ≈ stripped already; the harness records
  `pdb_bytes` separately.
- CPU% semantics are documented per-run in `runtime-notes.txt`; RSS is
  `WorkingSet64`, which is close to but not identical to macOS RSS.
- One GPU + driver ≠ Windows. Say so, the way report/18 says a container ≠
  a Linux desktop.
