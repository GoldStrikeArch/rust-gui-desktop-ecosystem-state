// Self-test harness (verification code, not production). Runs only when the
// app was launched with GRID_SELFTEST=1 (flag arrives via get_meta). Drives
// the real UI in the real webview — scroll positions, synthetic click /
// pointer events — and reports assertions to stdout via the `report` command.
(async () => {
  const g = window.__grid;
  const meta = await g.ready;
  if (!meta.selftest) return;

  const say = (line) => g.invoke("report", { line });
  let pass = 0, fail = 0;
  const check = async (cond, msg) => {
    cond ? pass++ : fail++;
    await say(`SELFTEST ${cond ? "PASS" : "FAIL"} ${msg}`);
  };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (fn, what, timeout = 3000) => {
    const t0 = performance.now();
    while (performance.now() - t0 < timeout) {
      try { if (fn()) return true; } catch (_) {}
      await sleep(25);
    }
    await say(`SELFTEST TIMEOUT waiting for ${what}`);
    return false;
  };
  const rowAt = (vi) => g.slice.querySelector(`.row[data-vi="${vi}"]:not(.placeholder)`);
  const firstRealRow = () =>
    g.slice.querySelector(".row:not(.placeholder)");

  // 1. Initial load: first window rendered from Rust.
  await waitFor(() => rowAt(0), "initial window");
  const r0 = rowAt(0);
  await check(!!r0 && r0.children[0].textContent === "1", "initial row vi=0 has id=1");
  await check(!!g.slice.querySelector(".chip.chip-ok, .chip.chip-warn, .chip.chip-err"),
    "status chips rendered (custom cell)");
  await say(`SELFTEST LOADED viewLen=${g.state.viewLen}`);
  await sleep(1200); // RSS-after-load checkpoint window for the outer script

  // 2. Long scroll: middle then bottom (virtualization + windowed IPC).
  g.vp.scrollTop = Math.floor(g.state.viewLen / 2) * 28;
  const midVi = Math.floor(g.vp.scrollTop / 28);
  await waitFor(() => rowAt(midVi), "mid-scroll window");
  await check(!!rowAt(midVi), `windowed fetch at mid scroll (vi=${midVi})`);
  g.vp.scrollTop = g.vp.scrollHeight;
  await waitFor(() => rowAt(g.state.viewLen - 1), "bottom window");
  await check(!!rowAt(g.state.viewLen - 1), "windowed fetch at bottom (last row rendered)");

  // 3. Sort by Name asc, then desc (header clicks).
  g.vp.scrollTop = 0;
  const nameHeader = g.headerEl.querySelector('.cell[data-col="1"]');
  nameHeader.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await waitFor(() => g.state.sortCol === 1 && g.state.sortAsc && rowAt(0), "name asc");
  let a = rowAt(0), b = rowAt(1);
  await check(a && b && a.children[1].textContent <= b.children[1].textContent,
    `sort name asc (${a && a.children[1].textContent} <= ${b && b.children[1].textContent})`);
  await check(g.headerEl.querySelector('[data-ind="1"]').textContent === "▲", "sort indicator ▲");
  nameHeader.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await waitFor(() => !g.state.sortAsc && rowAt(0), "name desc");
  a = rowAt(0); b = rowAt(1);
  await check(a && b && a.children[1].textContent >= b.children[1].textContent, "sort name desc");
  await check(g.headerEl.querySelector('[data-ind="1"]').textContent === "▼", "sort indicator ▼");

  // 4. Filter-as-you-type: "p", "pr", "pri", "prim" (Rust prints FILTER_MS).
  const type = async (q) => {
    g.filterInput.value = q;
    g.filterInput.dispatchEvent(new Event("input", { bubbles: true }));
    const seq = g.state.filterSeq; // incremented synchronously by the listener
    await waitFor(() => g.state.filterApplied >= seq && !g.state.inflight && firstRealRow(),
      `filter "${q}" applied`);
  };
  for (const q of ["p", "pr", "pri", "prim"]) await type(q);
  const shown = [...g.slice.querySelectorAll(".row:not(.placeholder)")];
  await check(g.state.viewLen > 0 && g.state.viewLen < 100000,
    `filter "prim" narrowed to ${g.state.viewLen} rows`);
  await check(shown.length > 0 && shown.every((r) => r.children[1].textContent.includes("prim")),
    "all rendered rows match filter");
  await check(document.getElementById("row-count").textContent ===
    `${g.state.viewLen.toLocaleString("en-US")} of 100,000 rows`, "count label format");
  await type(""); // clear
  await check(g.state.viewLen === 100000, "filter cleared back to 100,000");

  // 5. Selection: click vi=2, shift-click vi=6 -> 5 selected rows.
  await waitFor(() => rowAt(2) && rowAt(6), "rows for selection");
  rowAt(2).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  rowAt(6).dispatchEvent(new MouseEvent("click", { bubbles: true, shiftKey: true }));
  await check(g.slice.querySelectorAll(".row.selected").length === 5,
    "shift-click selected range of 5");

  // 6. Column resize: drag the Name divider +80px via pointer events.
  const app = document.querySelector(".app");
  const wBefore = parseFloat(getComputedStyle(app).getPropertyValue("--w-1"));
  const divider = g.headerEl.querySelector('[data-divider="1"]');
  const pe = (type, x) => new PointerEvent(type, { bubbles: true, clientX: x, pointerId: 7 });
  divider.dispatchEvent(pe("pointerdown", 300));
  window.dispatchEvent(pe("pointermove", 340));
  window.dispatchEvent(pe("pointermove", 380));
  window.dispatchEvent(pe("pointerup", 380));
  const wAfter = parseFloat(getComputedStyle(app).getPropertyValue("--w-1"));
  await check(Math.abs(wAfter - wBefore - 80) < 1,
    `column resize ${wBefore}px -> ${wAfter}px`);

  await say(`SELFTEST DONE pass=${pass} fail=${fail}`);

  // 7. Sustained scroll burst (RSS-after-long-scroll checkpoint follows).
  for (let i = 0; i < 24; i++) {
    g.vp.scrollTop = (i * 977 % g.state.viewLen) * 28;
    await sleep(90);
  }
  await say("SELFTEST SCROLLDONE");
})();
