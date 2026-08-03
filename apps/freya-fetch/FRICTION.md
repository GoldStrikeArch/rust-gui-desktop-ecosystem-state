# FRICTION — Fetcher (freya =0.4.0), SPEC-8

Reference machine per spec. `cargo build --release` clean, `cargo build
--locked --release` reproduces. Evidence: `selftest-log.txt` /
`selftest-err.txt` (`FETCHER_PORT=7879 FETCH_SELFTEST=1
./target/release/freya-fetch`, exit 0, `SELFTEST DONE pass=10 fail=0`), a
purpose-started server instance on port 7879 whose log was captured, and an
interactive release run driven with synthetic input.

## Ratings

| capability | rating | evidence | note |
|---|---|---|---|
| async_integration | **built-in** | self-test + observed | Freya has its own single-threaded executor: `spawn(fut) -> TaskHandle`, `spawn_forever`, `use_future`. Tokio interop is documented and is exactly `Builder::new_multi_thread()` + `let _guard = rt.enter();` before `launch`, then keep using **Freya's** `spawn`. The consequence is the nicest async story in this cohort: futures are polled *on the UI thread* with the Tokio reactor available, so `self.results.set(results)` right after `.await` just works — **no channel, no `Send` bound, no foreign-thread wakeup shim, no message enum**. `tokio::spawn` is the thing you must not use (it needs `Send`), which the docs say plainly. |
| http_client_choice | **assembled** | source | `reqwest =0.12.24`, `default-features = false` + `json` + `stream` — no TLS stack needed for 127.0.0.1, and `bytes_stream()` is what makes progress + real cancellation possible. Freya provides no HTTP client; it does ship an optional `query` feature (`freya-query`, a TanStack-Query-style cache) and a `remote-asset` feature for images, but neither is a general client. |
| debounce_stale | **hand-rolled (5 lines)** | self-test | No debounce helper (floem has one). `spawn` returns a `TaskHandle`, so the whole thing is: cancel the previous handle, spawn a task that `tokio::time::sleep(250 ms)` then requests. Because cancelling **drops the future at its await point**, the pending request dies with it — stale protection is *real cancellation*, not a sequence guard. The generation counter is kept purely as belt-and-braces and produces the `stale=` evidence. Log: `SEARCH_QUEUED gen=2 q="co"` never reaches `SEARCH_READY`, and the server records `request_id=16 … SEARCH_CANCEL`, while `gen=3 q="br"` completes. `stale_seen` stayed 0 — nothing had to be discarded because nothing survived. |
| progress_streaming | **assembled** | self-test + observed | `response.bytes_stream()` + `futures_util::StreamExt::next()`, writing `Download::Running { received, total }` straight into a signal on each chunk; `ProgressBar::new(percent)` is a stock component. 90–96 `DL_PROGRESS` lines per run, monotonic. Interactively verified: the bar showed `3.00 / 8.00 MiB — 37.5%` mid-download. |
| cancellation_real | **built-in** | self-test + server log | `TaskHandle::cancel()` drops the task, which drops the `Response` mid-`bytes_stream`, which closes the TCP connection. The server proves it: <br>`ts_ms=1785784540637 request_id=18 peer=127.0.0.1:52103 ABORT /download after 12/64 chunks`<br>App-side: `DL_CANCELLED 1572864/8388608`, and the self-test asserts no further progress arrives afterwards. |
| error_retry_ux | **assembled** | self-test | `/flaky` maps onto a small `Flaky` enum; the button relabels itself "Retry /flaky" while failed and the error text is shown inline. Manual retry (no backoff). Log: `FLAKY_ERR attempts=1`, `FLAKY_ERR attempts=2`, `FLAKY_OK attempts=3 server_attempt=3`. |

## The sharp edge that cost real time

`x.set(*x.peek() + 1)` **panics**. `peek()` returns a `ReadRef` which, as an
argument temporary, is still alive while `set()` takes the write borrow, so the
`GenerationalBox` refuses. The value has to be landed in a local first. Because
Freya's release-mode panic hook shows a modal `rfd` dialog and calls `exit(1)`
*before* chaining to the previous hook, this produced a hung window and an empty
stderr; the fix required a debug build (where the hook is not installed) plus an
app-level `set_hook` to see `writable_utils.rs:96` in a backtrace. Two lines of
code, ~20 minutes of diagnosis. The same shape sank the board app.

Second, smaller trap: wiring the search input through a `use_side_effect` that
reads `query` means the effect *also* runs once at startup (a spurious request)
and runs *after* any imperative call you make in the same tick — the first
version of the self-test called an explicit `start_search()` and the effect's
debounce immediately cancelled it, so a step meant to overlap two requests
silently tested nothing. Reading the evidence lines rather than only the
pass/fail count is what caught it.

## Helper crates

- `reqwest =0.12.24` (`json`, `stream`, no default features) — HTTP client.
- `tokio 1` (`rt-multi-thread`, `time`) — the reactor reqwest needs, plus
  `sleep` for the debounce and the scripted self-test delays.
- `serde 1` (`derive`) — `/search` and `/flaky` JSON.
- `futures-util 0.3` — `StreamExt::next()` for `bytes_stream()`; Freya's
  prelude re-exports no `StreamExt`.

## LoC split

- 654 total in one `src/main.rs`
- ~130 are the `FETCH_SELFTEST` scripted pass and its assertions
- ~520 production

## Where the time went

1. The `peek()`-temporary panic above, hidden behind the modal panic dialog.
2. Designing the self-test so the "stale" step really overlaps two requests
   (380 ms: past the 250 ms debounce, inside the server's 150–300 ms latency).
3. Almost nothing on the async plumbing itself — it is a `spawn` and an
   `.await`.

## Surprises

- Good: **the best async ergonomics of the cohort.** Because Freya's executor is
  single-threaded and lives on the UI thread, an async task *is* a UI task:
  await, then assign to a signal. Everything the Elm-shaped frameworks need
  (message enums, `Task::perform`, sipper/stream plumbing, foreign-thread
  wakeups) simply is not there.
- Good: cancellation is free and real — `TaskHandle::cancel()` is the whole
  story for both stale searches and download abort.
- Bad: no debounce, no timer, no `StreamExt` — small, ordinary gaps.
- Bad: the borrow/panic/modal-dialog combination turns a trivial mistake into
  an opaque hang.
