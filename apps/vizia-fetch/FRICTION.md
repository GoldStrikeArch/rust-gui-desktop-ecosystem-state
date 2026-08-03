# FRICTION — Fetcher (vizia =0.4.0), SPEC-8

Reference: SPEC-8.md. Built + verified on macOS 26.5.2 (M4 Pro, rustc
1.96.1) against the shared `tools/fetcher-server` on port 7878 (already
running; `/health` → `ok` checked before and during verification).
`cargo build --release` and `cargo build --locked --release` clean (no
warnings); the binary launched, was driven with real typing/clicks, and the
scripted run exits 0 with `SELFTEST DONE pass=10 fail=0`. No fallback was
needed. RSS at idle after a search + download: **107.6 MiB**.

Evidence labels: **observed**, **self-test** (the `FETCH_SELFTEST=1` run,
captured in `selftest-log.txt` / `selftest-err.txt`, cross-checked against
the server's own log), **synthetic-input**, **source-only**.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **hand-rolled** | observed | **vizia has no executor at all.** Its entire async story is `cx.spawn(\|proxy\| ..)`, which starts a raw `std::thread` and gives you a `ContextProxy` whose `emit` posts an event back through winit's user-event proxy. There is no `Task`, no subscription, no runtime feature to pick — so this app builds its own `tokio::runtime::Runtime` (2 worker threads, `enable_all`), spawns futures on it, and moves a `ContextProxy` clone into each task. That works cleanly because `ContextProxy: Send` (its `EventProxy` trait is `: Send`) so it survives an `async move` block; it is **not `Sync`**, so every task needs its own clone rather than sharing one. The upside of owning the runtime is that there is no hidden executor mismatch — the iced trap where the default executor has no reactor and reqwest panics at runtime simply cannot happen here. |
| http_client_choice | **assembled** | observed | `reqwest 0.12.24`, `default-features = false, features = ["json", "stream"]` — no TLS stack for a localhost server. Chosen because the app already owns a tokio runtime (so reqwest is free), `bytes_stream()` gives progress plus mid-body cancellation, and `Client` is `Clone` so each task gets a cheap handle. `ureq` + threads would have needed a hand-rolled cancellation channel and would not have produced a real `ABORT`. |
| debounce_stale | **assembled** | self-test + server log | One mechanism does both: every keystroke `abort()`s the previous `JoinHandle` and spawns `sleep(250 ms) → reqwest`. Abort during the sleep is the debounce; abort after it drops the in-flight reqwest future, which is protocol-level cancellation. **Proof (debounce):** five keystrokes `a, am, amb, ambe, amber` inside one window produced `SEARCH_QUEUED gen=1..5`, exactly **one** server line `SEARCH_START q="amber"`, one `SEARCH_READY gen=5`, and four aborts. **Proof (stale):** typing `mossy` (297 ms server delay) then `prism` mid-flight produced the server pair `SEARCH_START q="mossy"` … `SEARCH_CANCEL` — the older request was killed *on the wire* — followed by `SEARCH_START q="prism"` / `SEARCH_DONE`. A generation counter is kept as a belt-and-braces guard and logged (`stale=` never fired for a real query). |
| progress_streaming | **assembled** | self-test + synthetic-input | `response.bytes_stream()` + `proxy.emit(DownloadProgress(received, total))` per chunk. Each `emit` wakes the vizia event loop through winit's user-event proxy, so there is **no polling** — this is the one place vizia's thread+proxy model is strictly nicer than a channel drained by a timer. 73–77 `DL_PROGRESS` lines for the 8 MiB body; a live screenshot shows the built-in `ProgressBar` at 3.0 / 8.0 MiB with the Cancel button beside it. Total comes from `Content-Length`. Structural note: the progress panel is bound to `Memo`s of the state (bar ratio, byte text) with a `Binding` only on the *state kind*, so 77 progress events update two signals instead of rebuilding 77 view trees. |
| cancellation_real | **assembled** | self-test + server log | `JoinHandle::abort()` drops the in-flight `reqwest::Response`, which closes the TCP connection. The server's own log, in the window opened by this run, reads:<br>`ts_ms=1785784304683 request_id=49 peer=127.0.0.1:52084 ABORT /download after 12/64 chunks`<br>and the app logged `DL_CANCELLED 1572864/8388608` — 1,572,864 bytes = exactly 12 × 128 KiB, so the two lines are the same transfer. The UI shows "cancelled at 1.5 / 8.0 MiB". |
| error_retry_ux | **assembled** | self-test | A four-state enum (`Idle / Running / Failed / Succeeded`), red error text and a Retry button (manual retry per SPEC; no auto-backoff). Proof: `FLAKY_ERR attempts=1`, `FLAKY_ERR attempts=2` (HTTP 500 with the body "synthetic failure" surfaced), `FLAKY_OK attempts=3 server_attempt=3`, matched by the server's `FLAKY attempt=1 -> 500 / 2 -> 500 / 3 -> 200`. The scripted run calls `GET /flaky/reset` first so the phase is deterministic on the shared server. |

## Helper crates

- `tokio 1` (`rt-multi-thread`, `time`, `macros`) — **the runtime vizia does
  not have**, plus `sleep` for the 250 ms debounce.
- `reqwest =0.12.24` (no default features; `json`, `stream`) — HTTP client.
- `futures-util 0.3` (no default features) — `StreamExt::next` for
  `bytes_stream()`. Already in the tree via reqwest.
- `serde 1` (`derive`) — typed `/search` and `/flaky` bodies.

Nothing was tried and rejected. The one *pattern* not used: draining results
with a shared channel polled by a `cx.add_timer`. That is the obvious shape
given `ContextProxy: !Sync`, but it is unnecessary — a per-task clone plus
`proxy.emit` delivers straight into the event loop with no polling latency.

## LoC split

- Production: **~620** (`src/main.rs` 800 minus ~180 lines of the
  `FETCH_SELFTEST` state machine, its assertions and the `trace()`
  instrumentation).
- Verification: **~180**, all in-app; there is no external driver script.
  The scripted lifecycle is a 100 ms tick-driven state machine with explicit
  `wait(predicate, timeout)` steps, so a hang is reported as
  `SELFTEST TIMEOUT` and can never be mistaken for a pass.
- Retained evidence: `selftest-log.txt` (stdout), `selftest-err.txt` (empty),
  plus the quoted windows of the shared server log.

## Where the time went

1. **Deciding how futures reach the UI thread.** vizia gives one primitive
   (`ContextProxy::emit`) and no guidance; the `Send`-but-not-`Sync` shape
   of the proxy is the whole design constraint and is not documented as
   such. Once that was settled, the async code compiled first try.
2. **Keeping 77 progress events from rebuilding the UI 77 times.**
   `Binding::new` rebuilds its whole subtree, so binding the download panel
   to the state enum re-created the progress bar on every chunk. Splitting
   into "Binding on the state *kind*, signals for the numbers" is the vizia
   idiom and is not obvious from the examples.
3. The network semantics themselves — debounce, stale, cancel, retry — were
   the easy part, exactly as in the iced cohort: `JoinHandle::abort()` on a
   future that owns a `reqwest::Response` *is* protocol-level cancellation.

## Surprises

- Good: no executor means no executor *mismatch*. Owning the tokio runtime
  explicitly is more boilerplate than `features = ["tokio"]`, but it is
  impossible to get wrong at runtime.
- Good: `proxy.emit` wakes the event loop, so streaming progress needs no
  timer and no channel draining. The 100 ms timer in this app exists only
  for the self-test script.
- Bad: vizia offers *nothing* above the raw thread+proxy primitive — no
  cancellation token, no "run this future and give me the result as an
  event", no way to tie a task's lifetime to a view. Every app that touches
  the network re-invents the same 30 lines.
- Neutral: `ContextProxy::emit` returns `Result<(), ProxyEmitError>` and
  fails silently if the event loop has closed, which is the correct
  behaviour for a task that outlives the window but means every call site
  ends in `let _ =`.
