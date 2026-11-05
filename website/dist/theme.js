// Apply the saved choice before styles load. A new visit starts in Ledger Ink.
(() => {
  const root = document.documentElement;
  let theme = "dark";
  try {
    if (localStorage.getItem("tableiq-theme") === "light") theme = "light";
  } catch {
    // Storage may be blocked; switching still works for the current page.
  }
  root.dataset.theme = theme;

  document.addEventListener("DOMContentLoaded", () => {
    const button = document.querySelector("[data-theme-toggle]");
    if (!button) return;

    function render() {
      const dark = root.dataset.theme === "dark";
      button.setAttribute("aria-pressed", String(dark));
      button.title = `Switch to ${dark ? "light" : "dark"} mode`;
      document.querySelectorAll("source[data-theme-source='dark']").forEach((source) => {
        source.media = dark ? "all" : "not all";
      });
    }

    button.addEventListener("click", () => {
      root.dataset.theme = root.dataset.theme === "dark" ? "light" : "dark";
      try {
        localStorage.setItem("tableiq-theme", root.dataset.theme);
      } catch {
        // Do not prevent a theme change when the browser cannot save preferences.
      }
      render();
    });
    render();
    button.hidden = false;
  });
})();
