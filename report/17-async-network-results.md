# Async & network: "Fetcher" in seven frameworks (macOS)

**Run dates:** 2026-07-09..10. Evidence labels per cell in per-app
FRICTION.md; raw rows in [data/iter4-rows.md](data/iter4-rows.md). All
network behavior tested against the deterministic local server
(`tools/fetcher-server/`): seeded search latency, an 8 MiB/8 s streamed
download, a flaky endpoint, and — critically — an `ABORT` log line whenever a
client disconnects mid-stream, so the aggregate download-cancellation claims
below have TCP-level evidence rather than UI inference. Per-app attribution is
less uniform because the shared log has no request/client IDs or timestamps.
SPEC-8 now deliberately defines `/flaky` as a process-global three-call cycle
and requires resetting the cycle before a deterministic serial probe so another
client cannot consume a step. The original shared-host agents instead
phase-aligned or ran private instances.

Iteration 4, SPEC-8: search-as-you-type (250 ms debounce + stale protection),
streamed download with live progress and real cancel, error/retry UX.

## The headline: all seven cancel for real — with seven different architectures

Every framework produced a server-verified `ABORT /download after N/64
chunks` line. *How* they got there is the fragmentation story:

| | Runtime story | Stale-search protection | Cancel mechanism |
|---|---|---|---|
| iced | `features=["tokio"]` swaps the app executor to tokio | `Task::abortable` — **one mechanism covers debounce, stale, and cancel**; the generation counter never fired | drop reqwest Response via Task abort |
| egui | **no async runtime** — ehttp spawns a thread per request; callbacks + `ctx.request_repaint()` | AtomicU64 generation guard (ehttp plain fetches can't abort) | streaming callback returns `ControlFlow::Break` — aborted first try |
| gpui | parked current-thread tokio on a std thread; tokio JoinHandles awaited *directly* inside `cx.spawn` (cross-thread wakers) | replacing `Option<Task>` drop-cancels; the superseded request produced no post-delay server `SEARCH` line | drop drain task → AbortOnDrop → TCP close |
| tauri | the webview event loop drives user code; zero **user-authored** Rust async, while tauri-plugin-http executes Reqwest async work in Rust | AbortController + sequence guard (both) | plugin `fetch_cancel` → reqwest drop |
| xilem | xilem creates its own tokio; stock `worker_raw` + MessageProxy | worker-side `select!`; aborted query left no post-delay server `SEARCH` line | breaking the select! arm drops the pinned Response |
| slint | background tokio + `upgrade_in_event_loop` (its `spawn_local` **panics** with reqwest: executor but **no reactor**) | restartable single-shot Timer + AbortHandle + seq guard | `abort_handle().abort()` |
| dioxus | VirtualDom runs *on* multi-thread tokio | `use_resource` dependency change auto-cancels — **debounce and stale need no guard at all** | dropped future → hyper closes TCP |

HTTP client choices split reqwest (5) / ehttp (egui) / tauri-plugin-http
(forced — see traps). A pleasant systemic surprise, consistent with axum
dropping handlers on client disconnect: superseded *searches* never reached
the server's post-sleep log point in gpui/xilem/dioxus. Because `/search` logs only after its artificial delay, absence of that
line does not prove that no request arrived at the handler.

## The traps (one per framework, again)

- **iced**: `time::every`/network need a non-default executor feature; the
  default thread-pool executor has **no reactor** — reqwest panics at
  runtime with no compile-time hint.
- **egui**: ehttp's `Request::get` sets a default timeout that kills an 8 s
  stream — must clear it; plain (non-streaming) fetches cannot be aborted.
- **gpui**: `Application::with_http_client` is an attractive nuisance — the
  trait ships but the only implementation is a `NullHttpClient` that errors
  unconditionally (Zed's reqwest client is unpublished).
- **tauri**: browser `fetch` is **CORS-blocked** from `tauri://localhost`
  against any server you can't add headers to; the sanctioned
  tauri-plugin-http (+51 crates, +5.9 MiB) moves the gate into the capability
  ACL with **URL scopes baked at build time** (a runtime port change needs a
  rebuild); its fetch shim once froze the webview on a late abort.
- **xilem**: re-exports tokio *without* the `macros` feature — `select!`
  needs a direct tokio dependency.
- **slint**: `slint::spawn_local` panics ("no reactor running") the moment a
  reqwest future is polled — disproven by a dedicated probe binary, not just
  avoided.
- **dioxus**: the **occlusion freeze** — an occluded/unactivated window parks
  the entire VirtualDom+task loop (timers *and in-flight downloads* stop);
  wry 0.53 has `with_background_throttling(Disabled)` but dioxus 0.7.9
  doesn't plumb it through. Reproduced 3 of 6 runs; deterministic with
  always-on-top. **Upstream-actionable.**

## Verdict for the initiative

The async story is better than folklore suggests: every framework reached
correct debounce + stale-protection + real cancellation + streamed progress,
and Rust's drop-based cancellation composes beautifully where tokio is
native (dioxus, xilem, iced) or bridged (gpui). The costs are (a) seven
different integration idioms to learn — none portable, (b) a runtime-shaped
trap in nearly every framework that only fails at runtime, and (c) in the
tested Tauri browser/plugin path, the network gate moves into platform
security machinery (CORS/ACL). Dioxus used Rust Reqwest and did not exercise
that gate. A shared "async-UI bridge" pattern document
would be a cheap, high-value initiative deliverable.

## Caveats

One implementation per framework; local server only (no TLS, no real-network
latency variance); the shared server instance interleaved multiple agents'
traffic — cancellation evidence was attributed by byte-exact chunk counts or
private instances. Tauri's original client-side trace is missing, but a
[fresh audit rerun](../measurements/verification-iter4-rerun/tauri-fetch-20260710.log)
retains 10/10 client self-test checks, including stale-request abort, streamed
progress, download cancellation, flaky error/retry, and a full download. It ran
against the still-running historical server, whose separate log retained the
old untagged format, so this closes the client-trace gap without retroactively
adding request IDs to the historical server evidence. The current server source
now emits request IDs and `SEARCH_START`/`DONE`/`CANCEL` events for future
serial reruns; `/flaky/reset` establishes the initial 500/500/200 phase.
macOS/M4 Pro only.
