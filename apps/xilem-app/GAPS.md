# GAPS — xilem-app (xilem 0.4.0, latest crates.io release as of 2026-07-07)

## Spec coverage: complete

All seven functional requirements were expressible in xilem 0.4.0 with stock
views — no fallback approximations were needed:

| Requirement | How |
| --- | --- |
| Window "Tasks (xilem)", ~480×640, resizable | `WindowOptions::new(..).with_initial_inner_size(LogicalSize::new(480.0, 640.0)).with_resizable(true)` |
| Text input + placeholder | `text_input(..).placeholder("What needs to be done?")` |
| Add button | `text_button("Add", ..)` |
| Enter-to-submit | `.insert_newline(InsertNewline::Never).on_enter(..)` — first-class API |
| Per-row Delete button | closure capturing row index; view tree rebuilt after each mutation |
| Live `N task(s)` counter | plain `label`, recomputed each rebuild |
| Scrolling | `portal(flex_col(rows))` |

This is unsurprising: the upstream `to_do_mvc` example in the xilem repo is
almost exactly this app, so the spec sits squarely inside xilem's best-tested
path.

## Caveats and observations (research data)

1. **Version skew inside the release.** xilem 0.4.0 (2025-10-29) pins
   vello 0.6.0 and parley 0.6.0, while standalone vello is at 0.9.0
   (2026-05-15) and parley at 0.11.0 (2026-06-26). ~8 months of renderer and
   text improvements (incl. the sparse-strips/`imaging` migration described in
   Linebender's Q1 2026 post) are only available on xilem git main.
2. **No keyed/identity list view in 0.4.0.** Rows are diffed positionally;
   delete-by-captured-index is correct here only because `app_logic` re-runs
   after every mutation. Fine at this scale, but a stable-identity list (or
   `virtual_scroll`, which exists but wants uniform async-loaded rows) would be
   needed for large dynamic lists.
3. **API/docs mismatch friction.** `FlexSpacer::Fixed` takes a
   `Length`, not `f64` (older examples floating around show `f64`). One
   compile-fix iteration; error message pointed at the exact enum definition.
4. **Build warning:** `block v0.1.6` (via `copypasta` → objc bindings) triggers
   a future-incompatibility warning on rustc 1.96.1. Cosmetic today.
5. **Duplicate deps in tree:** two `skrifa` versions (0.37.0 via parley 0.6,
   0.42.1 via vello 0.6) — symptom of the same internal version skew.
6. **Runtime:** launched cleanly on macOS 26.5 (M4 Pro); zero runtime warnings
   on stdout/stderr over a 10 s run. Window renders with correct title, size,
   placeholder, button, counter and empty-state label. This was manually
   observed; the original SPEC-1 screenshot artifact was not retained.
7. **Interaction verification limit (environment, not framework):** macOS
   denied synthetic keystrokes to the sandboxed shell ("osascript is not
   allowed to send keystrokes", error 1002), so Enter-to-submit and button
   clicks were verified by code path (identical to upstream `to_do_mvc`) and
   visual launch check rather than scripted UI automation.
8. **Styling defaults:** dark theme only out of the box; no attempt made to
   restyle (per spec non-goals). Masonry 0.4's styling-property system exists
   but is young.

## Feature flags

Default `xilem` features only. Nothing in the spec required more.

## Measurements (Apple M4 Pro, macOS 26.5.2, rustc 1.96.1)

- Canonical clean `cargo build --release`: **28 s**; no-op incremental build:
  **1 s**. The earlier ~98 s observation was not the controlled serial result.
- Binary size: **11,944,000 bytes raw**; **10,217,064 bytes (9.7 MiB)
  stripped**.
- `deps-flat.txt`: **143 unique crate names / 154 name-version entries
  including the app**.
- App source: 81 lines, single `src/main.rs`.
