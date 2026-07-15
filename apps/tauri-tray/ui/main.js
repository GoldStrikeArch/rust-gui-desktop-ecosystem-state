// Tray Notes frontend. Text state lives HERE (the textarea is the document);
// Rust is the OS-integration layer. Dialogs are called from JS on purpose —
// that is the one plugin whose ACL permissions this app pays for
// (dialog:default in capabilities/default.json). Everything else (tray,
// menus, hotkey, notification, clipboard image, file drop) is Rust-side and
// arrives via events / commands.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const dialog = window.__TAURI__.dialog;

const editor = document.getElementById("editor");
const status = document.getElementById("status");
const themeLabel = document.getElementById("theme-label");
const thumbRow = document.getElementById("thumb-row");
const thumb = document.getElementById("thumb");
const thumbCaption = document.getElementById("thumb-caption");

const setStatus = (msg) => { status.textContent = msg; };
const TXT_FILTER = { filters: [{ name: "Text", extensions: ["txt"] }] };

// --- File menu actions (menu events come from Rust; dialogs run from JS) ---

async function openNote() {
  const path = await dialog.open({ multiple: false, directory: false, ...TXT_FILTER });
  if (!path) return;
  editor.value = await invoke("read_note", { path });
  setStatus(`Opened ${path}`);
}

async function saveNote() {
  const path = await dialog.save(TXT_FILTER);
  if (!path) return;
  await invoke("write_note", { path, text: editor.value });
  setStatus(`Saved ${path}`);
}

listen("menu-new", () => { editor.value = ""; setStatus("New note"); editor.focus(); });
listen("menu-open", () => openNote().catch((e) => setStatus(`Open failed: ${e}`)));
listen("menu-save", () => saveNote().catch((e) => setStatus(`Save failed: ${e}`)));

// File drop is handled natively in Rust (DragDropEvent::Drop) which reads the
// file and pushes the content down as an event.
listen("note-loaded", (event) => {
  editor.value = event.payload.text;
  setStatus(`Loaded ${event.payload.path}`);
});

// --- Clipboard image -> thumbnail -------------------------------------------
// paste_image returns raw bytes: [w u32 LE][h u32 LE][RGBA...] as an
// ArrayBuffer (tauri::ipc::Response), decoded straight into a canvas.
document.getElementById("paste-image").addEventListener("click", async () => {
  try {
    const buf = await invoke("paste_image");
    const dv = new DataView(buf);
    const w = dv.getUint32(0, true);
    const h = dv.getUint32(4, true);
    thumb.width = w;
    thumb.height = h;
    thumb.getContext("2d").putImageData(
      new ImageData(new Uint8ClampedArray(buf, 8), w, h), 0, 0);
    thumbRow.hidden = false;
    thumbCaption.textContent = `clipboard image ${w}×${h}`;
    setStatus("Pasted image from clipboard");
    invoke("report", { msg: `webview rendered clipboard image ${w}x${h}` });
  } catch (e) {
    setStatus(`No image on clipboard (${e})`);
    invoke("report", { msg: `paste_image failed: ${e}` });
  }
});

// --- Live dark mode ----------------------------------------------------------
// CSS does the restyle by itself; this listener only *proves* (on stdout)
// that the media query flipped live inside the webview.
const mq = window.matchMedia("(prefers-color-scheme: dark)");
const showTheme = () => { themeLabel.textContent = `theme: ${mq.matches ? "dark" : "light"}`; };
mq.addEventListener("change", () => {
  showTheme();
  invoke("report", { msg: `webview prefers-color-scheme flipped to ${mq.matches ? "dark" : "light"}` });
});
showTheme();

// Rust-side ThemeChanged event (window server view of the same flip).
listen("theme-changed", (event) => setStatus(`OS theme now ${event.payload}`));

invoke("report", { msg: `webview booted; prefers-color-scheme=${mq.matches ? "dark" : "light"}` });
