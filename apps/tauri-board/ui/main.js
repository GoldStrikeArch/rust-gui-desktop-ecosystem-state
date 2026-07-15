// Board (Tauri) frontend — vanilla JS, no external libraries.
//
// All board state lives here in the webview (see FRICTION.md for the
// justification vs. Rust-owned state). Drag-and-drop is HTML5 drag events
// (native drag ghost); the drop indicator is a single line element moved
// around with insertBefore; drop/reorder/delete animate via a hand-rolled
// FLIP pass over persistent card elements.

let nextId = 1;
const card = (text) => ({ id: nextId++, text });

const state = [
  { name: "Todo", cards: [card("Write FRICTION.md"), card("Measure IPC latency")] },
  { name: "Doing", cards: [card("Build the kanban board")] },
  { name: "Done", cards: [card("Project scaffolding"), card("Window + column layout")] },
];

const board = document.getElementById("board");
const columns = []; // per column: { root, cardsEl, countEl, addBtn, addInput }
const cardEls = new Map(); // card id -> persistent DOM element (needed for FLIP)

// The single drop indicator, moved between columns with insertBefore.
const indicator = document.createElement("div");
indicator.className = "drop-indicator";

let drag = null; // { id } while a card drag is in flight

// ---------- rendering ----------

function locate(id) {
  for (let c = 0; c < state.length; c++) {
    const i = state[c].cards.findIndex((k) => k.id === id);
    if (i >= 0) return [c, i];
  }
  return null;
}

function cardEl(k) {
  let el = cardEls.get(k.id);
  if (!el) {
    el = document.createElement("div");
    el.className = "kcard";
    el.draggable = true;

    const text = document.createElement("span");
    text.className = "kcard-text";

    const del = document.createElement("button");
    del.type = "button";
    del.className = "kcard-del";
    del.textContent = "✕";
    del.setAttribute("aria-label", "Delete card");
    del.addEventListener("click", (e) => {
      e.stopPropagation();
      const [c, i] = locate(k.id);
      cardEls.delete(k.id);
      el.remove(); // drop it from the DOM first so FLIP only moves survivors
      flip(() => {
        state[c].cards.splice(i, 1);
        render();
      });
    });

    el.append(text, del);

    el.addEventListener("dblclick", () => startEdit(el, k));

    el.addEventListener("dragstart", (e) => {
      drag = { id: k.id };
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(k.id));
      // Defer so the native drag ghost snapshots the un-dimmed card.
      setTimeout(() => el.classList.add("dragging"), 0);
    });
    el.addEventListener("dragend", () => {
      el.classList.remove("dragging");
      indicator.remove();
      drag = null;
    });

    cardEls.set(k.id, el);
  }
  el.querySelector(".kcard-text").textContent = k.text;
  return el;
}

function render() {
  for (let c = 0; c < state.length; c++) {
    const col = columns[c];
    col.countEl.textContent = String(state[c].cards.length);
    // Re-append persistent elements in order; appendChild moves, not clones,
    // so element identity survives for FLIP and for in-flight edits.
    for (const k of state[c].cards) col.cardsEl.appendChild(cardEl(k));
  }
}

// FLIP: record every card's rect, mutate + re-render, then transition each
// surviving card from its old position to its new one.
function flip(mutate) {
  const before = new Map();
  for (const [id, el] of cardEls) before.set(id, el.getBoundingClientRect());
  mutate();
  for (const [id, el] of cardEls) {
    const b = before.get(id);
    if (!b) { // newly added card: fade/slide in instead
      el.classList.add("kcard-new");
      el.addEventListener("animationend", () => el.classList.remove("kcard-new"), { once: true });
      continue;
    }
    const a = el.getBoundingClientRect();
    const dx = b.left - a.left, dy = b.top - a.top;
    if (!dx && !dy) continue;
    el.style.transition = "none";
    el.style.transform = `translate(${dx}px, ${dy}px)`;
    requestAnimationFrame(() => {
      el.style.transition = "transform 200ms ease";
      el.style.transform = "";
      el.addEventListener("transitionend", () => { el.style.transition = ""; }, { once: true });
    });
  }
}

// ---------- drag and drop ----------

// Insertion index within column c for a pointer at clientY, expressed against
// the card list *excluding* the dragged card (which is how the state splice
// works after removal).
function insertionIndex(c, clientY) {
  let idx = 0;
  for (const k of state[c].cards) {
    if (drag && k.id === drag.id) continue;
    const r = cardEls.get(k.id).getBoundingClientRect();
    if (clientY < r.top + r.height / 2) break;
    idx++;
  }
  return idx;
}

// Places the indicator before the idx-th non-dragged card of column c.
function placeIndicator(c, idx) {
  const col = columns[c];
  let seen = 0, anchor = null;
  for (const k of state[c].cards) {
    if (drag && k.id === drag.id) continue;
    if (seen === idx) { anchor = cardEls.get(k.id); break; }
    seen++;
  }
  col.cardsEl.insertBefore(indicator, anchor); // null anchor = append at end
}

function wireColumnDnd(c, colRoot) {
  colRoot.addEventListener("dragover", (e) => {
    if (!drag) return;
    e.preventDefault(); // required to allow the drop
    e.dataTransfer.dropEffect = "move";
    placeIndicator(c, insertionIndex(c, e.clientY));
  });
  colRoot.addEventListener("dragleave", (e) => {
    if (!colRoot.contains(e.relatedTarget)) indicator.remove();
  });
  colRoot.addEventListener("drop", (e) => {
    if (!drag) return;
    e.preventDefault();
    const idx = insertionIndex(c, e.clientY);
    indicator.remove();
    const [fc, fi] = locate(drag.id);
    if (fc === c && fi === idx) return; // dropped back onto its own slot
    flip(() => {
      const [k] = state[fc].cards.splice(fi, 1);
      state[c].cards.splice(idx, 0, k);
      render();
    });
  });
}

// ---------- inline edit (double-click, Enter commits, Esc cancels) ----------

function startEdit(el, k) {
  if (el.classList.contains("editing")) return;
  el.classList.add("editing");
  el.draggable = false; // so text selection inside the input works
  const text = el.querySelector(".kcard-text");
  const input = document.createElement("input");
  input.type = "text";
  input.className = "kcard-edit";
  input.value = k.text;
  text.replaceWith(input);
  input.focus();
  input.select();

  const finish = (commit) => {
    if (commit && input.value.trim()) k.text = input.value.trim();
    input.replaceWith(text);
    text.textContent = k.text;
    el.classList.remove("editing");
    el.draggable = true;
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") finish(true);
    else if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => {
    if (el.classList.contains("editing")) finish(false);
  });
}

// ---------- column construction ----------

function buildColumn(c, name) {
  const root = document.createElement("section");
  root.className = "column";

  const header = document.createElement("header");
  const title = document.createElement("h2");
  title.textContent = name;
  const count = document.createElement("span");
  count.className = "count";
  count.textContent = "0";
  header.append(title, count);

  const cardsEl = document.createElement("div");
  cardsEl.className = "cards"; // independently scrollable

  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "add-btn";
  addBtn.textContent = "+ Add card";

  const addInput = document.createElement("input");
  addInput.type = "text";
  addInput.className = "add-input";
  addInput.placeholder = "Card text… (Enter adds, Esc closes)";
  addInput.hidden = true;

  addBtn.addEventListener("click", () => {
    addBtn.hidden = true;
    addInput.hidden = false;
    addInput.focus();
  });
  const closeAdd = () => {
    addInput.value = "";
    addInput.hidden = true;
    addBtn.hidden = false;
  };
  addInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const t = addInput.value.trim();
      if (!t) return; // empty input is ignored
      flip(() => {
        state[c].cards.push(card(t));
        render();
      });
      addInput.value = ""; // stay open for consecutive adds
    } else if (e.key === "Escape") {
      closeAdd();
    }
  });
  addInput.addEventListener("blur", closeAdd);

  root.append(header, cardsEl, addBtn, addInput);
  wireColumnDnd(c, root);
  board.appendChild(root);
  columns[c] = { root, cardsEl, countEl: count, addBtn, addInput };
}

state.forEach((col, c) => buildColumn(c, col.name));
render();
