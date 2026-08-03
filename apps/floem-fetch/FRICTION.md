# FRICTION — Fetcher (floem git @ 778bb5f2), SPEC-8

Verified on macOS (M4 Pro, rustc 1.96.1): release build clean (locked too);
plain launch alive >8 s; `FETCH_SELFTEST=1` scripted run against the local
axum server (tools/fetcher-server @ 7878) printed the full lifecycle
(selftest-log.txt) and exited `SELFTEST DONE pass=10 fail=0`.

## Capability ratings (rating + evidence + note)

| capability | rating | evidence | note |
|---|---|---|---|
| async_integration | **assembled** | self-test | floem ships NO executor. The upstream tokio-timer example blesses: build a tokio multi-thread `Runtime`, then `block_on(block_in_place(floem::launch(...)))` — after which `tokio::spawn` works from any UI closure. Futures never touch the UI thread; results come back via `create_ext_action` (one-shot) and `update_signal_from_channel` (streams), both built on floem's `ExtSendTrigger` foreign-thread wakeup. Clean, but 100% assembly-required and documented only by example. |
| http_client_choice | **assembled** | self-test | `reqwest =0.12.24` (same pin as iced-fetch), default-features off + `json`,`stream`. Chosen because the tokio runtime is already there; reqwest futures need only `tokio::spawn`. |
| debounce_stale | **built-in + assembled** | self-test | The 250 ms debounce is a floem BUILT-IN: `debounce_action(query_signal, 250ms, cb)` — no timer bookkeeping at all (unique among frameworks tested). Stale protection is real cancellation: each `start_search` aborts the previous tokio task via `AbortHandle`, dropping the in-flight reqwest future. Server log proof of a mid-flight abort: `request_id=17 … SEARCH_CANCEL`. The generation counter never saw a stale response (`stale=false` on every SEARCH_READY; `stale_seen == 0` check). |
| progress_streaming | **assembled** | self-test | `bytes_stream()` chunks → std mpsc channel → `update_signal_from_channel` → `Download::Running` signal → reactive progress bar. ~120 monotonic `DL_PROGRESS` lines over the 8 s stream. Papercut: floem has NO progress-bar widget — the bar is two nested styled views with a reactive `width_pct`. |
| cancellation_real | **assembled** | self-test | `AbortHandle::abort()` drops the reqwest `Response` mid-stream → TCP close. Server proof (quoted): `ts_ms=1785782892763 request_id=20 peer=127.0.0.1:51969 ABORT /download after 12/64 chunks`. App side: `DL_CANCELLED 1572864/8388608`, and zero further progress events in the following 700 ms (checked). |
| error_retry_ux | **assembled** | self-test | Non-2xx surfaced as an error state with a Retry button (manual retry per spec). Deterministic cycle after `/flaky/reset`: `FLAKY_ERR attempts=1`, `FLAKY_ERR attempts=2`, `FLAKY_OK attempts=3 server_attempt=3`. |

## Helper crates

- `reqwest =0.12.24` (json, stream; no TLS) — HTTP client.
- `tokio 1` (rt-multi-thread, time) — the executor floem doesn't have.
- `serde 1` (derive) — typed /search results.
- `futures 0.3` — `StreamExt::next` over the byte stream.

## LoC split

711 total; ~200 are the scripted self-test + evidence tracing; production
logic ≈ 510.

## Where the time went

1. Choosing the crossing-back mechanism per shape: one-shot completions →
   `create_ext_action`; streams → `update_signal_from_channel` (which even
   disposes itself when the sender drops on abort). Both undocumented
   outside source/examples.
2. The progress bar (no widget — hand-styled fill).
3. Everything else mapped 1:1 from the iced port; the debounce got SIMPLER
   (built-in `debounce_action` replaced the sleep-prefix-abort trick).

## Surprises

- Good: `debounce_action` as a framework primitive.
- Good: dropping a tokio task is a fully honest cancellation story — both
  server-side proofs (SEARCH_CANCEL, ABORT /download) appeared first try.
- Bad: no executor, no progress bar, and the async bridging primitives are
  discoverable only by reading floem's source.
