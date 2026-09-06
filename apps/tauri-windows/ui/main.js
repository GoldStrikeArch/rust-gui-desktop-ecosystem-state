const rowsEl = document.getElementById("rows");
const statusEl = document.getElementById("status");

boot((m) => {
  rowsEl.innerHTML = "";
  for (const p of m.projects) {
    const el = document.createElement("div");
    el.className = "row" + (p.id === m.selected ? " sel" : "");
    el.dataset.id = p.id;
    el.innerHTML =
      `<span class="c-name"></span><span class="c-owner"></span>` +
      `<span class="c-budget"></span><span class="c-status"></span>`;
    el.querySelector(".c-name").textContent = p.name;
    el.querySelector(".c-owner").textContent = p.owner;
    el.querySelector(".c-budget").textContent = p.budget.toFixed(2);
    el.querySelector(".c-status").textContent = p.status;
    el.addEventListener("click", () => invoke("select", { id: p.id }));
    el.addEventListener("dblclick", () => invoke("open_window", { which: "edit" }));
    rowsEl.appendChild(el);
  }
  statusEl.innerHTML = "";
  statusEl.append("selected: ");
  const b1 = document.createElement("b");
  b1.textContent = m.selectedName;
  statusEl.append(b1, " · pings: ");
  const b2 = document.createElement("b");
  b2.textContent = String(m.pings);
  statusEl.append(b2, m.dirty ? " · unsaved changes" : "");

  // Input block while the "modal" child window is up.
  let veil = document.getElementById("veil");
  if (m.modal && !veil) {
    veil = document.createElement("div");
    veil.id = "veil";
    veil.addEventListener("click", () => say("click swallowed by modal veil"));
    document.body.appendChild(veil);
  } else if (!m.modal && veil) {
    veil.remove();
  }
  document.getElementById("btn-inspector").textContent =
    m.inspectorOpen ? "Inspector ✓" : "Inspector";
});

document.getElementById("btn-edit").onclick = () => invoke("open_window", { which: "edit" });
document.getElementById("btn-inspector").onclick = () => invoke("open_window", { which: "inspector" });
document.getElementById("btn-prefs").onclick = () => invoke("open_window", { which: "prefs" });
document.getElementById("btn-delete").onclick = () => {
  say("Delete pressed");
  invoke("delete_selected");
};
document.getElementById("btn-pong").onclick = () => invoke("pong");
