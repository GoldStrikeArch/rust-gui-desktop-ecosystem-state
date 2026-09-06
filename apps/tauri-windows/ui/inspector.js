const F = ["name", "owner", "budget", "status"];
let id = 0, typing = false;
boot((m) => {
  id = m.selected;
  document.getElementById("note").textContent = `project #${id} · pings ${m.pings}`;
  const p = m.projects.find((x) => x.id === id);
  if (!p) return;
  for (const f of F) {
    const el = document.getElementById("f-" + f);
    // Don't fight the user's caret: skip the field being typed in.
    if (document.activeElement === el && typing) continue;
    const v = f === "budget" ? p.budget.toFixed(2) : p[f];
    if (el.value !== v) el.value = v;
  }
});
for (const f of F) {
  const el = document.getElementById("f-" + f);
  const push = () => invoke("set_field", { id, field: f, value: el.value });
  el.addEventListener("input", () => { typing = true; push(); });
  el.addEventListener("change", push);
  el.addEventListener("blur", () => { typing = false; });
}
document.getElementById("btn-ping").onclick = () => invoke("ping");
listen("flash", async (e) => {
  document.body.classList.add("flash");
  say("flash " + e.payload + "ms");
  setTimeout(() => document.body.classList.remove("flash"), e.payload);
});
