// Fetcher (Tauri) — SPEC-8 frontend. All async logic is browser-idiom JS:
// setTimeout debounce, AbortController cancellation, ReadableStream progress.
// The transport is tauri-plugin-http's fetch-compatible API
// (window.__TAURI__.http.fetch): same call shape as browser fetch, but the
// request is made by reqwest in the Rust process (IPC + channel-streamed
// body), because browser-native fetch is CORS-blocked from the
// tauri://localhost origin against the header-less local server.
const { invoke } = window.__TAURI__.core;
const http = window.__TAURI__.http;

const searchInput = document.getElementById("search-input");
const searchStatus = document.getElementById("search-status");
const resultsEl = document.getElementById("results");
const dlBtn = document.getElementById("dl-btn");
const dlCancel = document.getElementById("dl-cancel");
const dlStatus = document.getElementById("dl-status");
const dlFill = document.getElementById("dl-fill");
const flakyBtn = document.getElementById("flaky-btn");
const flakyRetry = document.getElementById("flaky-retry");
const flakyStatus = document.getElementById("flaky-status");

let base = "http://127.0.0.1:7878";

// Instrumentation the self-test asserts on (harmless in normal runs).
const state = {
  searchSeq: 0,       // sequence guard: newest search wins
  searchAborts: 0,    // stale in-flight requests actually aborted
  resultCount: 0,
  download: "idle",   // idle | running | done | cancelled | error
  dlReceived: 0,
  dlTotal: 0,
  progressEvents: 0,
  flaky: null,        // { ok, attempt?, status? }
};

const MIB = 1024 * 1024;
const mib = (b) => (b / MIB).toFixed(1);

function setStatus(el, text, cls = "") {
  el.textContent = text;
  el.className = `status ${cls}`.trim();
}

// ---------- search-as-you-type: 250 ms debounce + stale protection ----------
// Two layers: the debounce timer swallows keystrokes closer than 250 ms, and
// when a new request fires, the previous in-flight one is BOTH aborted (real
// cancellation — the Rust side drops the reqwest request) and sequence-
// guarded (an already-delivered older response can never overwrite a newer
// one, even if abort loses the race).

let debounceTimer = null;
let searchCtl = null;

searchInput.addEventListener("input", () => {
  clearTimeout(debounceTimer);
  const q = searchInput.value.trim();
  if (q === "") {
    state.searchSeq++; // invalidate anything in flight
    if (searchCtl) searchCtl.abort();
    resultsEl.replaceChildren();
    state.resultCount = 0;
    setStatus(searchStatus, "");
    return;
  }
  debounceTimer = setTimeout(() => runSearch(q), 250);
});

async function runSearch(q) {
  const seq = ++state.searchSeq;
  if (searchCtl) searchCtl.abort(); // cancel the stale in-flight request
  const ctl = (searchCtl = new AbortController());
  setStatus(searchStatus, "searching…");
  try {
    const res = await http.fetch(`${base}/search?q=${encodeURIComponent(q)}`, {
      signal: ctl.signal,
    });
    const items = await res.json();
    // Fully settled: stop tracking the controller. Aborting a plugin-http
    // request AFTER it completed raises a stray "resource id invalid"
    // rejection from the plugin's fire-and-forget cancel (and once froze the
    // webview mid-run — see FRICTION.md), so only in-flight requests may be
    // aborted.
    if (searchCtl === ctl) searchCtl = null;
    if (seq !== state.searchSeq) return; // stale response: discard
    state.resultCount = items.length;
    resultsEl.replaceChildren(
      ...items.map((it) => {
        const li = document.createElement("li");
        const name = document.createElement("span");
        name.textContent = it.name;
        const score = document.createElement("span");
        score.className = "score";
        score.textContent = it.score.toFixed(1);
        li.append(name, score);
        return li;
      })
    );
    setStatus(searchStatus, `${items.length} result(s) for "${q}"`);
  } catch (e) {
    if (searchCtl === ctl) searchCtl = null;
    if (ctl.signal.aborted) {
      state.searchAborts++; // superseded by a newer query — expected
      return;
    }
    if (seq === state.searchSeq) setStatus(searchStatus, `search failed: ${e}`, "error");
  }
}

// ---------- download: streamed progress + real cancellation ----------

let dlCtl = null;

dlBtn.addEventListener("click", async () => {
  dlBtn.disabled = true;
  dlCancel.disabled = false;
  state.download = "running";
  state.dlReceived = 0;
  state.progressEvents = 0;
  dlFill.style.width = "0%";
  const ctl = (dlCtl = new AbortController());
  try {
    const res = await http.fetch(`${base}/download`, { signal: ctl.signal });
    state.dlTotal = Number(res.headers.get("content-length")) || 0;
    // Plugin fetch exposes the body as a real ReadableStream (chunks arrive
    // over a Tauri channel from reqwest) — same reader loop as browser fetch.
    const reader = res.body.getReader();
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      state.dlReceived += value.byteLength;
      state.progressEvents++;
      const pct = state.dlTotal ? (state.dlReceived / state.dlTotal) * 100 : 0;
      dlFill.style.width = `${pct.toFixed(1)}%`;
      setStatus(dlStatus, `${mib(state.dlReceived)} / ${mib(state.dlTotal)} MiB`);
    }
    state.download = "done";
    setStatus(dlStatus, `done — ${mib(state.dlReceived)} MiB`, "ok");
  } catch (e) {
    if (ctl.signal.aborted) {
      state.download = "cancelled";
      setStatus(dlStatus, `cancelled at ${mib(state.dlReceived)} / ${mib(state.dlTotal)} MiB`);
    } else {
      state.download = "error";
      setStatus(dlStatus, `download failed: ${e}`, "error");
    }
  } finally {
    dlCtl = null; // settled — must not be aborted after the fact (see above)
    dlBtn.disabled = false;
    dlCancel.disabled = true;
  }
});

// Aborting the controller cancels the Rust-side reqwest request/stream; the
// server's `ABORT /download` log line is the proof (see FRICTION.md).
dlCancel.addEventListener("click", () => {
  if (dlCtl) dlCtl.abort();
});

// ---------- flaky endpoint: visible error + manual retry ----------

async function callFlaky() {
  flakyBtn.disabled = true;
  flakyRetry.hidden = true;
  setStatus(flakyStatus, "calling…");
  try {
    const res = await http.fetch(`${base}/flaky`);
    if (!res.ok) {
      state.flaky = { ok: false, status: res.status };
      setStatus(flakyStatus, `HTTP ${res.status} — server failed, try again`, "error");
      flakyRetry.hidden = false;
    } else {
      const j = await res.json();
      state.flaky = { ok: true, attempt: j.attempt };
      setStatus(flakyStatus, `success on attempt ${j.attempt}`, "ok");
    }
  } catch (e) {
    state.flaky = { ok: false, error: String(e) };
    setStatus(flakyStatus, `request failed: ${e}`, "error");
    flakyRetry.hidden = false;
  } finally {
    flakyBtn.disabled = false;
  }
}
flakyBtn.addEventListener("click", callFlaky);
flakyRetry.addEventListener("click", callFlaky);

// ---------- error + init ----------

function reportErr(line) {
  invoke("report", { line: `JSERR ${line}` }).catch(() => {});
}
window.onerror = (msg, src, ln) => reportErr(`${msg} @ ${src}:${ln}`);
window.addEventListener("unhandledrejection", (e) => reportErr(`unhandled rejection: ${e.reason}`));

const ready = (async () => {
  const cfg = await invoke("get_config");
  base = `http://127.0.0.1:${cfg.port}`;
  return cfg;
})();

// Hooks for selftest.js (no-op in normal runs).
window.__fetchapp = {
  state, ready, invoke, http,
  base: () => base,
  els: { searchInput, resultsEl, dlBtn, dlCancel, flakyBtn, flakyRetry, flakyStatus },
};
