// Ledger frontend. It does exactly two number things: FILTER keystrokes and
// FORMAT for display with Intl.NumberFormat. Every parse, step, clamp,
// validation and total happens in Rust over rust_decimal.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const say = (line) => invoke("report", { line });

let M = null;
const rowsEl = document.getElementById("rows");

// ---- locale-derived separators, straight from the platform's ICU ----------
let DEC = ".", GRP = ",";
function separators(loc) {
  const parts = new Intl.NumberFormat(loc).formatToParts(1234567.5);
  DEC = (parts.find((p) => p.type === "decimal") || { value: "." }).value;
  GRP = (parts.find((p) => p.type === "group") || { value: "," }).value;
}
const money = (loc) =>
  new Intl.NumberFormat(loc, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const fmt = (canonical, loc) => money(loc).format(Number(canonical));
// the same value without grouping, in the locale's decimal separator — what a
// field shows while it is being edited
const plain = (canonical) => canonical.replace(".", DEC);
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function acceptable(text, kind) {
  if (text === "" || text === "-") return true;
  const g = `[0-9${esc(GRP)}\\u00a0\\u202f ]`;
  const re =
    kind === "int"
      ? new RegExp(`^-?${g}*$`)
      : new RegExp(`^-?${g}*(${esc(DEC)}[0-9]{0,2})?$`);
  return re.test(text);
}

// ---- rendering -----------------------------------------------------------
function cellInput(i, field, attrs) {
  const el = document.createElement(attrs.tag || "input");
  el.dataset.row = i;
  el.dataset.f = field;
  Object.assign(el, attrs.props || {});
  for (const [k, v] of Object.entries(attrs.attrs || {})) el.setAttribute(k, v);
  return el;
}

function render(m) {
  const first = !rowsEl.firstChild;
  separators(m.locale);
  document.getElementById("locale-btn").textContent = `Locale: ${m.locale}`;

  if (first) {
    m.rows.forEach((r, i) => {
      const tr = document.createElement("tr");
      tr.dataset.row = i;

      const tdD = document.createElement("td");
      tdD.appendChild(cellInput(i, "date", { attrs: { type: "date", "aria-label": `Row ${i + 1} date` } }));
      const tdT = document.createElement("td");
      tdT.appendChild(cellInput(i, "desc", { attrs: { type: "text", maxlength: "80", "aria-label": `Row ${i + 1} description` } }));
      const tdC = document.createElement("td");
      const sel = cellInput(i, "cat", { tag: "select", attrs: { "aria-label": `Row ${i + 1} category` } });
      for (const c of m.categories) {
        const o = document.createElement("option");
        o.value = o.textContent = c;
        sel.appendChild(o);
      }
      tdC.appendChild(sel);
      const tdQ = document.createElement("td");
      tdQ.className = "c-num";
      tdQ.appendChild(cellInput(i, "qty", { attrs: { type: "text", inputmode: "numeric", class: "num", "aria-label": `Row ${i + 1} quantity` } }));
      const tdU = document.createElement("td");
      tdU.className = "c-num";
      tdU.appendChild(cellInput(i, "unit", { attrs: { type: "text", inputmode: "decimal", class: "num", "aria-label": `Row ${i + 1} unit price` } }));
      const tdA = document.createElement("td");
      tdA.className = "c-num amount";
      const tdR = document.createElement("td");
      tdR.className = "c-chk";
      tdR.appendChild(cellInput(i, "reimb", { attrs: { type: "checkbox", "aria-label": `Row ${i + 1} reimbursable` } }));
      tr.append(tdD, tdT, tdC, tdQ, tdU, tdA, tdR);
      rowsEl.appendChild(tr);
    });
  }

  m.rows.forEach((r, i) => {
    const tr = rowsEl.children[i];
    const q = (f) => tr.querySelector(`[data-f="${f}"]`);
    const set = (el, v) => {
      if (document.activeElement !== el && el.value !== v) el.value = v;
      el.dataset.committed = v;
    };
    set(q("date"), r.date);
    set(q("desc"), r.desc);
    set(q("cat"), r.cat);
    // numbers: formatted when idle, plain while the caret is in the field
    const qEl = q("qty"), uEl = q("unit");
    set(qEl, document.activeElement === qEl ? qEl.value : new Intl.NumberFormat(m.locale).format(Number(r.qty)));
    qEl.dataset.committed = r.qty;
    set(uEl, document.activeElement === uEl ? uEl.value : fmt(r.unit, m.locale));
    uEl.dataset.committed = r.unit;
    q("reimb").checked = r.reimb;
    for (const f of ["desc", "qty", "unit"]) q(f).classList.toggle("bad", r.errors.includes(f));
    const amt = tr.children[5];
    const neg = Number(r.amount) < 0;
    amt.textContent = neg ? `(${fmt(r.amount.replace("-", ""), m.locale)})` : fmt(r.amount, m.locale);
    amt.classList.toggle("neg", neg);
  });

  document.getElementById("f-subtotal").textContent = fmt(m.subtotal, m.locale);
  document.getElementById("f-vat").textContent = fmt(m.vatAmount, m.locale);
  document.getElementById("f-vatlabel").textContent = `VAT ${fmt(m.vat, m.locale)} %`;
  document.getElementById("f-total").textContent = fmt(m.total, m.locale);

  const vn = document.getElementById("vat-num");
  if (document.activeElement !== vn) vn.value = plain(m.vat);
  document.getElementById("vat-range").value = m.vat;

  const save = document.getElementById("save-btn");
  save.disabled = m.errors.length > 0;
  save.textContent = m.errors.length ? `Save (${m.errors.length} error${m.errors.length > 1 ? "s" : ""})` : "Save";
  document.getElementById("summary").textContent = m.errors.join(" · ");
}

// ---- editing behaviour ---------------------------------------------------
const isNum = (el) => el.dataset.f === "qty" || el.dataset.f === "unit";
const kindOf = (el) => (el.dataset.f === "qty" ? "int" : "dec");

rowsEl.addEventListener("focusin", (e) => {
  const el = e.target;
  if (isNum(el)) el.value = plain(el.dataset.committed); // drop grouping to edit
  say(`FOCUS row=${el.dataset.row} field=${el.dataset.f}`);
});

rowsEl.addEventListener("beforeinput", (e) => {
  const el = e.target;
  if (!isNum(el) || e.inputType === "insertFromPaste") return;
  const v = el.value;
  const next = v.slice(0, el.selectionStart) + (e.data || "") + v.slice(el.selectionEnd);
  if (!acceptable(next, kindOf(el))) {
    e.preventDefault();
    say(`FILTER rejected ${JSON.stringify(next)} for ${el.dataset.f}`);
  }
});

rowsEl.addEventListener("input", (e) => {
  e.target.dataset.dirty = "1";
});

async function commit(el) {
  if (el.dataset.dirty !== "1") return;
  delete el.dataset.dirty;
  const value = el.type === "checkbox" ? String(el.checked) : el.value;
  const r = await invoke("set_cell", { row: +el.dataset.row, field: el.dataset.f, value });
  if (!r.ok) {
    el.classList.add("bad");
    document.getElementById("summary").textContent =
      `row ${+el.dataset.row + 1} · ${el.dataset.f}: ${r.message} (kept ${r.value})`;
    el.value = isNum(el) ? plain(r.value) : r.value; // previous committed value
    say(`REJECT ${el.dataset.f} ${JSON.stringify(value)} -> ${r.message}`);
  }
}

rowsEl.addEventListener("change", (e) => commit(e.target));
rowsEl.addEventListener("focusout", (e) => commit(e.target));

const cellsInColumn = (f) => [...rowsEl.querySelectorAll(`[data-f="${f}"]`)];

rowsEl.addEventListener("keydown", async (e) => {
  const el = e.target;
  const row = +el.dataset.row;
  if (e.key === "Enter") {
    e.preventDefault();
    await commit(el);
    const col = cellsInColumn(el.dataset.f);
    const next = col[row + (e.shiftKey ? -1 : 1)];
    if (next) { next.focus(); if (next.select) next.select(); }
    return;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    delete el.dataset.dirty;
    el.value = isNum(el) ? plain(el.dataset.committed) : el.dataset.committed;
    el.classList.remove("bad");
    say(`ESC reverted row=${row} field=${el.dataset.f}`);
    return;
  }
  if (isNum(el) && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
    e.preventDefault();
    delete el.dataset.dirty;
    const r = await invoke("step_cell", {
      row, field: el.dataset.f, text: el.value,
      units: e.key === "ArrowUp" ? 1 : -1, shift: e.shiftKey,
    });
    el.value = plain(r.value);
  }
});

// Paste goes to Rust unparsed: "$1,234.56", "1.234,56 €" and "(12.50)" are the
// parser's job, not a regex's.
rowsEl.addEventListener("paste", async (e) => {
  const el = e.target;
  if (!isNum(el)) return;
  e.preventDefault();
  const text = e.clipboardData.getData("text");
  delete el.dataset.dirty;
  const r = await invoke("set_cell", { row: +el.dataset.row, field: el.dataset.f, value: text });
  el.value = plain(r.value);
  say(`PASTE ${JSON.stringify(text)} -> ${r.ok ? r.value : "REJECTED " + r.message}`);
});

// ---- toolbar -------------------------------------------------------------
document.getElementById("locale-btn").onclick = () =>
  invoke("set_locale", { locale: M.locale === "en-US" ? "fr-FR" : "en-US" });
document.getElementById("vat-range").oninput = (e) =>
  invoke("set_vat", { value: e.target.value });
document.getElementById("vat-num").onchange = (e) =>
  invoke("set_vat", { value: e.target.value });
document.getElementById("save-btn").onclick = () => say("SAVE clicked (form valid)");

const focusedRow = () => {
  const a = document.activeElement;
  return a && a.dataset && a.dataset.row !== undefined ? +a.dataset.row : 0;
};
document.getElementById("copy-btn").onclick = () => invoke("copy_row", { row: focusedRow() });
document.getElementById("paste-btn").onclick = () => invoke("paste_row", { row: focusedRow() });

window.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey)) return;
  const a = document.activeElement;
  const inField = a && a.tagName === "INPUT" && a.type === "text";
  if (e.key.toLowerCase() === "z") {
    // A field with an uncommitted edit keeps ⌘Z for WebKit's own text undo;
    // otherwise ⌘Z is form-level undo of the last committed cell edit.
    if (inField && a.dataset.dirty === "1") return;
    e.preventDefault();
    invoke("undo", { redo: e.shiftKey });
  } else if (e.key.toLowerCase() === "c" && !inField) {
    e.preventDefault();
    invoke("copy_row", { row: focusedRow() });
  } else if (e.key.toLowerCase() === "v" && !inField) {
    e.preventDefault();
    invoke("paste_row", { row: focusedRow() });
  }
});

window.onerror = (m, s, l) => say(`JSERROR ${m} @${l}`);

(async () => {
  await listen("model", (e) => { M = e.payload; render(M); });
  M = await invoke("get_model");
  render(M);
  window.__ledgerReady = M;
})();
