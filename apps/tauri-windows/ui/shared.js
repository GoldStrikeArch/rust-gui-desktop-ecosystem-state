// Shared window bootstrap. Every window is a renderer over ONE Rust-owned
// model: pull a snapshot on load, then re-render on every `model` broadcast.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const LABEL = document.body.dataset.label;
let M = null;
const renderers = [];

function applyChrome(m) {
  document.documentElement.dataset.theme = m.theme;
  document.body.classList.toggle("compact", m.compact);
}

async function boot(render) {
  renderers.push(render);
  await listen("model", (e) => {
    M = e.payload;
    applyChrome(M);
    renderers.forEach((f) => f(M));
  });
  M = await invoke("get_model");
  applyChrome(M);
  renderers.forEach((f) => f(M));
}

const say = (line) => invoke("report", { line: `[${LABEL}] ${line}` });

// SPEC-9 §11: Esc closes the topmost non-main window. There is no framework
// "topmost" notion — the focused window is the one receiving the keystroke,
// which is the same thing.
if (LABEL !== "main") {
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") invoke("close_self", { label: LABEL });
  });
}
window.onerror = (m, s, l) => say(`JSERROR ${m} @${l}`);
