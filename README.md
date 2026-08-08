# Rust Desktop GUI Ecosystem Research

Research for the RCN "Cross-Platform GUI Desktop Apps" initiative
([rcn#46](https://github.com/Rust-Commercial-Network/rcn/issues/46)).
Core ecosystem research and the baseline experiment were collected
**2026-07-07**; later rounds and independent reconciliation continued through
**2026-07-10**. Local measurements used an Apple M4 Pro,
macOS 26.5.2, and rustc 1.96.1.

Two questions drove this work:

1. **Fragmentation**: what exists, what is duplicated between frameworks
   (text, layout, windowing, a11y…), and where is consolidation happening?
2. **The practitioner question**: how do you actually build a cross-platform
   desktop GUI app in Rust today — steps, tech, tradeoffs?

## Read this first

- **[report/00-ecosystem-map.md](report/00-ecosystem-map.md)** — the
  centerpiece: layered crate map, duplication matrix, convergence/divergence
  analysis, 5 consolidation opportunities, a11y verdict. ~85 source links.
- **[report/20-how-to-build.md](report/20-how-to-build.md)** — the decision
  guide (decision tree, paradigm tradeoff ledger, shipping steps).
- **[report/10-empirical-results.md](report/10-empirical-results.md)** — one
  identical app built in 10 frameworks: build times, binary sizes, dep trees,
  overlap analysis.
- `dashboard.html` — shareable one-page summary of all of the above.

## Per-framework research (source-linked)

| Report | Framework | Version tested | Depth |
|---|---|---|---|
| [report/01-iced.md](report/01-iced.md) | iced | 0.14.0 | full deep dive |
| [report/02-egui.md](report/02-egui.md) | egui/eframe | 0.35.0 | full deep dive |
| [report/03-gpui.md](report/03-gpui.md) | gpui (Zed) | 0.2.2 | full deep dive |
| [report/04-tauri.md](report/04-tauri.md) | Tauri | 2.11.5 | full deep dive |
| [report/05-linebender.md](report/05-linebender.md) | xilem / Linebender stack | 0.4.0 | full deep dive |
| [report/06-slint.md](report/06-slint.md) | Slint | 1.17.1 | full deep dive |
| [report/07-dioxus.md](report/07-dioxus.md) | Dioxus (+ Blitz) | 0.7.9 | full deep dive |
| [report/data/stack-rows.md#freya](report/data/stack-rows.md#freya) | Freya | 0.4.0 | cohort-derived² |
| [report/data/stack-rows.md#vizia](report/data/stack-rows.md#vizia) | Vizia | 0.4.0 | cohort-derived² |
| [report/data/stack-rows.md#floem](report/data/stack-rows.md#floem) | Floem | git-778bb5f2¹ | cohort-derived² |

¹ Floem's crates.io release (0.2.0, Nov 2024) is 20 months stale and
API-incompatible with current documentation; upstream recommends `main`, which
cannot be published because it depends on a forked winit. The cohort therefore
pins git rev `778bb5f2aa08429e579ee2e6ac97e84fbf18b618` — a research finding in
itself.

² The three frameworks added in the **2026-08-03 expansion round** were
researched through the cohort itself rather than as standalone upstream deep
dives. Their stack rows are sourced from the 24 app crates (`apps/freya-*`,
`apps/vizia-*`, `apps/floem-*` — manifests, committed `Cargo.lock` /
`deps-flat.txt`, `GAPS.md`, and the eight `FRICTION.md` files per framework),
and they appear on equal footing with the other seven in the ecosystem map,
the duplication matrix, `dashboard.html`, and every measured round
(iter1–iter4, packaging, Linux, Windows). What they do *not* yet have is the
prose-page treatment of `report/01`–`07`: a few upstream-ecosystem fields
(governance, production users, wasm/mobile support, docs quality) are marked
`pending_upstream_research` rather than guessed. A 2026-08-04 follow-up filled
license, maintainer concentration, and download/star counts for Freya.

`report/data/stack-rows.md` holds the raw structured comparison rows returned
by each research agent, one `##` section per framework — including the ten-way
`shared-infra` row set.

## The experiments (80 apps + packaging round)

Eight specs, each implemented as an **independent crate per framework** with
pinned versions — 10 frameworks × 8 specs = 80 apps. The original 56 produced
release binaries, survived the central eight-second launch check on macOS, and
exposed a visible window in the retained 2026-07-10 audit. The 24 apps added in
the **2026-08-03 expansion round** (Freya, Vizia, Floem) build `--locked` on
the same pinned toolchain and pass their per-spec self-tests; their full
launch/window audit lands with the next complete cohort run. Capability-level
interaction evidence varies by app as documented in the FRICTION/GAPS files
and evidence manifest:

- `apps/SPEC.md` — "Tasks" todo app (forms/lists baseline) → `apps/<fw>-app/`
  with `GAPS.md` per app.
- `apps/SPEC-2.md` — "Pulse" live metrics dashboard (drag-reorder grid,
  10–60 Hz live data, hover-tooltip chart, animations) → `apps/<fw>-dash/`.
- `apps/SPEC-3.md` — "Board" kanban (cross-column DnD, drop indicators,
  inline edit, drop animations) → `apps/<fw>-board/`.
- `apps/SPEC-4.md` — "Tray Notes" OS shell integration (tray, global hotkey,
  native menubar, dialogs, clipboard image, Finder drop, notifications,
  live dark mode, multi-window) → `apps/<fw>-tray/`.
- `apps/SPEC-5.md` — "Babel" text/i18n stress (shared multilingual corpus in
  `apps/babel-assets/`, BiDi/CJK/emoji/editing, per-framework
  `screenshot.png`) → `apps/<fw>-babel/`.
  SPEC-2..5 apps carry a `FRICTION.md` rating every capability:
  built-in / assembled / hand-rolled / not-achievable.

- `apps/SPEC-6.md` — "Peek" media/hardware (camera preview + texture-path
  benchmark, mic meter, audio, 200-image gallery, TCC behavior) →
  `apps/<fw>-peek/`.
- `apps/SPEC-7.md` — "Grid" 100k-row table (virtualization, sort,
  filter-as-you-type with self-timed latency, resize, selection) →
  `apps/<fw>-grid/`.
- `apps/SPEC-8.md` — "Fetcher" async/network against the deterministic local
  server in `tools/fetcher-server/` (debounce, stale protection, streamed
  progress, server-verified cancellation) → `apps/<fw>-fetch/`.

Plus a **packaging round**: the todo apps bundled into ad-hoc-signed
`.app` + `.dmg` artifacts under `dist/` —
[report/14-packaging-results.md](report/14-packaging-results.md).
Results for the later rounds:
[report/11-interactive-results.md](report/11-interactive-results.md),
[report/12-shell-integration-results.md](report/12-shell-integration-results.md),
[report/13-text-i18n-results.md](report/13-text-i18n-results.md).
Round-5 (Linux reality check — Docker/Xvfb, software GPU):
[report/18-linux-reality-results.md](report/18-linux-reality-results.md),
environment `measurements/linux-env.txt`, matrix `measurements/results-linux.csv`,
artifacts `linux-results/`, probes `linux/probes/`.
Round-6 (Windows reality check — one x64 machine: AMD Ryzen AI 9 HX 370 /
Radeon 890M, Windows 11 Home 26200.7171, rustc 1.96.1):
[report/21-windows-reality-results.md](report/21-windows-reality-results.md),
raw rows `report/data/windows-rows.md`, environment
`measurements/reruns/20260808-ten-framework-tri-platform/windows/environment.txt`,
matrix `measurements/reruns/20260808-ten-framework-tri-platform/windows/results.csv`
(runtime, selftest and packaging CSVs live beside it).
Round-4 results:
[report/15-media-hardware-results.md](report/15-media-hardware-results.md),
[report/16-data-grid-results.md](report/16-data-grid-results.md),
[report/17-async-network-results.md](report/17-async-network-results.md).

Reproduce the measurements:

```sh
./measure.sh --round iter1      # ten todo apps → timestamped rerun CSV
./measure.sh --round iter2      # dashboard + board → timestamped rerun CSV
./measure.sh --round iter3      # tray + Babel → timestamped rerun CSV
./measure.sh --round iter4      # Peek + Grid + Fetcher → timestamped rerun CSV
./scripts/runtime-sample.sh     # new dashboard CPU/RSS summary + raw-sample sibling
python3 scripts/overlap.py --round iter1  # todo-app dependency overlap
./scripts/verify-iter3.sh       # serial window/self-test evidence for tray+Babel
./scripts/verify-windows.sh     # serial visible-window evidence for all 80 apps
./scripts/sync-benchmark-tables.py --check  # detect report/dashboard drift
./scripts/generate-evidence-manifest.py --check  # verify artifact provenance hashes
```

**Windows campaign:** executed 2026-08-08 — the full Windows-machine run (all
80 apps, runtime sampling, selftests, and the MSI/NSIS/WiX packaging
head-to-head) per the runbook at [WINDOWS-RUN.md](WINDOWS-RUN.md), driven by
the PowerShell harness under `windows/` and recorded into the cohort as the
`windows` artifact arm; results in
[report/21-windows-reality-results.md](report/21-windows-reality-results.md)
(MSI install verification is pending one elevated re-run; the NVDA pass was
not performed).
Note that `scripts/verify-windows.sh` above verifies *visible windows* on
macOS — it is unrelated to Microsoft Windows.

`measure.sh` deliberately refuses non-macOS hosts because its binary-size,
stripping, and launch measurements use BSD/Mach-O/macOS assumptions. It builds
the package-named benchmark target with `--locked`, writes each destination CSV
atomically only after a successful serial pass, defaults to a timestamped path
under `measurements/reruns/`, and resolves custom relative app paths from the
directory where it was invoked. Pass `--output` only when intentionally
selecting another destination. Use the separate Round-5 workflow above for
Linux evidence; Windows evidence comes from the separate serial PowerShell
driver `windows/run-cohort.ps1` (see [WINDOWS-RUN.md](WINDOWS-RUN.md)).

Round CSVs and per-app logs land in `measurements/`. The historical
`runtime.csv` summary predates raw-sample preservation: its published averages
cannot be recomputed from a retained per-second series. Future
`scripts/runtime-sample.sh` runs default to timestamped `runtime-<UTC>.csv`
and `runtime-<UTC>-samples.csv` files; setting `RUNTIME_OUTPUT` selects a
different run-specific destination and sibling. That future output should not
be mistaken for missing historical evidence. The `process_survived_8s` CSV
field is a liveness check, not an interaction assertion; verification levels
are documented in the reports and per-app friction logs and
[measurements/EVIDENCE.md](measurements/EVIDENCE.md).
Numeric rows between `BEGIN/END GENERATED` markers are synchronized from the
immutable round CSVs by `scripts/sync-benchmark-tables.py`; edit the dataset or
generator, not those rows by hand.
Interactive-round analysis:
[report/11-interactive-results.md](report/11-interactive-results.md); primer &
sustainability: [report/30-primer.md](report/30-primer.md),
[report/data/load-bearing-crates.md](report/data/load-bearing-crates.md).

## Headline findings

1. The ecosystem is **converging bottom-up**: HarfRust unified shaping across
   the three principal reusable pure-Rust stacks (cosmic-text + parley +
   epaint, with epaint adopting it in 0.35); wgpu is the convergence point for
   sampled native-GPU stacks (Zed dropped blade on Linux, Feb 2026); AccessKit
   is the dominant shared a11y layer (Iced and Floem remain unintegrated, and
   Zed currently opts out of GPUI's merged path); taffy crossed framework lines
   (GPUI, Blitz, Servo, Bevy, Slint).
2. Still fragmented at the top: **4 text-layout stacks** and at least **6
   framework-side renderer families** targeting a mix of wgpu, platform GPU
   APIs, OpenGL/Skia, and software paths; 3 windowing layers (winit / tao fork /
   gpui); widgets and styling re-implemented everywhere.
3. **These small baseline apps built quickly on the test machine**: the
   original seven clean-built in 22–56 s on the M4 Pro with warm registries and
   empty target directories, then rebuilt in 1–4 s; the 2026-08-03 expansion
   round's measurements are recorded in `measurements/reruns/`. This is not a
   general large-app ranking.
4. The shared foundation has high **recent author concentration**, while its
   support ranges from employers and time-bounded grants through umbrella or
   personal funding to no public source found. Sustainability remains a major
   lever, but funding and the former “bus factor” label are audit inferences rather than
   employment facts.
