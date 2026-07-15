// Pulse (Tauri) frontend — vanilla JS + Canvas2D, no external libraries.
//
// Rust pushes a `tick` event (batch of 6 metric values) at the tick rate over
// Tauri's IPC event bridge; this file keeps ring buffers of the last 300
// samples, draws sparklines + the main chart on <canvas>, and hand-rolls
// card drag-reorder (HTML5 drag events + FLIP animation), click-to-select,
// and the hover crosshair/tooltip. Repaints are coalesced to one
// requestAnimationFrame per display frame regardless of the tick rate.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const METRICS = [
  { name: "CPU", unit: "%", color: "#5b9cf5" },
  { name: "Memory", unit: "%", color: "#a97ef7" },
  { name: "Net In", unit: " MB/s", color: "#34c98e" },
  { name: "Net Out", unit: " MB/s", color: "#f0b04b" },
  { name: "Disk", unit: "%", color: "#f06868" },
  { name: "Requests", unit: "/s", color: "#3fc3d8" },
];
const HISTORY = 300; // main chart window
const SPARK = 60; // sparkline window

const buffers = METRICS.map(() => []); // per-metric sample arrays, capped at HISTORY
let order = [0, 1, 2, 3, 4, 5]; // card order (values = metric indices)
let selected = 0;
let latestSeq = -1;
let paused = false;
let hover = null; // {x, y} in chart CSS px, or null

const grid = document.getElementById("card-grid");
const chartCanvas = document.getElementById("chart");
const chartCtx = chartCanvas.getContext("2d");
const chartWrap = chartCanvas.parentElement;
const tooltip = document.getElementById("tooltip");
const chartTitle = document.getElementById("chart-title");
const pauseBtn = document.getElementById("pause-btn");
const rateSlider = document.getElementById("rate-slider");
const rateLabel = document.getElementById("rate-label");

// ---------- canvas helpers ----------

function fitCanvas(canvas) {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
    canvas.width = w * dpr;
    canvas.height = h * dpr;
  }
  canvas.getContext("2d").setTransform(dpr, 0, 0, dpr, 0, 0);
}

function extent(data) {
  let lo = Infinity, hi = -Infinity;
  for (const v of data) { if (v < lo) lo = v; if (v > hi) hi = v; }
  if (!isFinite(lo)) { lo = 0; hi = 1; }
  if (hi - lo < 1e-9) { lo -= 1; hi += 1; }
  const pad = (hi - lo) * 0.12;
  return [lo - pad, hi + pad];
}

// ---------- metric cards ----------

const cards = []; // per metric index: { root, valueEl, spark }

function buildCards() {
  for (let m = 0; m < METRICS.length; m++) {
    const root = document.createElement("div");
    root.className = "card";
    root.draggable = true;
    root.dataset.metric = String(m);

    const name = document.createElement("div");
    name.className = "card-name";
    name.textContent = METRICS[m].name;

    const value = document.createElement("div");
    value.className = "card-value";
    value.textContent = "–";
    value.style.color = METRICS[m].color;

    const spark = document.createElement("canvas");
    spark.className = "spark";

    root.append(name, value, spark);
    cards[m] = { root, valueEl: value, spark };

    root.addEventListener("click", () => selectMetric(m));
    root.addEventListener("dragstart", (e) => {
      dragFrom = order.indexOf(m);
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", String(m));
      // Defer the style change so the native drag snapshot is the full card.
      setTimeout(() => root.classList.add("dragging"), 0);
    });
    root.addEventListener("dragend", () => {
      root.classList.remove("dragging");
      clearSlotCue();
      dragFrom = null;
    });

    grid.appendChild(root);
  }
  syncSelection();
}

function selectMetric(m) {
  selected = m;
  syncSelection();
  scheduleDraw();
}

function syncSelection() {
  for (let m = 0; m < cards.length; m++) {
    cards[m].root.classList.toggle("selected", m === selected);
  }
  chartTitle.innerHTML = "";
  chartTitle.append(METRICS[selected].name, Object.assign(document.createElement("span"), {
    className: "chart-sub",
    textContent: " last 300 samples",
  }));
}

// ---------- card drag-reorder (HTML5 drag events + FLIP) ----------

let dragFrom = null; // position of the dragged card in `order`, or null

function nearestSlot(x, y) {
  let best = -1, bestDist = Infinity;
  const els = [...grid.children];
  for (let i = 0; i < els.length; i++) {
    const r = els[i].getBoundingClientRect();
    const dx = x - (r.left + r.width / 2);
    const dy = y - (r.top + r.height / 2);
    const d = dx * dx + dy * dy;
    if (d < bestDist) { bestDist = d; best = i; }
  }
  return best;
}

function clearSlotCue() {
  for (const el of grid.children) el.classList.remove("drop-slot");
}

grid.addEventListener("dragover", (e) => {
  if (dragFrom === null) return;
  e.preventDefault(); // required to allow a drop
  e.dataTransfer.dropEffect = "move";
  const slot = nearestSlot(e.clientX, e.clientY);
  clearSlotCue();
  if (slot >= 0 && slot !== dragFrom) grid.children[slot].classList.add("drop-slot");
});

grid.addEventListener("drop", (e) => {
  if (dragFrom === null) return;
  e.preventDefault();
  const slot = nearestSlot(e.clientX, e.clientY);
  clearSlotCue();
  if (slot < 0 || slot === dragFrom) return;
  flip(grid, () => {
    const [m] = order.splice(dragFrom, 1);
    order.splice(slot, 0, m);
    for (const idx of order) grid.appendChild(cards[idx].root); // reorder DOM
  });
  for (const m of order) fitCanvas(cards[m].spark); // slots may have moved rows
  scheduleDraw();
});

// FLIP: capture rects, mutate DOM order, then animate each element from its
// old position to its new one with a transform transition.
function flip(container, mutate) {
  const before = new Map([...container.children].map((el) => [el, el.getBoundingClientRect()]));
  mutate();
  for (const el of container.children) {
    const b = before.get(el);
    if (!b) continue;
    const a = el.getBoundingClientRect();
    const dx = b.left - a.left, dy = b.top - a.top;
    if (!dx && !dy) continue;
    el.style.transition = "none";
    el.style.transform = `translate(${dx}px, ${dy}px)`;
    requestAnimationFrame(() => {
      el.style.transition = "transform 220ms ease";
      el.style.transform = "";
      el.addEventListener("transitionend", () => { el.style.transition = ""; }, { once: true });
    });
  }
}

// ---------- drawing ----------

let drawQueued = false;
function scheduleDraw() {
  if (drawQueued) return;
  drawQueued = true;
  requestAnimationFrame(() => {
    drawQueued = false;
    drawAll();
  });
}

function drawAll() {
  for (let m = 0; m < METRICS.length; m++) {
    const data = buffers[m];
    if (data.length) {
      cards[m].valueEl.textContent = data[data.length - 1].toFixed(1) + METRICS[m].unit;
    }
    drawSpark(m);
  }
  drawChart();
}

function drawSpark(m) {
  const canvas = cards[m].spark;
  const ctx = canvas.getContext("2d");
  const w = canvas.clientWidth, h = canvas.clientHeight;
  ctx.clearRect(0, 0, w, h);
  const data = buffers[m].slice(-SPARK);
  if (data.length < 2) return;
  const [lo, hi] = extent(data);
  const step = w / (SPARK - 1);
  const x = (i) => w - (data.length - 1 - i) * step;
  const y = (v) => h - ((v - lo) / (hi - lo)) * h;
  ctx.beginPath();
  for (let i = 0; i < data.length; i++) ctx.lineTo(x(i), y(data[i]));
  ctx.strokeStyle = METRICS[m].color;
  ctx.lineWidth = 1.5;
  ctx.stroke();
  ctx.lineTo(x(data.length - 1), h);
  ctx.lineTo(x(0), h);
  ctx.closePath();
  ctx.fillStyle = METRICS[m].color + "2a"; // ~16% alpha fill
  ctx.fill();
}

function drawChart() {
  const ctx = chartCtx;
  const w = chartCanvas.clientWidth, h = chartCanvas.clientHeight;
  ctx.clearRect(0, 0, w, h);
  const data = buffers[selected];
  const color = METRICS[selected].color;
  const [lo, hi] = extent(data.length ? data : [0]);
  const step = w / (HISTORY - 1);
  const x = (i) => w - (data.length - 1 - i) * step; // newest sample pinned right
  const y = (v) => h - ((v - lo) / (hi - lo)) * h;

  // horizontal gridlines + value labels
  ctx.font = "10px -apple-system, sans-serif";
  ctx.fillStyle = "rgba(160,170,190,.55)";
  ctx.strokeStyle = "rgba(160,170,190,.14)";
  ctx.lineWidth = 1;
  for (let g = 0; g <= 4; g++) {
    const gy = (h / 4) * g + 0.5;
    ctx.beginPath();
    ctx.moveTo(0, gy);
    ctx.lineTo(w, gy);
    ctx.stroke();
    const v = hi - ((hi - lo) / 4) * g;
    ctx.fillText(v.toFixed(1), 4, Math.min(h - 3, gy + 11));
  }

  if (data.length >= 2) {
    ctx.beginPath();
    for (let i = 0; i < data.length; i++) ctx.lineTo(x(i), y(data[i]));
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.stroke();
    const grad = ctx.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, color + "38");
    grad.addColorStop(1, color + "00");
    ctx.lineTo(x(data.length - 1), h);
    ctx.lineTo(x(0), h);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
  }

  drawCrosshair(w, h, data, x, y, step);
}

function drawCrosshair(w, h, data, x, y, step) {
  if (!hover || data.length < 2) {
    tooltip.hidden = true;
    return;
  }
  const i = Math.max(0, Math.min(data.length - 1,
    data.length - 1 - Math.round((w - hover.x) / step)));
  const px = x(i), py = y(data[i]);
  const ctx = chartCtx;
  ctx.strokeStyle = "rgba(220,228,245,.45)";
  ctx.lineWidth = 1;
  ctx.setLineDash([4, 4]);
  ctx.beginPath();
  ctx.moveTo(px + 0.5, 0);
  ctx.lineTo(px + 0.5, h);
  ctx.moveTo(0, py + 0.5);
  ctx.lineTo(w, py + 0.5);
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.beginPath();
  ctx.arc(px, py, 4, 0, Math.PI * 2);
  ctx.fillStyle = METRICS[selected].color;
  ctx.fill();
  ctx.strokeStyle = "#fff";
  ctx.stroke();

  const seq = latestSeq - (data.length - 1 - i);
  tooltip.textContent =
    `${METRICS[selected].name}: ${data[i].toFixed(2)}${METRICS[selected].unit}  ·  sample #${seq}`;
  tooltip.hidden = false;
  const tw = tooltip.offsetWidth;
  const left = px + 14 + tw > w ? px - tw - 14 : px + 14;
  tooltip.style.left = `${Math.max(0, left)}px`;
  tooltip.style.top = `${Math.max(0, Math.min(h - 28, py - 34))}px`;
}

chartCanvas.addEventListener("mousemove", (e) => {
  const r = chartCanvas.getBoundingClientRect();
  hover = { x: e.clientX - r.left, y: e.clientY - r.top };
  scheduleDraw();
});
chartCanvas.addEventListener("mouseleave", () => {
  hover = null;
  scheduleDraw();
});

// ---------- live data over the IPC event bridge ----------

// Inter-arrival + latency stats, reported back to Rust every 5 s (printed to
// stdout — lets a headless launch verify the event path under load).
const stats = { last: 0, intervals: [], latencies: [] };

listen("tick", (event) => {
  const { seq, emitted_ms, values } = event.payload;
  const now = performance.timeOrigin + performance.now();
  if (stats.last) stats.intervals.push(now - stats.last);
  stats.last = now;
  stats.latencies.push(now - emitted_ms);
  for (let m = 0; m < METRICS.length; m++) {
    const b = buffers[m];
    b.push(values[m]);
    if (b.length > HISTORY) b.shift();
  }
  latestSeq = seq;
  scheduleDraw();
});

setInterval(() => {
  if (!stats.intervals.length) return;
  const mean = (a) => a.reduce((s, v) => s + v, 0) / a.length;
  const mi = mean(stats.intervals);
  invoke("report_stats", {
    count: stats.intervals.length,
    meanIntervalMs: mi,
    maxIntervalMs: Math.max(...stats.intervals),
    jitterMs: Math.sqrt(mean(stats.intervals.map((v) => (v - mi) ** 2))),
    meanLatencyMs: mean(stats.latencies),
    maxLatencyMs: Math.max(...stats.latencies),
  });
  stats.intervals = [];
  stats.latencies = [];
}, 5000);

// ---------- controls ----------

pauseBtn.addEventListener("click", async () => {
  paused = !paused;
  await invoke("set_paused", { paused });
  pauseBtn.textContent = paused ? "Resume" : "Pause";
  pauseBtn.classList.toggle("paused", paused);
});

rateSlider.addEventListener("input", async () => {
  const hz = Number(rateSlider.value);
  rateLabel.textContent = `${hz} Hz`;
  await invoke("set_rate", { hz });
});

// ---------- init ----------

function fitAll() {
  fitCanvas(chartCanvas);
  for (const c of cards) fitCanvas(c.spark);
  scheduleDraw();
}

buildCards();
fitAll();
window.addEventListener("resize", fitAll);

invoke("get_config").then(({ hz, paused: p }) => {
  rateSlider.value = String(Math.round(hz));
  rateLabel.textContent = `${Math.round(hz)} Hz`;
  paused = p;
  pauseBtn.textContent = p ? "Resume" : "Pause";
});
