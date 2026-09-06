# Fixes_3 disposition

Disposition of every item in [`Fixes_3.md`](../../Fixes_3.md) (third-pass
verification audit, 2026-07-10) after independent two-agent verification and
fix application. Verdicts: **CONFIRMED-APPLIED** (audit finding verified,
correction applied), **PARTIAL-APPLIED** (finding partly right; the supported
part applied), **REJECTED** (finding contradicted by retained evidence or the
claimed defect is not present in our files). `dashboard.html` corrections are
handled in a separate pass; rows below note where that applies.

## Must-correct items 1–15

| Item | Verdict | Rationale |
|---|---|---|
| 1. GPUI Linux epoll root cause unsupported | CONFIRMED-APPLIED | Retained probe failed (`awk: function strtonum never defined`, no registrations enumerated); demoted to explicit HYPOTHESIS in `18-linux-reality-results.md` and bracketed correction in `data/linux-rows.md`. Reproduced 2026-07-10 with a working probe (`linux/probes/gpui-epoll-probe2.sh`, artifacts `linux-results/gpui-epoll-probe2-*`): the hypothesis is now REFUTED — the X fd IS in the main polled epoll set (fdinfo `tfd: 8`; 292 epoll_pwaits + 370 reads/6 s) and the proven root cause is elsewhere: every presented frame is a depth-24 PutImage onto gpui's depth-32 ARGB window, rejected BadMatch by Xvfb (51/51 in 6 s), silently swallowed; wording updated in both files. |
| 2. Linux native-failure count wrong (2/5) | CONFIRMED-APPLIED | Now "no usable UI for 3 of 5" (iced panic, xilem abort, gpui black window); gpui Fix cell → "no workaround found in this Xvfb/X11 configuration"; "software Vulkan is one plausible CI configuration". |
| 3. WebKitGTK "workaround folklore obsolete" unsupported | CONFIRMED-APPLIED | Scoped to "verified unnecessary for WebKitGTK 2.50.6 in this Xvfb/llvmpipe environment"; noted current Tauri docs still recommend the workarounds for NVIDIA/driver-conflict setups; applied in `18-linux-reality-results.md` + `data/linux-rows.md`. |
| 4. GPUI 0.2.2 vs Zed-main architectures mixed | PARTIAL-APPLIED | Mixing real in 4 dashboard cells; "every claim" overstated — footer/a11y cells already scoped. Dashboard cells handled in the separate dashboard pass. |
| 5. eframe wgpu default since 0.34, not 0.32 | CONFIRMED-APPLIED | 0.32/0.33 manifests default to Glow; corrected in `00-ecosystem-map.md`, `data/load-bearing-crates.md`, `data/stack-rows.md`. |
| 6. Glow lockfile presence ≠ selectable eframe renderer | CONFIRMED-APPLIED | `02-egui.md` now states the Glow renderer is behind the opt-in `glow` cargo feature; this measured default 0.35 build compiled only the wgpu renderer — lockfile glow 0.17 arrives via wgpu's GL backend. |
| 7. "Dioxus has no DnD support" is false | PARTIAL-APPLIED | Core correction right; the audit's self-contradiction framing not present in our files. Bracketed correction added to `data/interactive-rows.md` (Dioxus 0.7 exposes HTML5 drag events; this implementation chose a mouse-event state machine). |
| 8. Reverse-dep counts contradict (1,272/1,273; 478/479) | CONFIRMED-APPLIED | Raw API responses not retained; unified to "~1,273" (wgpu) and "~479" (rfd) in `00-ecosystem-map.md` and `data/load-bearing-crates.md`. |
| 9. "Handled 100k rows" exceeds Xilem scroll evidence | CONFIRMED-APPLIED | `16-data-grid-results.md` now says "handled a 100k-row model" (Xilem's retained scroll probe covered ~8k rows of range). |
| 10. GPUI widget matrix contradicts deep report | CONFIRMED-APPLIED | Matrix cell now "low-level elements only (Zed `ui` is GPL/unpublished; high-level controls are app-built or third-party, e.g. gpui-component)". |
| 11. Slint kiosk licensing too categorical (+ "only Rust framework") | CONFIRMED-APPLIED | Now "a kiosk/instrument/vehicle deployment MAY be classified as embedded … confirm the specific deployment with Slint" in `06-slint.md` + `data/stack-rows.md`; designer-workflow claim scoped to "only framework in this seven-framework sample". |
| 12. libcosmic a11y caveat stale | CONFIRMED-APPLIED | `00-ecosystem-map.md` appendix now: libcosmic enables `a11y` (iced/a11y + iced_accessibility) in default features — source-verified; real screen-reader completeness unverified. |
| 13. Axum cancellation causality overstated | CONFIRMED-APPLIED | `17-async-network-results.md` now "consistent with axum dropping handlers on client disconnect: superseded searches never reached the server's post-sleep log point". |
| 14. "Only zero-copy video path" too broad | CONFIRMED-APPLIED | `15-media-hardware-results.md` now "the sample's only explicit native-framework API and Rust-visible IOSurface→Metal-texture path". |
| 15. Forced incremental rebuilds mislabeled "no-op" | CONFIRMED-APPLIED | `measure.sh` touches `main.rs` first; reworded to "forced incremental rebuild (after touch main.rs)" in `05-linebender.md`, `06-slint.md`, `07-dioxus.md`. Genuine no-op mentions in `data/iter4-rows.md` retained. |

## Scope/wording items and appendix reconciliation

| Item | Verdict | Rationale |
|---|---|---|
| "Pixel-consistent with macOS" (BiDi) | CONFIRMED-APPLIED | Now "visually/order-consistent with the macOS artifact in the observed run" in `18-linux-reality-results.md`; bracketed correction ("no pixel-diff retained") in `data/linux-rows.md`. |
| Other dashboard-wording bullets (shaping engine, AccessKit contract date, Dioxus occlusion rate, quarantine vs spctl, non-webview scope, renderer description, a11y recommendations, version-skew cost, todo-spec coverage, Tauri incremental cause, shell viability, "entirely in render/shell", fontique version) | PARTIAL-APPLIED | Dashboard-scoped; handled in the separate dashboard pass. Report files already carry scoped wording for these (e.g. `data/stack-rows.md` scopes HarfRust convergence to "the principal reusable pure-Rust stacks"). |
| stack-rows: Tauri icons "required" | CONFIRMED-APPLIED | Bracketed correction: n=1 observation — this Tauri version/config demanded the configured RGBA icon paths; not universal. |
| stack-rows: "a11y cost zero lines" | CONFIRMED-APPLIED | Now: a11y plumbing was free; semantic labels/ARIA were still authored. |
| stack-rows: "accessible iced exists only in System76's fork" | CONFIRMED-APPLIED | Now "the only shipping integration verified by this audit is System76's fork". |
| stack-rows: "pushes serious users off stable" | CONFIRMED-APPLIED | Now "the three largest observed projects track master or forks (Sniffnet ships stable 0.14)". |
| stack-rows: "~0% CPU idle" | CONFIRMED-APPLIED | Appended "[not retained as a controlled measurement]". |
| stack-rows: "bidi/RTL still unimplemented"/"no BiDi" | CONFIRMED-APPLIED | Now "no paragraph-level BiDi reordering (individual RTL runs shape)" (both occurrences). |
| stack-rows: Slint AccessKit default unscoped | CONFIRMED-APPLIED | Appended "(Winit desktop path; Qt/custom backends not covered)" at both locations. |
| stack-rows: Floem/Bevy/Slint "one identical migration" | REJECTED | No such conflation found: the files already differentiate (Bevy 0.19 "switch cosmic-text→Parley", floem leaving cosmic-text, Slint 1.14 "unify on fontique+Parley"). |
| shell-text-rows: "No Slint dialogs" | CONFIRMED-APPLIED | Now "No native file-dialog API in Slint (its `Dialog` is a rendered element)". |
| iter4-rows: GPUI sort range vs retained log | CONFIRMED-APPLIED | Now "10.8–12.2 ms (string), 3.5 ms (f64) in the retained log". |
| linux-rows: epoll + WebKitGTK claims | CONFIRMED-APPLIED | Bracketed audit corrections added (see items 1 and 3). |
| How-to guide: Zed-code-reuse licensing warning | CONFIRMED-APPLIED | `20-how-to-build.md` now notes Zed's first-party UI crates are GPL-3.0-or-later and unpublished; permissive proprietary path = gpui core + own controls or a permissively licensed third-party set like gpui-component. |
| Report 12: muda "cannot attach directly" softening | REJECTED | Retained E0277 probe shows no winit-facing muda attach API exists on Linux (`Menu::init_for_gtk_window` requires `IsA<gtk::Window>`); the existing "can't attach to winit windows" wording is supported at the type level. |
| Runtime CPU "not a stable ranking" | PARTIAL-APPLIED | Audit's fresh low-CPU table contradicted by retained rerun measurements/`runtime-dash-20260710.csv` (within ~1pp of historical, same ordering; raw samples retained); disclaimer added but "unstable ranking" claim rejected. |
| Dashboard UI defects (narrow-viewport tag overflow; `--ink-3` contrast) | PARTIAL-APPLIED | Dashboard-only; handled in the separate dashboard pass. |

The audit's "What verified successfully" and "Evidence and reproducibility
limitations" sections recorded findings only and required no report edits;
the latter's limitations remain accurately disclosed in the affected reports.
