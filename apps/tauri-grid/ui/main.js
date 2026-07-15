// Grid (Tauri) — SPEC-7 frontend. The webview is a windowed view over the
// Rust-owned dataset: it renders only the viewport slice and fetches row
// windows over IPC (`get_rows`). Virtualization is hand-rolled: fixed row
// height + scroll math + one absolutely-positioned slice under a spacer div.
// No external JS libraries.
const { invoke } = window.__TAURI__.core;

const ROW_H = 28;        // must match --row-h in styles.css
const HEADER_H = 28;     // sticky header occupies the first row height
const OVERSCAN = 8;      // extra rows rendered above/below the viewport
const FETCH_PAD = 48;    // extra rows fetched around the render range

const COLS = [
  { label: "ID" },
  { label: "Name" },
  { label: "Category" },
  { label: "Value" },
  { label: "Date" },
  { label: "Status" },
];

const vp = document.getElementById("viewport");
const headerEl = document.getElementById("header");
const spacer = document.getElementById("spacer");
const slice = document.getElementById("slice");
const filterInput = document.getElementById("filter-input");
const rowCount = document.getElementById("row-count");

const state = {
  total: 0,
  viewLen: 0,
  sortCol: null,
  sortAsc: true,
  gen: 0,          // invalidation counter: bumped on filter/sort change
  cache: null,     // { start, rows: RowDto[] } — last fetched window
  inflight: null,  // in-flight get_rows promise (one at a time)
  filterSeq: 0,    // stale-response guard for set_filter round-trips
  filterApplied: 0, // last filterSeq whose response has been applied
  selected: new Set(), // selected view indices (cleared on filter/sort)
  anchor: null,    // view index of the last plain click (shift-range anchor)
};

// ---------- header (labels, sort indicators, resize dividers) ----------

function buildHeader() {
  headerEl.replaceChildren(
    ...COLS.map((col, i) => {
      const cell = document.createElement("div");
      cell.className = `cell c${i}`;
      cell.dataset.col = i;
      const label = document.createElement("span");
      label.textContent = col.label;
      const ind = document.createElement("span");
      ind.className = "sort-ind";
      ind.dataset.ind = i;
      cell.append(label, ind);
      cell.addEventListener("click", () => sortBy(i));

      const divider = document.createElement("div");
      divider.className = "divider";
      divider.dataset.divider = i;
      divider.addEventListener("click", (e) => e.stopPropagation());
      divider.addEventListener("pointerdown", (e) => startResize(e, i, divider));
      cell.append(divider);
      return cell;
    })
  );
}

function updateSortIndicators() {
  for (const ind of headerEl.querySelectorAll(".sort-ind")) {
    const i = Number(ind.dataset.ind);
    ind.textContent = i === state.sortCol ? (state.sortAsc ? "▲" : "▼") : "";
  }
}

// Column resize: pointer events + CSS custom properties. pointermove/up go on
// window (not relying on setPointerCapture, which throws for synthetic
// pointerIds in the self-test).
function startResize(e, col, divider) {
  e.preventDefault();
  try { divider.setPointerCapture(e.pointerId); } catch (_) {}
  divider.classList.add("dragging");
  const app = document.querySelector(".app");
  const startX = e.clientX;
  const startW = parseFloat(getComputedStyle(app).getPropertyValue(`--w-${col}`));
  const onMove = (ev) => {
    const w = Math.max(50, Math.min(600, startW + (ev.clientX - startX)));
    app.style.setProperty(`--w-${col}`, `${w}px`);
  };
  const onUp = () => {
    divider.classList.remove("dragging");
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
}

// ---------- virtualized body ----------

function renderRange() {
  const first = Math.max(0, Math.floor(vp.scrollTop / ROW_H) - OVERSCAN);
  const visible = Math.ceil(Math.max(0, vp.clientHeight - HEADER_H) / ROW_H) + 1;
  const last = Math.min(state.viewLen, first + visible + 2 * OVERSCAN);
  return [first, last];
}

function buildRow(dto) {
  const row = document.createElement("div");
  row.className = "row" + (dto.vi % 2 === 0 ? " even" : "");
  if (state.selected.has(dto.vi)) row.classList.add("selected");
  row.dataset.vi = dto.vi;
  const cells = [dto.id, dto.name, dto.category, dto.value, dto.date].map((v, i) => {
    const c = document.createElement("div");
    c.className = `cell c${i}`;
    c.textContent = v;
    return c;
  });
  // Custom cell rendering: status as a colored chip.
  const sc = document.createElement("div");
  sc.className = "cell c5";
  const chip = document.createElement("span");
  chip.className = `chip chip-${dto.status.toLowerCase()}`;
  chip.textContent = dto.status;
  sc.append(chip);
  cells.push(sc);
  row.append(...cells);
  return row;
}

function buildPlaceholder(vi) {
  const row = document.createElement("div");
  row.className = "row placeholder" + (vi % 2 === 0 ? " even" : "");
  const c = document.createElement("div");
  c.className = "cell c0";
  c.textContent = "…";
  row.append(c);
  return row;
}

function render() {
  const [first, last] = renderRange();
  slice.style.transform = `translateY(${first * ROW_H}px)`;
  const nodes = [];
  for (let vi = first; vi < last; vi++) {
    const c = state.cache;
    const dto = c && vi >= c.start && vi < c.start + c.rows.length ? c.rows[vi - c.start] : null;
    nodes.push(dto ? buildRow(dto) : buildPlaceholder(vi));
  }
  slice.replaceChildren(...nodes);
}

// Fetches a window covering the render range (+FETCH_PAD) unless the cache
// already covers it. One request in flight at a time; a response from a
// stale generation (filter/sort changed meanwhile) is discarded.
function ensureWindow() {
  const [first, last] = renderRange();
  const c = state.cache;
  if (c && first >= c.start && last <= c.start + c.rows.length) return;
  if (state.inflight) return;
  const start = Math.max(0, first - FETCH_PAD);
  const count = Math.min(state.viewLen, last + FETCH_PAD) - start;
  const gen = state.gen;
  state.inflight = invoke("get_rows", { start, count })
    .then((rows) => {
      state.inflight = null;
      if (gen === state.gen) {
        state.cache = { start, rows };
        render();
      }
      // Re-check even for a stale-generation response: the view changed while
      // we were in flight and the fresh window still needs fetching.
      ensureWindow();
    })
    .catch((e) => {
      state.inflight = null;
      reportErr(`get_rows failed: ${e}`);
    });
}

let rafPending = false;
function scheduleRender() {
  if (rafPending) return;
  rafPending = true;
  requestAnimationFrame(() => {
    rafPending = false;
    render();
    ensureWindow();
  });
}

function setViewLen(n) {
  state.viewLen = n;
  spacer.style.height = `${n * ROW_H}px`;
  rowCount.textContent = `${n.toLocaleString("en-US")} of ${state.total.toLocaleString("en-US")} rows`;
}

function invalidate() {
  state.gen++;
  state.cache = null;
  state.selected.clear();
  state.anchor = null;
}

// ---------- interactions ----------

vp.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", scheduleRender);

// Filter-as-you-type: every keystroke is one IPC round-trip; Rust rebuilds
// the view, prints FILTER_MS, and returns the new view length. filterSeq
// guards against out-of-order responses.
filterInput.addEventListener("input", async () => {
  const seq = ++state.filterSeq;
  const len = await invoke("set_filter", { q: filterInput.value });
  if (seq !== state.filterSeq) return; // a newer keystroke already answered
  state.filterApplied = seq;
  invalidate();
  setViewLen(len);
  vp.scrollTop = 0;
  render();
  ensureWindow();
});

async function sortBy(col) {
  const res = await invoke("set_sort", { col });
  state.sortCol = res.col;
  state.sortAsc = res.asc;
  invalidate();
  setViewLen(res.viewLen);
  updateSortIndicators();
  render();
  ensureWindow();
}

// Row selection: click selects; shift-click selects the contiguous range
// from the last plain-clicked row (anchor). Selection is view-relative and
// cleared when filter/sort changes (documented in FRICTION.md).
slice.addEventListener("click", (e) => {
  const row = e.target.closest(".row");
  if (!row || row.dataset.vi === undefined) return;
  const vi = Number(row.dataset.vi);
  if (e.shiftKey && state.anchor !== null) {
    const [lo, hi] = [Math.min(state.anchor, vi), Math.max(state.anchor, vi)];
    state.selected = new Set();
    for (let i = lo; i <= hi; i++) state.selected.add(i);
  } else {
    state.selected = new Set([vi]);
    state.anchor = vi;
  }
  render();
});

// ---------- error + init ----------

function reportErr(line) {
  invoke("report", { line: `JSERR ${line}` }).catch(() => {});
}
window.onerror = (msg, src, ln) => reportErr(`${msg} @ ${src}:${ln}`);

const ready = (async () => {
  const meta = await invoke("get_meta");
  state.total = meta.total;
  buildHeader();
  setViewLen(meta.viewLen);
  render();
  ensureWindow();
  return meta;
})();

// Hooks for selftest.js (no-op in normal runs).
window.__grid = { state, vp, slice, headerEl, filterInput, ready, render, invoke };
