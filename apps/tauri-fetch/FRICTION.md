# FRICTION — tauri-fetch ("Fetcher", SPEC-8)

Tauri =2.11.5 / tauri-build =2.6.3 (same pins as ../tauri-app), manual
no-Node setup, **plus tauri-plugin-http =2.5.9** (first helper crate any
Tauri app in this study has needed). No external JS libraries.

## Architecture: the JS path — with one forced substitution

The idiomatic Tauri answer to SPEC-8 is "the browser already does all of
this": `setTimeout` debounce, `AbortController` cancellation,
`ReadableStream` progress, promise-based error/retry. That is what this app
implements — the Rust side is **40 production LoC**: plugin registration,
a `get_config` command (webviews can't read env vars, so Rust hands over
`FETCHER_PORT`), and a `report` command for verification logging. No tokio
code, no channels, no state — the webview's event loop is the async runtime
and no future ever touches a UI thread, because the UI *is* the JS thread.

**The forced substitution**: browser-native `fetch()` does not work here.
The page origin is `tauri://localhost` (custom scheme), so WKWebView applies
CORS to any http request — and the shared fetcher-server sends no
`Access-Control-Allow-Origin` header (and may not be modified per the study
rules). Observed in the self-test, in the real webview:

    NATIVE_FETCH blocked: TypeError: Load failed

The escape hatch is the official `tauri-plugin-http`: a fetch-compatible JS
API (`window.__TAURI__.http.fetch` — injected as a global via
`withGlobalTauri`, still no npm) whose transport is **reqwest in the Rust
process**, reached over IPC, with the response body streamed back over a
Tauri channel into a real `ReadableStream`. CORS does not apply because the
webview never makes the HTTP request. So the honest architecture label is:
JS-shaped code (fetch + AbortController idioms preserved verbatim), Rust
execution — "Rust-side reqwest via commands/channels" prebuilt by the
ecosystem. The alternative (hand-rolling reqwest commands + progress
channels + CancellationToken) was not needed; the plugin is that path.

## The permission wiring (remote-domain ACL — Tauri-specific friction)

Two layers had to be configured by hand:

1. **Capability scope** (`capabilities/default.json`): granting
   `http:default` alone is NOT enough — every URL must match an `allow`
   scope entry (URLPattern syntax) or the plugin rejects the request at
   runtime. This app allows `http://localhost:7878/*` and
   `http://127.0.0.1:7878/*`. Friction: the scope is **baked in at build
   time** (tauri-build embeds capabilities), so although the app honors
   `FETCHER_PORT` at runtime, a port other than 7878 would be ACL-blocked
   unless the capability is edited and the app rebuilt. A dynamic
   env-derived scope has no sanctioned expression. (source-only: read from
   the capability/schema behavior; the non-7878 case was not exercised.)
2. **CSP**: the plugin's IPC transport is not subject to `connect-src`, so
   the ACL above is the real gate. The `connect-src ... http://127.0.0.1:7878`
   entries in tauri.conf.json exist only so the self-test's native-fetch
   CORS probe fails on CORS (the interesting layer) rather than on CSP.
   A production app using only the plugin needs no connect-src for the API
   host — security config lives in the capability, not the CSP, which is
   easy to get backwards coming from the web.

## Capability ratings

| Capability | Rating | Evidence | Note |
|---|---|---|---|
| async_integration | **built-in** | observed | The webview's event loop is the runtime; promises/async-await all the way. Rust's tokio exists under the plugin but is invisible — zero "get this future onto the UI thread" code, which no native-Rust framework in this study can say. |
| http_client_choice | **assembled** | observed | tauri-plugin-http (reqwest behind IPC) — not by preference but forced: native fetch is CORS-blocked from the tauri:// origin against a header-less server (`TypeError: Load failed`, quoted above). One Cargo line + capability scope; +51 unique crates (204→255), binary 8.0→13.9 MiB. |
| debounce_stale | **hand-rolled** | self-test | 250 ms setTimeout debounce; stale protection is BOTH real abort of the in-flight request and a sequence guard. Demonstrated: typed "mossy" (deterministic 297 ms server delay), typed "prism" at +265 ms; prism rendered, aborts=1, and the server log has **no** `SEARCH q="mossy"` line — the axum handler was dropped mid-latency-sleep, i.e. even search cancellation was server-side real. |
| progress_streaming | **assembled** | self-test | Plugin fetch returns a genuine ReadableStream (chunks over a Tauri channel); reader loop + Content-Length drives the bar. Full 8 MiB download: 83 incremental reads, bar to 100%, server logged `DOWNLOAD complete`. |
| cancellation_real | **assembled** | observed | AbortController → plugin `fetch_cancel` → reqwest request dropped → TCP close. Server log (measurements/fetcher-server.log), correlated by byte-offset with the run: `ABORT /download after 14/64 chunks` at the self-test's cancel click (1.6 of 8.0 MiB received). |
| error_retry_ux | **hand-rolled** | self-test | `res.ok` check → error status line + Retry button → repeat until 200 shows `success on attempt N`. Trivial in JS. The server's counter is process-global by design, so concurrent clients can shift the phase (this run succeeded at attempt=21 after one visible 500); reproducible probes reset it and run serially. Manual retry only; no auto-backoff. |

## Plugin wart found (the expensive one)

Aborting an AbortController whose plugin-fetch has **already completed**
raises an unhandled rejection from the plugin's fire-and-forget cancel
(`The resource id N is invalid`) — and in one run (observed once, fetch-run4)
was followed by the webview's JS halting entirely mid-selftest: no further
DOM updates or IPC, while a later download was dropped after 15/64 chunks
(consistent with the WebContent process dying and Rust reaping the body
channel). Not reproduced after the fix; root cause not isolated
(the stream-controller `error()` call in the plugin's abort listener on an
already-consumed response is the suspect — source-only reading of
api-iife.js). Production fix: null out the controller the moment a request
settles so only in-flight requests can be aborted. Browser fetch tolerates
late aborts as no-ops; the plugin's shim does not — a real compatibility gap
behind its "fetch-compatible" claim.

Also: one first-launch run (fetch-run1) produced no output at all — no
probe, no error, no server traffic — and was never reproduced across five
later launches. Unexplained; recorded because "webview silently blank" is a
failure mode native-Rust frameworks don't have.

## Helper crates

- `tauri-plugin-http =2.5.9` — see http_client_choice. Cost: +51 unique
  crate names (255 vs 204), binary 13.9 MiB vs 8.0 (MiB), clean build
  121.7 s vs 116.9 s (grid, same day, loaded machine).
- `serde`/`serde_json` — required by `#[tauri::command]`.

## LoC (509 physical; 558 including config)

- Production **377**: Rust **46** (40 of `src/main.rs` + 6 `build.rs`),
  frontend **331** (46 HTML + 200 JS + 85 CSS)
- Verification **132**: Rust 7 (`report` command + selftest flag), frontend
  125 (`ui/selftest.js` 118 + hooks/probe lines)
- Config: 49 (`tauri.conf.json` 33 + capability 16)
- The Rust:frontend production ratio (46:331) is the finding: Tauri's answer
  to the async chapter is "don't write Rust".

## Where the time went

1. The abort-after-completion wart: first full run froze mid-test with one
   cryptic rejection line; diagnosing it from the minified api-iife.js and
   designing the deterministic in-flight abort (picking "mossy" by computing
   the server's FNV-based per-query delays) took the bulk of the session.
2. The silent first launch (no output, no error) — burned a build/run cycle
   adding probes before anything was diagnosable.
3. The CORS/ACL double wall is conceptual, not typing: knowing that native
   fetch fails on origin, that the fix is a plugin, and that the plugin's
   gate is capability scope (not CSP) — none of it is discoverable from the
   error string "Load failed".

## Verification

Server confirmed up (`curl /health` → ok; started by an earlier session,
logging to measurements/fetcher-server.log). Five launches of the raw
release binary. Final selftest run (fetch-run5): **10/10 PASS** — plugin
/health, search render, stale protection (abort + no stale overwrite),
incremental progress (23 chunks at cancel), cancel at 1.6/8.0 MiB with
correlated server `ABORT /download after 14/64 chunks`, flaky error→Retry→
success, full 8.0 MiB download (83 progress events, server `DOWNLOAD
complete`). Server-log attribution done by recording the log's byte offset
before launch and diffing after (other agents share the server; the diff
window contained only this run's lines). Plain launch (no selftest): alive
at 10 s, empty stdout, killed cleanly. All interactions were synthetic
events dispatched inside the real WKWebView; no human input.
