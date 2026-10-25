// Applies the theme before first paint, so no page flashes the wrong one. A store whose
// merchant chose light or dark (/store-theme.js sets data-lock ahead of this file)
// always gets that. Otherwise the visitor's saved choice, else the system's. ui.js owns
// the toggle and later changes.
{
  const root = document.documentElement;
  const modes = ["light", "dark"];
  let saved = null;
  try {
    saved = localStorage.getItem("stor.appearance");
  } catch {
    /* No storage: the system's appearance applies. */
  }
  const system = () => {
    try {
      return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    } catch {
      return "light";
    }
  };
  root.dataset.theme = modes.includes(root.dataset.lock)
    ? root.dataset.lock
    : modes.includes(saved)
      ? saved
      : system();
}
