# FRICTION.md — Fetcher (egui 0.35 + eframe + ehttp)

App: `apps/egui-fetch/` · package `egui-fetch` · `cargo run --release`
(reads `FETCHER_PORT`, default 7878; `tools/fetcher-server` must be up).
Build: release, clean (no warnings). Launch verified on macOS against the
live server: full scripted pass (search → stale demo → download + cancel →
flaky retry-until-success), alive after 10 s, no stderr. Evidence retained
in `verify-stdout.log` (app stdout) and `measurements/fetcher-server.log`
(server side). 2 tests, incl. an egui_kittest end-to-end: types "amber"
into the real search box, waits for the debounced request to land from the
callback thread.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **assembled** (idiomatic, tiny) | observed | No runtime/executor at all. `ehttp` spawns one native thread per request and invokes a plain callback; the callback writes into `Arc<Mutex<..>>` shared state and calls `ctx.request_repaint()` (Context is Send+Sync+Clone — designed for exactly this). Time-based logic (debounce, backoff) is deadline math in the frame callback + `request_repaint_after`, same idiom as egui-dash. Total integration glue is ~15 LoC; there is nothing framework-shaped to fight. |
| http_client_choice | **assembled** (helper crate) | observed | `ehttp 0.7.1` (egui org; native backend is ureq 3 under the hood) over reqwest+poll_promise: callback-based fits immediate mode with no tokio, no pinned-future plumbing, wasm-portable, and it's the pairing egui_extras itself declares (crates.io: egui_extras 0.35.0 → ehttp ^0.7.1 — ehttp is NOT egui-versioned). Trade-off accepted: no handle to abort a *plain* fetch in flight (see debounce_stale); reqwest would give abort-on-drop. **Trap:** `Request::get` sets a default timeout — must set `timeout = None` for the 8 s stream. |
| debounce_stale | **assembled** | observed (stdout log) | Debounce: `debounce_until = now + 250 ms` on `changed()`, fired from the frame callback via one `request_repaint_after` (no polling). Stale protection: **sequence guard, not cancellation** — `AtomicU64` generation per request; the callback applies a response only if `gen > applied_gen`. Demonstrated with the server's deterministic latency: fired "coral" (273 ms) then "amber" (154 ms); log shows `SEARCH_APPLY gen=3` then `SEARCH_STALE_DROP gen=2 applied_gen=3`. ehttp affords no request abort for plain fetches, so a guard is the honest mechanism (the core finding for egui). |
| progress_streaming | **built-in** (in the helper crate) | observed | `ehttp::streaming::fetch` (feature `streaming`) delivers `Part::Response` (headers → Content-Length) then `Part::Chunk(Vec<u8>)` per chunk on the fetch thread; empty chunk = EOF. Each chunk updates shared `received` and requests a repaint → `egui::ProgressBar` with MiB text tracks the 8 MiB / ~8 s stream live (64 repaints total, ~8 Hz). No chunked-reader thread needed. |
| cancellation_real | **built-in** (ControlFlow) | observed (server log) | Cancel button sets an `AtomicBool`; the next chunk callback (≤125 ms later) returns `ControlFlow::Break`, which makes ehttp drop the ureq reader → TCP close. App: `DOWNLOAD_CANCELLED received_bytes=2490368`. Server log (the required proof): **`ABORT /download after 20/64 chunks`**. UI-only cancellation this is not. |
| error_retry_ux | **assembled** | observed (stdout log) | Error state = red label + Retry button; bonus auto-retry checkbox with exponential backoff (400 ms → 4 s cap, "auto-retry in N s" countdown), driven by the same deadline mechanism. Verified run: `FLAKY_RESULT try=1 status=500`, `try=2 status=500`, `try=3 status=200 attempt=9` (server counter is global, so the 500/500/200 cycle phase depends on total prior calls — retry-until-success is the only robust client logic). |

## Helper crates & why

- `ehttp = "=0.7.1"` (features `json`, `streaming`) — see
  http_client_choice. `json` gives `Response::json::<T>()`.
- `serde = "=1.0.228"` (derive) — for the `/search` result struct;
  serde_json arrives transitively via ehttp/json.
- Dev-only: `egui_kittest = "=0.35.0"`.

## LoC / time

Production 487 (src/main.rs above the test module) · verification 163
(58 tests + 105 src/selftest.rs scripted driver; ships in the binary but
inert without `FETCH_SELFTEST=1`).
Where the time went: ~30% API verification before writing (ehttp streaming
Part/ControlFlow semantics, default-timeout trap, version cross-checks on
crates.io — pre-2026 examples are useless for egui 0.35); ~30% self-test
design (deterministic stale-ordering demo required reimplementing the
server's FNV-1a latency function to pick a provably slow/fast query pair);
~25% the three feature slices themselves (short); ~15% the kittest
end-to-end (hit the known `Harness::run`-panics-while-spinner-animates
trap again — must drive with `step()`).

## Surprises

- Good: real cancellation was the easiest cell — `ControlFlow::Break` from
  the chunk callback aborts the connection; the server ABORT line appeared
  on the first attempt.
- Good: the whole app needs zero async machinery; egui's Context being a
  thread-safe repaint handle makes "futures reach the UI thread" a
  non-question (there are no futures).
- Bad: plain `ehttp::fetch` cannot be aborted mid-flight, so search
  "cancellation" is necessarily a stale-guard; if true request abort
  mattered, you'd switch to reqwest + tokio and drop the JoinHandle.
- Constraint: `/flaky` uses a documented process-global counter, so clients
  cannot assume they start at attempt 1 if probes run concurrently. Canonical
  probes call `/flaky/reset` and then run serially.
