// Self-test harness (verification code, not production). Runs only when the
// app was launched with FETCH_SELFTEST=1. Drives the real UI (synthetic input
// events + button clicks) against the real local server and reports evidence
// to stdout via the `report` command. The outer script correlates the
// download-cancel step with the server's `ABORT /download` log line.
(async () => {
  const inv = window.__TAURI__?.core?.invoke;
  const say0 = (l) => (inv ? inv("report", { line: l }).catch(() => {}) : undefined);
  const f = window.__fetchapp;
  let cfg;
  try {
    cfg = await f.ready;
  } catch (e) {
    await say0(`SELFTEST ERROR init: ${e} (tauri=${typeof window.__TAURI__} http=${typeof window.__TAURI__?.http} app=${typeof f})`);
    return;
  }
  if (!cfg.selftest) return;
  await say0(`PROBE http=${typeof window.__TAURI__?.http} fetch=${typeof window.__TAURI__?.http?.fetch}`);
  const { state, els } = f;

  const say = (line) => f.invoke("report", { line });
  let pass = 0, fail = 0;
  const check = async (cond, msg) => {
    cond ? pass++ : fail++;
    await say(`SELFTEST ${cond ? "PASS" : "FAIL"} ${msg}`);
  };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (fn, what, timeout = 6000) => {
    const t0 = performance.now();
    while (performance.now() - t0 < timeout) {
      try { if (fn()) return true; } catch (_) {}
      await sleep(30);
    }
    await say(`SELFTEST TIMEOUT waiting for ${what}`);
    return false;
  };
  const type = (q) => {
    els.searchInput.value = q;
    els.searchInput.dispatchEvent(new Event("input", { bubbles: true }));
  };

  await say(`SELFTEST START base=${f.base()}`);

  // 1. CORS probe: browser-NATIVE fetch from the tauri://localhost origin.
  //    Expected to fail (server sends no CORS headers) — this failure is the
  //    documented reason the app uses tauri-plugin-http.
  try {
    const r = await fetch(`${f.base()}/health`);
    await say(`NATIVE_FETCH unexpectedly ok status=${r.status}`);
  } catch (e) {
    await say(`NATIVE_FETCH blocked: ${e}`);
  }

  // 2. Plugin transport sanity: /health through Rust reqwest.
  const h = await f.http.fetch(`${f.base()}/health`);
  await check(h.ok && (await h.text()) === "ok", "plugin fetch /health == ok");

  // 3. Search-as-you-type (debounce + render).
  type("amber");
  await waitFor(() => state.resultCount > 0, "search results");
  await check(state.resultCount > 0, `search "amber" -> ${state.resultCount} result(s)`);
  const names = [...els.resultsEl.querySelectorAll("li span:first-child")].map((s) => s.textContent);
  await check(names.every((n) => n.includes("amber")), "all results match query");

  // 4. Stale protection: fire query A, then query B while A is in flight.
  //    A = "mossy": deterministic 297 ms server delay (longest in the word
  //    list), so with the 250 ms debounce A's request starts at ~t=252 and
  //    completes at ~t=549; typing B at t=265 fires B (and the abort of A)
  //    at ~t=515 — comfortably mid-flight.
  const abortsBefore = state.searchAborts;
  type("mossy");         // A
  await sleep(265);      // debounce fired at 250 -> A's request is in flight
  type("prism");         // B: aborts A + bumps the sequence guard
  // Old results stay visible while B is in flight (status: "searching…");
  // wait until B's response actually rendered before asserting.
  const firstName = () => els.resultsEl.querySelector("li span:first-child")?.textContent || "";
  await waitFor(() => firstName().includes("prism"), "query B results rendered");
  const names2 = [...els.resultsEl.querySelectorAll("li span:first-child")].map((s) => s.textContent);
  await check(names2.length > 0 && names2.every((n) => n.includes("prism")),
    "newer query's results shown (no stale overwrite)");
  await check(state.searchAborts > abortsBefore,
    `stale in-flight request aborted (aborts=${state.searchAborts})`);

  // 5. Download + real cancellation: cancel mid-stream, then let the outer
  //    script find `ABORT /download` in the server log.
  els.dlBtn.click();
  await waitFor(() => state.dlReceived > 1.5 * 1024 * 1024, "download past 1.5 MiB");
  const evAtCancel = state.progressEvents;
  await check(evAtCancel >= 5, `progress streamed incrementally (${evAtCancel} chunks)`);
  els.dlCancel.click();
  await waitFor(() => state.download === "cancelled", "cancelled state");
  await check(state.download === "cancelled" && state.dlReceived < state.dlTotal,
    `cancelled at ${(state.dlReceived / 1048576).toFixed(1)} of ${(state.dlTotal / 1048576).toFixed(1)} MiB`);
  await say("SELFTEST CANCELLED_DOWNLOAD");
  await sleep(700); // give the server a moment to log ABORT

  // 6. Flaky + retry: click, then use the Retry affordance until success
  //    (2 failures per success normally; other clients may shift the phase).
  els.flakyBtn.click();
  await waitFor(() => state.flaky !== null, "flaky response");
  const sawError = !state.flaky.ok && !els.flakyRetry.hidden;
  for (let i = 0; i < 6 && !(state.flaky && state.flaky.ok); i++) {
    state.flaky = null;
    (els.flakyRetry.hidden ? els.flakyBtn : els.flakyRetry).click();
    await waitFor(() => state.flaky !== null, `flaky retry ${i + 1}`);
  }
  await check(sawError, "flaky showed error state with Retry affordance");
  await check(state.flaky && state.flaky.ok,
    `flaky succeeded (attempt=${state.flaky && state.flaky.attempt})`);

  // 7. Full download to completion (progress bar reaches 100%).
  els.dlBtn.click();
  await waitFor(() => state.download === "done", "full download", 12000);
  await check(state.download === "done" && state.dlReceived === state.dlTotal,
    `full download ${(state.dlReceived / 1048576).toFixed(1)} MiB, ${state.progressEvents} progress events`);

  await say(`SELFTEST DONE pass=${pass} fail=${fail}`);
})();
