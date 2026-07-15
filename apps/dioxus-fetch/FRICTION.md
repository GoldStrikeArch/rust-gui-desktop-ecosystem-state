# FRICTION — dioxus-fetch ("Fetcher", Dioxus 0.7.9 desktop/webview)

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| async_integration | **built-in** | observed | dioxus-desktop runs the VirtualDom on a multi-thread tokio runtime (`enable_all`, launch.rs:119), so plain reqwest futures run directly inside `use_resource`/`spawn` and **signals are written straight from async code** — no channel, no "invoke on UI thread", no bridge anywhere in this app. The whole search pipeline (debounce → HTTP → decode → signal) is one `use_resource`. Big caveat: task servicing is gated on the webview (see "The occlusion freeze" below). |
| http_client_choice | **assembled** | observed | `reqwest 0.12` (default-features off — plain-HTTP localhost, no TLS; `json` for serde_json decode). Dioxus ships/re-exports no HTTP client, but because the runtime is tokio, reqwest is a drop-in; zero adapter code. `Response::chunk()` avoids even needing futures-util for streaming. Per-call `reqwest::get` (fresh client/connection each request) chosen deliberately so cancellation semantics are observable per-connection; a shared pooled `Client` would still close a connection dropped mid-body (source-only: hyper can't reuse a dirty keep-alive conn). |
| debounce_stale | **built-in (mechanism) / self-test** | self-test | One `use_resource`: read `query` (subscribes), `tokio::time::sleep(250ms)`, then GET. Any query write makes use_resource **cancel the old task and start a new one** (dioxus-hooks 0.7.9 use_resource.rs:81 `task.write().cancel()`), so a change during the sleep = debounce (no request ever sent), a change mid-request = real connection cancel. **No sequence guard exists in this app; stale responses are impossible by construction.** Clean-run proof (run-stdout.log + server log): typing "am" then "amber" 100 ms later produced exactly one `SEARCH q="amber" delay=154ms results=20` — no "am" request. Under heavy desktop contention one earlier run overslept the 100 ms gap and did send "am"; its response was discarded (cancelled future) and the final results were still "amber" — stale protection held even when debounce leaked. |
| progress_streaming | **assembled** | self-test | `spawn` + loop over `Response::chunk()`, `received += chunk.len()` written to a signal per 128 KiB chunk; the progress bar is a div whose width renders from that signal; total from Content-Length. Mid-download stdout: `SELFTEST_DL_PROGRESS 2097152 of 8388608` (2.00 of 8.00 MiB at cancel time). Visual bar motion verified by construction (same signal → style binding), not by eye. |
| cancellation_real | **built-in** | **observed** | **THE question answered: yes — dropping the future kills the TCP connection, for both cancellation paths.** Clean-run server log (fetcher-server.log, all five lines mine, uncontaminated window): `ABORT /download after 12/64 chunks` — use_resource dependency change (ct_key 1→2) cancelling a mid-stream download; `ABORT /download after 16/64 chunks` — Cancel-button path (`Task::cancel()`), exactly matching client-side `SELFTEST_DL_PROGRESS 2097152` (= 16 × 131,072 B). Chain: task cancel → future dropped → reqwest Response dropped → hyper closes the connection → server's stream sender fails → ABORT. No UI-only cancellation anywhere. |
| error_retry_ux | **assembled** | self-test (manual retry) / unexercised (auto-backoff) | Error state in a signal (`FlakyPhase::Failed` + message), button relabels to "Retry", attempts counter shown. Self-test drove the manual path 3×: `SELFTEST_FLAKY attempts=1/2 failed HTTP 500`, `attempts=3 success {"attempt":24}` correlating with server `FLAKY attempt=22/23 -> 500, attempt=24 -> 200` (the counter is server-global and shared with other agents on this desktop, hence 22-24 not 1-3). Bonus auto-retry with 300/600/1200 ms backoff is implemented (`auto_flaky`) but **unexercised** — no self-test drove it. |

## The occlusion freeze (the iteration-4 dioxus finding)

Three of six verification runs froze: all async work — tokio timers,
in-flight downloads, the self-test script — stopped, always immediately
after a signal write, resuming never (app killed after 25+ s; process alive,
RSS stable, zero CPU-visible progress, stdout silent). Mechanism
(source-only, dioxus-desktop 0.7.9): after a render produces an edit batch,
`edits_in_progress` (edits.rs:117 — "We don't run the virtual dom while this
is true") gates the poll loop until the **webview** fetches and applies the
edits (webview.rs:562 "If we're waiting for a render, wait for it to finish
before we continue"). Shell-launched apps get no macOS activation, so the
window can sit occluded behind other windows → WKWebView throttles its JS →
the edit fetch never runs → **the entire Rust async layer parks**, timers
included. Confirmed behaviorally (observed): with the window forced
`with_always_on_top` (self-test mode only), the freeze went 3-in-5-runs →
0-in-1 and the full script completed deterministically. Implications: a
fully-occluded/hidden Dioxus desktop window does not just skip painting — it
stops *downloading*, *ticking*, everything, until re-exposed. wry 0.53 has
the knob (`WebViewBuilder::with_background_throttling(Disabled)`), but
dioxus-desktop 0.7.9's `Config` does not plumb it through (source-only:
config.rs has no such option) — within the pinned version the workaround is
window-level (always-on-top / assured visibility), not framework-level.

## Helper crates

- `reqwest 0.12` (default-features = false, `json`) — no HTTP client in the
  framework; tokio runtime makes it drop-in.
- `serde 1` (derive) — decode `/search` hits.
- `tokio 1` (`time` only) — the 250 ms debounce sleep + self-test pacing;
  the framework runs on tokio yet re-exports no timer (recurring finding).

## Where the time went

- ~40% diagnosing the occlusion freeze: probe builds, five instrumented
  runs, reading dioxus-desktop's edits/webview source, the always-on-top
  fix. (The shared server log being written to by other agents' concurrent
  runs also cost a re-run to get an uncontaminated evidence window.)
- ~20% self-test design: making debounce/cancel observable through the
  server's logs (positive ABORT proof for use_resource via the ct_key
  resource — a dependency-keyed download that gets cancelled mid-stream).
- ~15% pre-verifying APIs in vendored sources (use_resource cancel-on-change
  at use_resource.rs:81; `Task` lives at `dioxus::core::Task`, not prelude —
  the one compile error).
- ~15% UI states/CSS (progress bar, error banners, disabled buttons).
- ~10% Cargo feature triage (reqwest without TLS, json feature).

## Measurements

- `src/main.rs`: **493 lines total = 402 production + 91 verification**
  (84-line VERIFICATION rsx block: ct_key resource + script, plus the
  always-on-top self-test gate in `main`). Production includes ~70 CSS-in-Rust
  and ~30 module-doc lines.
- First `cargo check`: **1 error** (`Task` not in prelude — fixed with
  `use dioxus::core::Task`), then 0 errors, 0 warnings.
- Release build (cold, parallel, deps shared-cache): **273.1 s** (reqwest/
  hyper stack roughly doubles the dioxus baseline); touch rebuild: **1.3 s**.
  Dependency graph ~309 unique crate names (+18 over dioxus-grid). Binary
  **7,363,472 bytes (7.0 MiB) raw**.
- Verification runs: retained `run-stdout.log` (clean full pass) and
  `run-stdout-frozen.log` / `run-stdout-frozen2.log` (two occlusion-freeze
  partials). RSS (main process, `ps -o rss= -p <pid>`): **96.8 MiB** during
  the self-test (steady; WKWebView WebContent is out-of-process and not
  attributable on a shared desktop). Plain (non-selftest) 10 s launch:
  window up, empty stdout/stderr, RSS **95.7 MiB**, clean SIGTERM.
- Server evidence (measurements/fetcher-server.log, clean window):
  `SEARCH q="amber"` ×1, `ABORT /download after 12/64 chunks`,
  `ABORT /download after 16/64 chunks`, `FLAKY attempt=22/23 -> 500`,
  `FLAKY attempt=24 -> 200`.
