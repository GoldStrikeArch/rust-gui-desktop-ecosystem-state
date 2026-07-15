# FRICTION — Fetcher (iced =0.14.0)

Reference: SPEC-8.md. Built + verified on macOS (M4 Pro, rustc 1.96.1)
against the shared `tools/fetcher-server` on port 7878 (already running;
`/health` → `ok` checked before and during verification). `cargo build
--release` clean (first compile), launched, alive far past the 10 s bar,
killed cleanly. No fallback needed. RSS at idle after the full test
sequence: 92.7 MiB (`ps -o rss= -p <pid>`).

Evidence labels: **observed** (behavior seen without synthetic input),
**self-test** (synthetic HID input, verified from the captured stdout log
`selftest-log.txt` / `selftest-log-run1.txt` + screenshots in `selftest/`
+ the server's own log), **source-only**, **unexercised**.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **built-in** | observed | `iced = { features = ["tokio"] }` swaps the app executor for the tokio runtime, so `Task` futures get a reactor. Futures never touch the UI thread: `update` returns `Task::perform/sip`, the runtime polls on tokio, results come back as messages. The trap (known from iteration 2's `time::every`): with the **default** thread-pool executor there is no reactor and reqwest panics at runtime — nothing at compile time warns you. |
| http_client_choice | **assembled** | observed | `reqwest 0.12.24`, `default-features = false, features = ["json","stream"]` — no TLS stack for a localhost server. Chosen because iced-on-tokio makes it zero-friction, it exposes `bytes_stream()` (progress + mid-body cancel), and `Client` is `Clone` so `update` can hand cheap clones into futures. `ureq`+thread would have required a hand-rolled cancellation channel. |
| debounce_stale | **assembled** | self-test | One mechanism does both: each keystroke aborts the previous search task (`Task::abortable` → `Handle::abort`) and spawns `sleep(250 ms) → reqwest`. Abort during the sleep = debounce; abort after it = real in-flight HTTP cancellation. Proof (log): typing "cobalt" → 6 `SEARCH_QUEUED gen=1..6`, exactly **one** server-side `SEARCH q="cobalt"`, one `SEARCH_READY gen=6`. Mid-flight: `SEARCH_QUEUED gen=22 q="rx"` (299 ms server delay) has **no** `SEARCH_READY gen=22` — aborted on the wire when the next key arrived. A generation counter is kept as a belt-and-braces stale guard (`stale=` logged; never fired, because abortion already guarantees ordering). |
| progress_streaming | **assembled** | self-test | iced 0.14's first-party answer: `features = ["sipper"]` + `Task::sip(sipper(...), on_progress, on_output)` — one task that streams `(received, total)` per `bytes_stream()` chunk and ends with a result; both arrive as ordinary messages driving a built-in `progress_bar`. Proof: 64 `DL_PROGRESS` lines at 128 KiB steps, screenshot of the live bar at 3.5 / 8.0 MiB, `DL_DONE bytes=8388608` + server `DOWNLOAD complete`. Total from Content-Length. |
| cancellation_real | **assembled** | self-test + server log | `.abortable()` on the sip task; Cancel calls `Handle::abort()`, dropping the in-flight `reqwest::Response` → TCP close. Server log (measurements/fetcher-server.log) within my marked window: `DOWNLOAD start` then **`ABORT /download after 34/64 chunks`** — provably this app's, because the app logged `DL_CANCELLED 4456448/8388608` and 4,456,448 bytes = exactly 34 × 128 KiB chunks. (The log is shared with other agents' runs, hence the line-count marker + byte↔chunk correlation.) UI shows "cancelled at 3.4 / 8.0 MiB". |
| error_retry_ux | **assembled** | self-test | State enum (`Idle/Running/Failed/Succeeded`) + red error text + Retry button (manual retry per SPEC; no auto-backoff). Proof: `FLAKY_ERR attempts=1`, `attempts=2` (HTTP 500 surfaced with body "synthetic failure"), `FLAKY_OK attempts=3 server_attempt=27`, error + success screenshots. The documented server counter is process-global, so concurrent clients can shift the phase; canonical probes reset it, run serially, and keep the UI's attempts counter app-local. |

## Helper crates

- `reqwest =0.12.24` (no default features; `json`, `stream`) — HTTP client;
  see http_client_choice.
- `tokio 1` (`time`) — already transitively present via iced's executor and
  reqwest; declared directly only for `tokio::time::sleep` (the debounce).
- `serde 1` (`derive`) — typed `/search` results.

## LoC split

- Production: **~552** (src/main.rs 584 minus ~32 lines of `FETCH_SELFTEST`
  instrumentation: `trace()` helper + SEARCH_/DL_/FLAKY_ evidence prints).
- Verification: **~253** = ~32 in-app instrumentation +
  `selftest/uihelper.swift` 172 (copied from apps/iced-grid: CGEvent input
  synthesis, window lookup, frontmost-at-point guard) + `selftest/drive.sh`
  49. Retained evidence: `selftest-log*.txt`, `selftest/shot-*.png`, and
  the marked windows of the shared server log quoted above.

## Where the time went

1. **Shared-desktop input delivery, again** (dwarfed everything): the
   Retry-button clicks were silently swallowed for a long stretch — the
   occluded, backgrounded window stopped receiving synthetic events
   (focus stolen sub-second by other agents; macOS does not deliver the
   activating click to the app). Fix: relaunch fresh and drive the clicks
   immediately, activation + guard + click in a tight loop.
2. **Correlating evidence on a shared server**: the server log mixes four
   agents' traffic, so every claim needed a line-count marker plus a
   content correlation (chunk counts, query strings).
3. The async code itself was the easy part: compiled first try, and
   `abortable`/`sip` mapped 1:1 onto debounce/stale/progress/cancel. The
   one genuine design decision is remembering that dropping a reqwest
   future *is* protocol-level cancellation.

## Surprises

- Good: `Task::abortable` collapses debounce, stale protection and real
  request cancellation into one 4-line pattern — the generation guard
  turned out to be redundant (never fired).
- Good: `sipper` (new first-party in 0.14) is exactly the
  progress-streaming shape SPEC-8 asks for; no Subscription needed.
- Bad: executor choice is a silent runtime landmine — default features +
  reqwest panics with no compile-time hint that a tokio feature is needed.
- Neutral: iced has no built-in HTTP anything; the ecosystem answer
  (reqwest) is friction-free but is a 100+-crate dependency subtree.
