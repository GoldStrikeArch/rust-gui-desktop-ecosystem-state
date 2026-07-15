// Babel frontend. The corpus comes from Rust (include_str!) — never retyped.
// Rendering is one <div class="line"> per paragraph; everything else (BiDi,
// shaping, fallback, emoji, editing) is WKWebView.
const { invoke } = window.__TAURI__.core;

const render = document.getElementById("render");
const editor = document.getElementById("editor");
const status = document.getElementById("status");
const setStatus = (m) => { status.textContent = m; };

let corpus = "";

// First strong-direction character AFTER the "[TAG] " prefix decides the
// line's base direction (the ASCII tag would otherwise force every line LTR
// under a plain first-strong heuristic — dir="auto" gets [AR]/[HE] wrong
// here). Hebrew/Arabic blocks incl. presentation forms count as RTL.
const RTL_CHAR = /[\u0590-\u08FF\uFB1D-\uFDFF\uFE70-\uFEFC]/;
const LTR_CHAR = /[A-Za-z\u00C0-\u058F\u0900-\u1FFF\u2C00-\uD7FF\uF900-\uFAFF]/;
function lineDir(line) {
  const body = line.replace(/^\[\w+\]\s*/, "");
  for (const ch of body) {
    if (RTL_CHAR.test(ch)) return "rtl";
    if (LTR_CHAR.test(ch)) return "ltr";
  }
  return "auto";
}

function renderLines(text) {
  const t0 = performance.now();
  const lines = text.split("\n").filter((l) => l.length > 0);
  const frag = document.createDocumentFragment();
  for (const line of lines) {
    const div = document.createElement("div");
    div.className = "line";
    div.dir = lineDir(line);
    div.textContent = line;
    frag.append(div);
  }
  render.replaceChildren(frag);
  render.scrollTop = 0;
  const built = performance.now() - t0;
  requestAnimationFrame(() => requestAnimationFrame(() => {
    setStatus(`${lines.length} lines · build ${built.toFixed(0)} ms · paint ${(performance.now() - t0).toFixed(0)} ms`);
  }));
  return lines.length;
}

document.getElementById("big-doc").addEventListener("click", () => {
  const n = renderLines(corpus.repeat(1000)); // ≈11k lines, generated in code
  invoke("report", { msg: `big doc rendered: ${n} lines, ${render.childElementCount} nodes` });
});
document.getElementById("reset-doc").addEventListener("click", () => renderLines(corpus));

// ---------------------------------------------------------------- selftest

// Scroll the big doc for ~150 frames and report frame-time stats.
function scrollProbe() {
  return new Promise((resolve) => {
    const deltas = [];
    let last = performance.now();
    let frames = 0;
    render.scrollTop = 0;
    const step = () => {
      const now = performance.now();
      deltas.push(now - last);
      last = now;
      render.scrollTop += 60;
      if (++frames < 150 && render.scrollTop < render.scrollHeight - render.clientHeight) {
        requestAnimationFrame(step);
      } else {
        deltas.shift(); // first delta measures setup, not a frame
        const mean = deltas.reduce((a, b) => a + b, 0) / deltas.length;
        const max = Math.max(...deltas);
        const jank = deltas.filter((d) => d > 33).length;
        resolve({ frames: deltas.length, mean, max, jank, scrollHeight: render.scrollHeight });
      }
    };
    requestAnimationFrame(step);
  });
}

window.__BABEL_SELFTEST__ = async () => {
  const rep = (msg) => invoke("report", { msg });
  const lines = [...render.children];
  rep(`corpus rendered: ${lines.length} lines; rtl lines: ${lines.filter((l) => l.dir === "rtl").length} (${lines.filter((l) => l.dir === "rtl").map((l) => l.textContent.slice(0, 4)).join(" ")})`);

  // Grapheme probe: does WebKit's backward-delete treat the ZWJ family as one
  // unit? (execCommand('delete') drives the same editing command as backspace)
  const family = "\u{1F468}‍\u{1F469}‍\u{1F467}‍\u{1F466}"; // 👨‍👩‍👧‍👦 = 11 UTF-16 units
  const saved = editor.value;
  const idx = editor.value.indexOf(family);
  if (idx >= 0) {
    editor.focus();
    editor.setSelectionRange(idx + family.length, idx + family.length);
    const before = editor.value.length;
    document.execCommand("delete");
    const deleted = before - editor.value.length;
    rep(`grapheme probe: backward-delete after family emoji removed ${deleted}/11 UTF-16 units -> ${deleted === 11 ? "WHOLE cluster" : "PARTIAL (corruption)"}`);
    editor.value = saved;
    // setSelectionRange mid-cluster: browsers do not snap programmatic offsets
    editor.setSelectionRange(idx + 5, idx + 5);
    rep(`grapheme probe: setSelectionRange(idx+5) landed at ${editor.selectionStart - idx} (programmatic offsets are not snapped; arrow keys are)`);
    editor.setSelectionRange(editor.value.length, editor.value.length);
  } else {
    rep("grapheme probe: family emoji not found in editor");
  }

  // Big doc + scroll probe.
  document.getElementById("big-doc").click();
  await new Promise((r) => setTimeout(r, 600));
  const s = await scrollProbe();
  rep(`scroll probe: ${s.frames} frames, mean ${s.mean.toFixed(2)} ms, max ${s.max.toFixed(1)} ms, frames>33ms: ${s.jank}, scrollHeight ${s.scrollHeight}px`);
  // Leave the app screenshot-ready (corpus view) and keystroke-ready (caret
  // at the end of the editor) for the scripted arrow-key phase.
  document.getElementById("reset-doc").click();
  editor.focus();
  editor.setSelectionRange(editor.value.length, editor.value.length);
  rep("selftest done");
};

// Caret reporter (selftest only): lets scripted arrow-key presses be observed
// on stdout. Polling because WebKit's selectionchange support on <textarea>
// is inconsistent.
async function maybeStartCaretReporter() {
  const flags = await invoke("get_flags");
  if (!flags.selftest) return;
  let prev = "";
  setInterval(() => {
    const cur = `${editor.selectionStart},${editor.selectionEnd}`;
    if (cur !== prev && document.activeElement === editor) {
      prev = cur;
      invoke("report", { msg: `caret ${cur}` });
    }
  }, 250);
}

// ------------------------------------------------------------------- boot

invoke("get_corpus").then((text) => {
  corpus = text;
  renderLines(corpus);
  const mixed = corpus.split("\n").find((l) => l.startsWith("[MIXED]")) || "";
  editor.value = mixed;
  editor.setSelectionRange(mixed.length, mixed.length);
  invoke("report", { msg: `booted; corpus ${corpus.length} chars; editor seeded ${mixed.length} UTF-16 units` });
  maybeStartCaretReporter();
});
