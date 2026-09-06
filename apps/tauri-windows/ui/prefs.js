const compact = document.getElementById("compact");
boot((m) => {
  compact.checked = m.compact;
  const r = document.querySelector(`input[name=theme][value="${m.theme}"]`);
  if (r) r.checked = true;
});
const push = () =>
  invoke("set_prefs", {
    compact: compact.checked,
    theme: document.querySelector("input[name=theme]:checked").value,
  });
compact.onchange = push;
for (const r of document.querySelectorAll("input[name=theme]")) r.onchange = push;
