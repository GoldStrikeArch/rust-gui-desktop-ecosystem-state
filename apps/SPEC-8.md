# SPEC-8: "Fetcher" — async & network integration test

Iteration 4. The last untested architectural dimension: how each framework
integrates async I/O — runtimes, cancellation, streaming progress, and
error recovery — against a **deterministic local server** (no real network).

## The shared server (already provided)

`tools/fetcher-server/` (axum). Start with
`FETCHER_PORT=7878 cargo run --release` from that directory (or rely on an
already-running instance; check `curl -s localhost:7878/health` first).
Endpoints:
- `GET /health` → `ok`
- `GET /search?q=<str>` → JSON array of up to 20 deterministic results for
  the query, after an artificial 150–300 ms latency (deterministic per query)
- `GET /download` → 8 MiB body streamed over ~8 s (chunked, steady rate) —
  for progress bars; the server **logs `ABORT <path>` when a client
  disconnects mid-stream** (this log line is the proof of real cancellation)
- `GET /flaky` → HTTP 500 on the 1st and 2nd call of every 3 (a process-global
  counter), 200 `{"attempt":N}` on the 3rd — for retry UX. Run app probes
  serially so another client cannot consume a step in the cycle. Before a
  deterministic probe against an already-running server, call
  `GET /flaky/reset` to begin at the first 500 response.

Every request receives an `X-Fetcher-Request-Id` response header. Server log
lines include a timestamp, request ID, and peer address; searches log entry,
completion, and cancellation separately so a missing completion line is not
misread as “the request never arrived.”

## Functional requirements (the app)

1. **Window** titled `Fetcher (<framework>)`, ~700×560.
2. **Search-as-you-type**: text input → `/search?q=` with **250 ms
   debounce**; a slow older response must never overwrite a newer one
   (**stale protection** — real request cancellation or a sequence guard;
   which one the framework's async model affords is the core finding).
   Results shown in a list; show a subtle "searching…" state.
3. **Download**: button starts `/download` with a **live progress bar**
   (bytes received / total from Content-Length) and a **Cancel** button that
   **aborts the connection** (verify: the server log must show `ABORT` —
   UI-only cancellation doesn't count).
4. **Flaky**: button calls `/flaky` with visible error state and a Retry
   affordance (manual retry is fine; auto-retry with backoff is bonus) until
   success shows.
5. Read the port from `FETCHER_PORT` env (default 7878).

## FRICTION.md (required — audit conventions)

Per capability: rating + evidence label + note:
async_integration (which runtime/executor, how futures reach the UI thread),
http_client_choice (reqwest? ureq+thread? framework-provided? why),
debounce_stale (mechanism used; demonstrated how), progress_streaming,
cancellation_real (quote the server ABORT log line), error_retry_ux.
Also: helper crates + why; LoC split production/verification; where the
time went.

## Implementation rules

Independent crate `apps/<framework>-fetch/` (package `<framework>-fetch`),
same pinned framework version as `apps/<framework>-app/`, fallback rule,
build + launch verification with evidence labels. The server must be running
during your verification (start it yourself if `/health` fails; leave it
running). Shared-desktop rules as SPEC-6.

## Reference machine

Apple M4 Pro, 24 GB, macOS 26.5.2, rustc/cargo 1.96.1.
