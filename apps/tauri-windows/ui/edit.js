const name = document.getElementById("e-name");
const budget = document.getElementById("e-budget");
// Snapshot once: a modal edits a copy and commits on OK.
invoke("get_model").then((m) => {
  const p = m.projects.find((x) => x.id === m.selected);
  if (!p) return;
  name.value = p.name;
  budget.value = p.budget.toFixed(2);
  name.focus();
  name.select();
});
const done = (commit) =>
  invoke("close_edit", { commit, name: name.value, budget: budget.value });
document.getElementById("e-ok").onclick = () => done(true);
document.getElementById("e-cancel").onclick = () => done(false);
window.addEventListener("keydown", (e) => {
  if (e.key === "Enter") done(true);
  else if (e.key === "Escape") done(false);
});
