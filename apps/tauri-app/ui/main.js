// Frontend is a thin view: all task state lives in the Rust core process.
// Every mutation goes through Tauri's IPC bridge (`invoke`) and the UI
// re-renders from the canonical list returned by Rust.
// `window.__TAURI__` is available because `app.withGlobalTauri` is true in
// tauri.conf.json (no npm/@tauri-apps/api package involved).
const { invoke } = window.__TAURI__.core;

const form = document.getElementById("add-form");
const input = document.getElementById("task-input");
const counter = document.getElementById("counter");
const list = document.getElementById("task-list");

function render(tasks) {
  counter.textContent = `${tasks.length} task(s)`;
  list.replaceChildren(
    ...tasks.map((text, index) => {
      const li = document.createElement("li");

      const span = document.createElement("span");
      span.className = "task-text";
      span.textContent = text; // textContent: no HTML injection from task text

      const del = document.createElement("button");
      del.type = "button";
      del.className = "delete-btn";
      del.textContent = "Delete";
      del.setAttribute("aria-label", `Delete task: ${text}`);
      del.addEventListener("click", async () => {
        render(await invoke("delete_task", { index }));
      });

      li.append(span, del);
      return li;
    })
  );
}

// Submitting the form covers both the "Add" button click and pressing
// Enter while the input is focused.
form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const text = input.value;
  if (text.trim() === "") return; // Rust also guards; skip a useless round-trip
  render(await invoke("add_task", { text }));
  input.value = "";
  input.focus();
});

// Initial render from the canonical Rust-side state.
invoke("get_tasks").then(render);
