# FRICTION — xilem-fetch ("Fetcher", xilem 0.4.0 from crates.io)

App: `apps/xilem-fetch/` · package `xilem-fetch` · `cargo run --release`
(reads `FETCHER_PORT`, default 7878; `tools/fetcher-server` must be up).
Build: release, clean (no app warnings; only the ecosystem-wide `block
v0.1.6` future-incompat note). Launch verified on macOS (M4 Pro) against the
live server: full scripted CGEvent pass — search-as-you-type → in-flight
abort demo → download with live progress → mid-stream cancel (server ABORT
proven) → full download to completion → flaky 500/500/200 with Retry —
alive after 10 s, empty stderr. Evidence in `verify/`: `run1-stdout.log`
(KEYSTROKE/SEARCH_*/DL_*/FLAKY_* lines), `run1-shot*.png` (inspected
screenshots), `server-log-offset.txt` (server log was 96 lines before this
run; all lines quoted below are past that offset in
`measurements/fetcher-server.log`). Note: the original build transcript was
lost; ratings are from code audit + a fresh observed run.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **built-in** | observed | xilem bundles tokio: `Xilem::new_simple` creates its own `tokio::runtime::Runtime::new()` (multi-thread, `enable_all`, so reqwest's IO/timer drivers are running). The stock `worker_raw` view is the whole story: it spawns a long-lived async task on that runtime, hands the UI an `UnboundedSender<Cmd>` (stored in app state via the first callback) and the task a `MessageProxy<Msg>` — `proxy.message(msg)` re-enters `app_logic` on the UI thread via the winit event loop. Three workers (search/download/flaky) under one `fork(body, (…))`. First-class and clean; no channel-to-event-loop glue to invent. |
| http_client_choice | **assembled** (helper crate) | observed | `reqwest 0.12`, default-features off + `json` (plain-HTTP localhost, no TLS stack). Chosen because it rides the exact tokio xilem already ships (one runtime, cargo unifies the crate), gives streaming bodies via `resp.chunk()`, and — decisive for this spec — **abort-on-drop** futures for real cancellation. Trap: xilem re-exports tokio *without* the `macros` feature, so `tokio::select!` (needed for debounce/cancel races) requires a direct `tokio = { features = ["macros"] }` dependency. |
| debounce_stale | **assembled** | observed (stdout + server log) | Both concerns live in the search worker, not the UI. Debounce: `tokio::select!` between a 250 ms `sleep` and `rx.recv()` — every newer keystroke restarts the window. Observed: typing "amber" (5 keystrokes, gen 1–5) issued exactly one request (`SEARCH_ISSUE gen=5`) → 20 results. Stale protection: requests are serialized per worker and an in-flight request is *really cancelled* — `select!` between the pinned request future and `rx.recv()`; a newer keystroke drops the future, closing the connection. Observed: `SEARCH_ABORT gen=8 superseded_by=9` for query "amberxyz", and the server log has **no** `SEARCH q="amberxyz"` line (its handler died with the connection) while gens 5/6/7/9 all logged. A `applied_gen` counter on the UI side is belt-and-braces (`STALE_DROP` path, unreachable by construction, not observed). |
| progress_streaming | **assembled** (bar itself built-in) | observed (stdout + screenshots) | Download worker: `resp.content_length()` → `DlMsg::Started{total}`, then a `resp.chunk().await` loop sending `DlMsg::Progress` per chunk (~125 ms cadence from the server) through the proxy; the stock `progress_bar(Option<f64>)` view renders it. Observed live: mid-run screenshot shows 41% / "3.25 / 8.00 MiB" with the button relabeled "Downloading…"; stdout logs 1 MiB steps; completion `DL_DONE bytes=8388608 secs=8.12` → bar 100%, green "done — 8.00 MiB in 8.1s". |
| cancellation_real | **assembled** (abort-on-drop is free) | observed (server log — the required proof) | Cancel sends `DlCmd::Cancel`; the worker's `select!` arm breaks, dropping the pinned stream future *and the `Response` inside it* → TCP close mid-body. No AbortHandle needed — drop semantics of reqwest futures do it. App: `DL_CANCEL after 4.28s` at 4,194,304+ bytes received. Server log, immediately following this run's `DOWNLOAD start`: **`ABORT /download after 33/64 chunks`** (offset >96, attributable to this run). UI shows red "cancelled at 4.12 / 8.00 MiB" with the bar frozen at 52%. |
| error_retry_ux | **assembled** | observed (stdout + screenshots) | `FlakyState` enum drives the section: Loading → red "call #N failed: HTTP 500 — synthetic failure" with the button relabeled **Retry** → green "call #3 succeeded (server attempt 30)". Observed full cycle: `FLAKY_RESULT click=1 err="HTTP 500 — synthetic failure"`, same for click=2, `FLAKY_RESULT click=3 ok attempt=30`. Manual retry only (no auto-backoff bonus). The server's documented process-global counter reached 30 because other clients had used it; canonical probes reset the cycle and run serially. |

## Helper crates & why

- `reqwest = 0.12` (default-features = false, `json`) — see
  http_client_choice.
- `serde = 1` (derive) — the two response shapes (`SearchResult`,
  `FlakyOk`).
- `tokio = 1` (`macros`) — only for `select!`/`pin!`; same crate instance
  as the one xilem bundles (444 `name =` entries in Cargo.lock incl. app).
- Verification tooling outside the crate: `verify/*.swift` CGEvent driver +
  window locator, `verify/batch.sh` (occlusion-checked window-relative
  input + screenshots) — copied from xilem-grid.

## LoC / time

Production **616** (single main.rs; zero custom masonry widgets — unlike
grid/dash/board, the stock view set covered this app entirely) ·
verification **166** (external `verify/` swift/sh drivers; in-binary aid is
only the `FETCH_TOPMOST=1` window-level gate).
Where the time went (reconstructed — original transcript lost; inferred
from code and comments): worker/channel architecture design (commands in,
proxy messages out, per-worker ownership of debounce/cancel races) is the
bulk of the thinking; the `tokio` macros-feature discovery and
`worker_raw`-vs-`worker` API archaeology in the vendored 0.4.0 sources
(no matching hosted docs) the rest; the three UI sections are plain stock
views. Verification-side: hitting the in-flight abort window required
timing keystrokes against the server's deterministic 150–300 ms latency
(first attempt missed; a 262 ms-spaced keystroke pair landed it).

## Surprises

- Good: real cancellation costs nothing in this stack — dropping a reqwest
  future closes the socket; the server `ABORT` line appeared on the first
  cancel click. The contrast with callback-style clients (egui/ehttp needs
  a ControlFlow hook; a sequence guard is their only search-stale option)
  is the core finding: xilem's tokio-native model makes *abort* the default.
- Good: `worker_raw` is exactly the right primitive for long-lived
  request-serializing services — one per concern, UI stays synchronous.
- Bad: xilem re-exports tokio without `macros`, so the moment you need
  `select!` you must add tokio as a direct dependency and trust cargo to
  unify versions — easy to get subtly wrong in a workspace.
- Bad: task/worker views are set-and-forget (not rebuilt on state change),
  so all mutable communication must flow through the channel; and the
  server's flaky counter is shared across clients (attempt=30 on our 3rd
  call), so clients cannot assume the 500/500/200 phase.
