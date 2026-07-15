# FRICTION — slint-fetch (Fetcher, Slint =1.17.1)

Reference machine: Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
Default slint features (winit backend, femtovg GL renderer).

Headline: Slint has **no async story of its own** — no runtime, no HTTP, no
`tokio` integration — but the two primitives it does have
(`Weak::upgrade_in_event_loop` for thread→UI marshalling, `slint::Timer` for
UI-thread scheduling) are exactly enough, and the whole app is a thin,
predictable two-thread architecture. The one trap is believing
`slint::spawn_local` covers this: it runs futures on the winit loop with **no
reactor**, so any tokio-backed future (reqwest included) panics at runtime.

## Evidence base

`FETCH_SELFTEST=1` drives search via real key events (`dispatch_event`),
demonstrates the stale guard with two overlapping requests of known
server-side latency, cancels a download mid-stream, and walks /flaky through
500→500→200. Verification ran against a **private fetcher-server instance on
port 7911** (also proving the `FETCHER_PORT` env requirement) because the
shared :7878 instance showed interleaved traffic from a sibling agent
(3 concurrent downloads, foreign FLAKY counter bumps) — that first run is in
the shared `measurements/fetcher-server.log`, lines 33+. Retained artifacts
here: `verify-stdout.log` (app), `verify-server.log` (private server log),
`verify-snapshot.png` (pixel evidence: typed query, results list, cancelled
progress bar at 36%, green flaky success), `probe-stdout.log` (spawn_local
probe), `launch-plain.log` (plain 10 s launch, default port 7878).

## The architecture decision (the core finding)

Two candidate models:

1. **`slint::spawn_local` + async HTTP client** — rejected, with observed
   evidence: `src/bin/spawnlocal_probe.rs` polls a plain `reqwest::get` from
   `spawn_local` on the Slint event loop. Result (exit 101, `probe-stdout.log`):

   ```
   thread 'main' panicked at tokio-1.52.3/src/net/tcp/stream.rs:164:18:
   there is no reactor running, must be called from the context of a Tokio 1.x runtime
   ```

   You could paper over this with `async-compat` or a tokio-free client, but
   then you own a second executor's edge cases inside the render loop.

2. **Background tokio runtime + `upgrade_in_event_loop`** — chosen. One
   multi-thread runtime with a single worker; every network task is
   `rt.spawn(...)`; each result hops to the UI thread via
   `Weak::upgrade_in_event_loop(move |ui| ...)` (the ergonomic wrapper over
   `invoke_from_event_loop`). No `Send` gymnastics beyond the closure itself;
   `Rc`-based UI state stays on the UI thread; the iteration-3 lesson
   (re-entering the loop from a Timer callback aborts) is structurally
   avoided because nothing blocking or re-entrant ever runs on the UI thread.

## Capability ratings

| Capability | Rating | Evidence | Notes |
|---|---|---|---|
| async_integration | **assembled** | observed | tokio =1.52.3 (1 worker) + `upgrade_in_event_loop`. The probe (above) proves `spawn_local` is not viable for tokio-backed I/O — Slint gives you a futures executor but no reactor, and nothing in the API surface warns you. |
| http_client_choice | **assembled** | observed | reqwest =0.12.24, `default-features = false` (plain-HTTP localhost ⇒ no TLS stack; saves ~40 crates). Chose reqwest over ureq+thread for streamed `chunk()` reads, connection pooling, and abort-on-drop cancellation. The `/flaky` cycle is process-global, so pooling is not required for retry correctness. |
| debounce_stale | **assembled** | synthetic-input + self-test | Debounce: one `slint::Timer` (SingleShot, 250 ms) restarted on every `edited` — two real keystrokes 100 ms apart produced **DEBOUNCE_DISPATCHES 1**. Stale: two layers — the previous in-flight task is aborted (AbortHandle), and a monotonic sequence checked on the UI thread before applying. Demonstrated with aborting disabled: q="rx" (server latency 299 ms, replicated FNV-1a) fired 40 ms before q="s" (152 ms); log shows `SEARCH_APPLIED seq=3 q="s"` then `STALE_DROPPED seq=2 q="rx"`, final status "20 results for \"s\"". |
| progress_streaming | **assembled** | observed | `resp.chunk().await` loop; Content-Length 8,388,608 from headers; ~64 UI updates over the 8 s stream into the std-widgets `ProgressIndicator` (that part is built-in). Snapshot shows the bar frozen at 36% after cancel. |
| cancellation_real | **assembled** | observed | `JoinHandle::abort_handle().abort()` drops the in-flight hyper body → TCP close. Private server log (`verify-server.log`): **`ABORT /download after 23/64 chunks`** — 23×125 ms ≈ the 3.0 s cancel point, matching UI progress 0.36. UI cancel state set synchronously in the callback; late chunk updates guarded by a state check. |
| error_retry_ux | **assembled** | observed | /flaky: `FLAKY_ERR 500` → red "HTTP 500 — click Retry" (button relabels to Retry) → 500 → 200: `success: {"attempt":3}` in green (snapshot). Manual retry per spec; no auto-backoff (bonus not implemented). Server log confirms attempt=1,2 → 500, attempt=3 → 200 on one connection. |

## Helper crates

| Crate | Version | Why |
|---|---|---|
| tokio | =1.52.3 | Reactor + executor for reqwest; 1 worker thread. |
| reqwest | =0.12.24 (no default features) | Streaming, pooling, abort-on-drop (see http_client_choice). |
| serde | =1.0.228 (derive) | Typed /search results. |
| serde_json | =1.0.150 | Parse response bodies (avoids reqwest's `json` feature). |

Rejected: `async-compat` (shim to make spawn_local work — extra executor
inside the render loop), `ureq` (no streaming abort semantics, new
connection per request breaks /flaky).

## LoC (production vs verification)

- Production: **437** — `src/main.rs` 292 (runtime, dispatch/search/download/
  flaky) + `build.rs` 3 + `ui/main.slint` 142.
- Verification: **179** — `src/main.rs` 142 (selftest harness, FNV latency
  picker, snapshot writer) + `src/bin/spawnlocal_probe.rs` 37.
- Production carries ~3 instrumentation lines (dispatch counter, evidence
  prints) counted as production above.

## Measurements

- Clean release build **56 s** serial (first concurrent-with-grid build read
  5 m 57 s — CPU contention), no-op rebuild **0.3 s**.
- Binary: 17,140,352 bytes raw / 14,775,376 bytes (**14.1 MiB**) stripped.
- Dependencies: **453** unique name-version entries incl. the app (grid app:
  416 — reqwest+tokio with no TLS cost only ~37 extra entries).

## Where the time went

1. ~30% the stale/debounce demonstration design: replicating the server's
   FNV-1a latency function to pick provably slow/fast queries, and deciding
   to expose `abort_prev=false` so the sequence guard could be shown working
   on its own.
2. ~25% verification attribution: the shared :7878 server had a sibling
   agent's downloads/flaky calls interleaved with mine (three ABORT lines,
   foreign counter bumps) — resolved by spinning a private :7911 instance,
   which conveniently also exercised `FETCHER_PORT`.
3. ~20% the spawn_local probe + reading crate sources to confirm Slint has
   no reactor (future.rs is executor-only).
4. ~25% UI wiring and states (download/cancel/flaky state machines in
   properties; guarding late chunk updates after cancel).
