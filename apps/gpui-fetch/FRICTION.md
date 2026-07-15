# FRICTION — Fetcher (gpui =0.2.2)

Reference: SPEC-8.md. Built + verified on macOS (M4 Pro, rustc 1.96.1):
`cargo build --release` clean in 205 s wall (cold, includes reqwest/tokio on
top of the gpui set; only the known `block v0.1.6` future-incompat warning).
Binary 7.49 MiB unstripped (7,855,536 B) — +2.4 MiB over gpui-grid for the
HTTP stack. Plain launch alive ≥ 10 s, empty stderr, killed cleanly. Server
`tools/fetcher-server` on :7878 was up throughout (`/health` → ok).

Verification: `FETCH_SELFTEST=1` drives the *same methods the UI events
call* (`set_search_text` per keystroke at 60 ms cadence, `start_download`,
`cancel_download`, `call_flaky`) on a timed script. Evidence retained:
fetch-stdout.log (client side), measurements/fetcher-server.log (server
side), fetch-download.png (live progress bar 2.5/8.0 MiB + Cancel),
fetch-flaky.png / fetch-done.png (flaky success, full green bar
"done — 8.0 MiB"). The server log is shared with sibling agents' runs;
attribution below uses exact byte/attempt matches. RSS during the run:
78,896 KiB ≈ 77.0 MiB (`ps -o rss= -p <pid>`).

## The headline finding (executor interop)

gpui 0.2.2 **re-exports an `HttpClient` trait** (`gpui::http_client`, with
`Application::with_http_client(...)` / `cx.http_client()` wiring) — but the
only implementation in the published crates is `NullHttpClient`, which
answers every request with `anyhow::bail!("No HttpClient available")`
(gpui-0.2.2/src/app.rs). Zed's real impl (`reqwest_client`) is not on
crates.io (third-party republishes exist — `reqwest-client-gpui-unofficial`,
`open-gpui-reqwest-client` — not evaluated). So you BYO exactly what Zed
does internally:

1. one std thread runs a parked **current-thread tokio runtime** (reqwest/
   hyper need tokio's reactor; gpui's GCD-backed background executor cannot
   drive them),
2. requests run there via `Handle::spawn`; the returned **`JoinHandle` is
   itself a `Future` and is awaited directly inside `cx.spawn` on gpui's
   foreground executor** — tokio wakers are cross-thread, so no glue code,
   no polling loop, no extra channel for one-shot requests,
3. streaming progress crosses on a `futures::channel::mpsc` (executor-
   agnostic) drained by a gpui task doing `entity.update + cx.notify`,
4. cancellation composes out of **drop semantics on both sides**: gpui
   `Task`s cancel when dropped, and each gpui task holds an
   `AbortOnDrop(tokio AbortHandle)` guard — dropping the gpui task aborts
   the tokio task, which drops the reqwest response, which closes the TCP
   connection. ~30 LoC of bridge total (`Net::start` + `AbortOnDrop`).

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **assembled** | self-test | Futures reach the UI thread by awaiting tokio `JoinHandle`s inside `cx.spawn` tasks; state lands via `this.update(cx, …) + cx.notify()`. gpui's own executors handle timers/debounce natively (`BackgroundExecutor::timer`). No runtime warnings, no deadlocks, both directions exercised continuously during the 24 s scripted run. |
| http_client_choice | **assembled** | self-test | reqwest 0.12 (default-features off — plain HTTP, `json`+`stream`) on a dedicated tokio current-thread runtime. Chosen over: (a) gpui's `HttpClient` trait — no shipped impl (see above), and implementing it would add `AsyncBody` bridging for zero benefit at app scale; (b) ureq+thread — no streaming-abort ergonomics; (c) unofficial republishes of Zed's client — unvetted. The bridge is the same architecture as Zed's internal `reqwest_client`. |
| debounce_stale | **assembled** | self-test | Debounce: 250 ms `background_executor().timer` as the first await of the search task; replacing `Option<Task>` drops the old task, so a pending debounce never fires. Proof: 5 keystrokes ("amber", 60 ms cadence) → exactly one `SEARCH_SENT 5 amber`. Stale: superseding an *in-flight* request aborts it (drop → AbortOnDrop) — `SEARCH_SENT 6 a` then `SEARCH_CANCEL in_flight_before 7`, and the server log contains **zero** `SEARCH q="a"` lines (axum cancels the handler mid-sleep on disconnect — the older request didn't just lose a race, it stopped existing). A `search_seq` guard remains as belt-and-braces; its `SEARCH_STALE_DROPPED` branch never fired. |
| progress_streaming | **assembled** | observed | `resp.bytes_stream()` chunks (server: 64 × 128 KiB, one per 125 ms) → `futures::channel::mpsc::unbounded` → gpui drain task → live bar (`w(relative(frac))` styled div) + "X.X / 8.0 MiB (NN%)" caption from Content-Length. Screenshot mid-download at 2.5/8.0 MiB (31%); full run `DL_DONE 8388608` + green "done — 8.0 MiB" + server `DOWNLOAD complete`. |
| cancellation_real | **assembled** | self-test | Cancel button sets `dl_task = None` — dropping the drain task aborts the tokio task, dropping the response mid-stream. Client: `DL_CANCEL 3145728`. Server: **`ABORT /download after 24/64 chunks`** — 24 × 128 KiB = 3,145,728 bytes, an exact match to my client log (the shared server log needed byte-level attribution because sibling agents were also aborting downloads). Connection-level, not UI-only. |
| error_retry_ux | **assembled** | self-test | `/flaky` failures render red "HTTP 500: synthetic failure (try N)" with the button relabelled **Retry** (amber); success renders green "success on server attempt 18 (after 3 tries)" — `FLAKY_ERR http=500` ×2 then `FLAKY_OK attempt=18`, matching server `FLAKY attempt=16/17 -> 500, attempt=18 -> 200`. Manual retry per spec; auto-backoff (bonus) not attempted. The spec now documents the server's process-global counter; canonical probes reset it and run serially, while this historical shared run retried until success instead of assuming the phase. |

## Helper crates

- `reqwest = 0.12` (default-features off; `json`, `stream`) — the de-facto
  Rust HTTP client; streaming body + clean abort-on-drop.
- `tokio = 1` (`rt`, `time`) — reqwest's required reactor; one parked
  current-thread runtime on a named thread.
- `futures = 0.3` — executor-agnostic mpsc channel + StreamExt.
- `serde`/`serde_json` — `/search` JSON.

## LoC split

- Production: **686** (src/main.rs 739 total, single file)
- Verification: **53** (`spawn_selftest` + env gate; the SEARCH_/DL_/FLAKY_
  stdout instrumentation doubles as the required evidence and is counted as
  production).

## Where the time went

1. **Establishing that gpui has no usable built-in HTTP client** — the trait
   + `with_http_client` API *looks* like the framework provides one; the
   NullHttpClient default is only discoverable in gpui's source. Deciding
   against implementing the trait (AsyncBody bridging) vs. the direct bridge.
2. **Evidence attribution on a shared server** — the fetcher-server log
   interleaves several agents' searches/downloads/flaky calls; proving *my*
   cancellation required matching exact chunk counts (24/64 ↔ 3,145,728 B)
   and rewriting the flaky self-test to be phase-independent.
3. The bridge itself was *not* a time sink — JoinHandle-as-Future plus two
   Drop impls; it compiled and behaved on the first run.

## Surprises

- Good: the whole interop story reduces to "await the tokio JoinHandle from
  a gpui task". No custom waker glue, no poll bridging, no `cx.notify`
  storms — and cancellation falls out of drop semantics on both executors.
- Good: superseded searches are cancelled so hard the server never even
  logs them (axum drops the handler future on client disconnect).
- Bad: `Application::with_http_client` is an attractive nuisance in the
  published crate — an API surface whose only shipped backend errors
  unconditionally.
- Shared-infrastructure lesson: deterministic server + parallel agents =
  non-deterministic *logs*; design self-tests to be phase-independent and
  attributable (exact byte counts) from the start.
