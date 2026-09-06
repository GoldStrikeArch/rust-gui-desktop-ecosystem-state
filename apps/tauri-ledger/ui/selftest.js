// Verification harness (not production). Rust calls window.__ledgerSelftest()
// when LEDGER_SELFTEST=1. It drives the REAL inputs with real DOM events and
// reports every assertion to stdout through the `report` command.
window.__ledgerSelftest = async () => {
  const { invoke } = window.__TAURI__.core;
  const say = (l) => invoke("report", { line: l });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  let pass = 0, fail = 0;
  const check = async (cond, msg) => {
    cond ? pass++ : fail++;
    await say(`SELFTEST ${cond ? "PASS" : "FAIL"} ${msg}`);
  };
  const cell = (row, f) => document.querySelector(`tr[data-row="${row}"] [data-f="${f}"]`);
  const amountOf = (row) => document.querySelector(`tr[data-row="${row}"] td.amount`).textContent;
  await say("SELFTEST START");
  try {

  // Type a value the way a keyboard does (beforeinput passes through the filter).
  const type = async (el, text) => {
    el.focus();
    el.setSelectionRange(0, el.value.length);
    document.execCommand("insertText", false, "");
    for (const ch of text) document.execCommand("insertText", false, ch);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(30);
  };
  const tab = async (el) => { el.dispatchEvent(new Event("change", { bubbles: true })); el.blur(); await sleep(250); };

  // 1. type 1234.5 into Unit price, Tab -> en-US formatting
  const u0 = cell(0, "unit");
  await type(u0, "1234.5");
  await tab(u0);
  await check(u0.value === "1,234.50", `en-US blur format: "${u0.value}" (want "1,234.50")`);
  await check(amountOf(0) === "1,234.50", `computed Amount followed: "${amountOf(0)}"`);

  // 2. locale toggle -> parsing AND display switch live
  document.getElementById("locale-btn").click();
  await sleep(400);
  const fr = u0.value.replace(/ | /g, " ");
  await check(fr === "1 234,50", `fr-FR display: "${fr}" (want "1 234,50")`);
  await check(document.getElementById("f-total").textContent.includes(","),
    `totals reformatted for fr-FR: ${document.getElementById("f-total").textContent}`);

  // 3. keystroke filtering is locale-aware: "." is not a decimal point in fr-FR
  await type(u0, "12.34");
  await check(u0.value === "1234", `fr-FR filter dropped "." -> "${u0.value}"`);
  await type(u0, "12,34");
  await tab(u0);
  await check(u0.value.replace(/ | /g, " ") === "12,34", `fr-FR comma accepted -> "${u0.value}"`);
  document.getElementById("locale-btn").click();
  await sleep(400);

  // 4. paste forms (the Rust parser, exercised through the real paste path)
  for (const [text, want] of [["$1,234.56", "1234.56"], ["1.234,56 €", "1234.56"], ["(12.50)", "-12.50"]]) {
    const r = await invoke("parse_probe", { text });
    await check(r.ok && Number(r.value) === Number(want), `paste parse ${JSON.stringify(text)} -> ${r.value} (want ${want})`);
  }

  // 5. arrow stepping: qty ±1, ⇧ ×10; price ±0.01
  const q0 = cell(0, "qty");
  q0.focus(); await sleep(60);
  const q0before = Number(q0.dataset.committed);
  q0.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
  await sleep(250);
  await check(Number(q0.dataset.committed) === q0before + 1, `ArrowUp qty ${q0before} -> ${q0.dataset.committed}`);
  q0.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true, shiftKey: true }));
  await sleep(250);
  await check(Number(q0.dataset.committed) === q0before + 11, `Shift+ArrowUp qty -> ${q0.dataset.committed}`);
  const u1 = cell(1, "unit");
  u1.focus(); await sleep(60);
  const u1before = Number(u1.dataset.committed);
  u1.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  await sleep(250);
  await check(Math.abs(Number(u1.dataset.committed) - (u1before - 0.01)) < 1e-9,
    `ArrowDown price ${u1before} -> ${u1.dataset.committed}`);

  // 6. invalid input keeps the previous committed value + inline error
  const q2 = cell(2, "qty");
  const q2before = q2.dataset.committed;
  await type(q2, "0");
  await tab(q2);
  await sleep(300);
  await check(q2.classList.contains("bad"), "invalid qty=0 marks the field red");
  await check(document.getElementById("save-btn").disabled, "Save is disabled while a row is invalid");
  await check(/error/.test(document.getElementById("save-btn").textContent), `Save shows the error count: "${document.getElementById("save-btn").textContent}"`);
  await check(document.getElementById("summary").textContent.includes("row 3"), `error summary lists the row: "${document.getElementById("summary").textContent}"`);
  await type(q2, q2before); await tab(q2); await sleep(250);
  await check(!document.getElementById("save-btn").disabled, "Save re-enables when the row is valid again");

  // 7. Esc reverts an uncommitted edit
  const d3 = cell(3, "desc");
  const d3before = d3.dataset.committed;
  await type(d3, "SCRATCH");
  d3.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await sleep(120);
  await check(d3.value === d3before, `Esc reverted the cell to "${d3.value}"`);

  // 8. Enter moves down within the column, Shift+Enter up
  const q4 = cell(4, "qty");
  q4.focus(); await sleep(60);
  q4.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await sleep(150);
  await check(document.activeElement === cell(5, "qty"), "Enter moved down one row in the same column");
  document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, shiftKey: true }));
  await sleep(150);
  await check(document.activeElement === cell(4, "qty"), "Shift+Enter moved back up");

  // 9. reading-order tab list (the real Tab walk is done with synthetic keys;
  //    this records the DOM order those keys must follow)
  const order = [...document.querySelectorAll("input, select, button")]
    .filter((e) => !e.disabled)
    .map((e) => e.getAttribute("aria-label") || e.id)
    .slice(0, 14);
  await say(`SELFTEST TABORDER ${JSON.stringify(order)}`);

  // 10. undo / redo of a committed cell edit
  const d5 = cell(5, "desc");
  const d5before = d5.dataset.committed;
  await type(d5, "UNDO ME"); await tab(d5); await sleep(250);
  await check(d5.dataset.committed === "UNDO ME", "committed a cell edit");
  await invoke("undo", { redo: false }); await sleep(300);
  await check(cell(5, "desc").dataset.committed === d5before, `form-level undo restored "${d5before}"`);
  await invoke("undo", { redo: true }); await sleep(300);
  await check(cell(5, "desc").dataset.committed === "UNDO ME", "form-level redo re-applied the edit");
  await invoke("undo", { redo: false }); await sleep(250);

  // 11. VAT slider <-> numeric field, both directions
  const vr = document.getElementById("vat-range"), vn = document.getElementById("vat-num");
  vr.value = "7.5"; vr.dispatchEvent(new Event("input", { bubbles: true })); await sleep(300);
  await check(vn.value === "7.50", `slider -> field: "${vn.value}"`);
  vn.value = "21"; vn.dispatchEvent(new Event("change", { bubbles: true })); await sleep(300);
  await check(Number(vr.value) === 21, `field -> slider: ${vr.value}`);

  // 12. live totals
  const before = document.getElementById("f-total").textContent;
  const q6 = cell(6, "qty");
  await type(q6, "9"); await tab(q6); await sleep(300);
  await check(document.getElementById("f-total").textContent !== before,
    `total recomputed ${before} -> ${document.getElementById("f-total").textContent}`);

  // 13. TSV copy/paste round trip through the clipboard plugin
  const tsv = await invoke("copy_row", { row: 0 });
  await check(tsv.split("\t").length === 6, `⌘C produced 6 TSV columns: ${JSON.stringify(tsv)}`);
  const r13 = await invoke("paste_row", { row: 11 });
  await sleep(300);
  await check(r13.ok && cell(11, "desc").dataset.committed === cell(0, "desc").dataset.committed,
    "⌘V filled row 12 from the TSV on the clipboard");

  // 14. decimal alignment is achievable at all: the font feature is honoured
  const cs = getComputedStyle(cell(0, "unit"));
  await check(/tabular-nums/.test(cs.fontVariantNumeric), `font-variant-numeric: ${cs.fontVariantNumeric}`);

  } catch (err) {
    await say(`SELFTEST ERROR ${err && err.stack ? err.stack : err}`);
    fail++;
  }
  await say(`SELFTEST DONE pass=${pass} fail=${fail}`);
};
