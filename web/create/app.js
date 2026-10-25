import { proof } from "/proof.js";
import { el, copy, initializeUI, notice, openDialog, request } from "/ui.js";

const messages = {
  invalid_subdomain: "3–40 lowercase letters, digits or hyphens",
  subdomain_reserved: "This address isn't available. Choose another",
  subdomain_taken: "This address is taken",
  invalid_source: "Paste the Google Sheet's link",
  source_in_use: "This sheet already has a store",
  source_unreachable: "Share the sheet with the address above first",
  source_read_only: "Share the sheet as Editor, not Viewer",
  source_invalid: "Two tabs look alike. Keep one of each",
  source_unavailable: "Google Sheets is unavailable. Try again",
};

// Long addresses wrap after a dot or an @, not inside a name like "my-store".
function breakable(node, text) {
  node.replaceChildren(
    ...text
      .split(/(?<=[.@])/)
      .flatMap((part) => [part, document.createElement("wbr")]),
  );
}

initializeUI();
// The footer's help and sample tables open as dialogs: a sheet from the bottom on
// phones, a window on wider screens. A click outside closes them.
document
  .querySelectorAll("[data-open]")
  .forEach((button) => button.addEventListener("click", () => openDialog(button.dataset.open)));
document
  .querySelectorAll("[data-close]")
  .forEach((button) => button.addEventListener("click", () => el(button.dataset.close).close()));
document.querySelectorAll("dialog").forEach((dialog) =>
  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) dialog.close();
  }),
);
const info = await request("/v1/info");
breakable(el("account"), info.share_with || "Unavailable");
el("suffix").textContent = `.${info.store_suffix}`;
el("copy-account").addEventListener("click", (event) =>
  copy(info.share_with, event.currentTarget),
);
// The formula bar shows the active row's hint, as a sheet shows the active cell.
const hint = el("hint");
const idle = hint.textContent;
el("create").addEventListener("focusin", (event) => {
  const row = event.target.closest("[data-hint]");
  if (row) hint.textContent = row.dataset.hint;
});
el("create").addEventListener("input", (event) =>
  event.target.removeAttribute("aria-invalid"),
);
// An address is lowercase letters, digits and hyphens: typed capitals and spaces
// become what they would have to be.
el("subdomain").addEventListener("input", (event) => {
  const value = event.target.value
    .toLowerCase()
    .replace(/[\s_.]+/g, "-")
    .replace(/[^a-z0-9-]/g, "");
  if (value !== event.target.value) event.target.value = value;
});
// Enter on the link goes to the next row, as in a sheet. Enter on the address creates.
el("source").addEventListener("keydown", (event) => {
  if (event.key !== "Enter" || event.isComposing) return;
  event.preventDefault();
  el("subdomain").focus();
});
el("create").addEventListener("focusout", (event) => {
  if (!el("create").contains(event.relatedTarget)) hint.textContent = idle;
});
el("create").addEventListener("submit", async (event) => {
  event.preventDefault();
  const button = el("submit");
  button.disabled = true;
  button.setAttribute("aria-busy", "true");
  try {
    const created = await request("/v1/stores", {
      method: "POST",
      body: {
        subdomain: el("subdomain").value.trim(),
        source: el("source").value.trim(),
        altcha: await proof("/v1/altcha?purpose=store"),
      },
    });
    showCreated(created, el("source").value.trim());
  } catch (error) {
    const message = messages[error.code] || error.message;
    notice(message, true);
    // The row at fault turns red and takes focus. The formula bar says why.
    const field = /subdomain/.test(error.code ?? "")
      ? el("subdomain")
      : /source_(?!unavailable)|_source/.test(error.code ?? "")
        ? el("source")
        : null;
    if (field) {
      field.setAttribute("aria-invalid", "true");
      field.focus();
      hint.textContent = message;
    }
  } finally {
    button.disabled = false;
    button.removeAttribute("aria-busy");
  }
});

// The result: the store's address to open or copy, its address on this machine when
// the server has one, the sheet, and the steps left there (the Payments tab, then
// Google's one-time Allow access for the Tokens tab's rate formulas).
function showCreated(created, source) {
  const host = (url) => url.replace(/^https?:\/\//, "");
  el("store").href = el("open-store").href = created.url;
  breakable(el("store"), host(created.url));
  const local = created.aliases?.[0];
  el("store-local").hidden = !local;
  if (local) {
    el("store-local").href = local;
    breakable(el("store-local"), `On this computer: ${host(local)}`);
  }
  // A bare sheet ID opens as a link too.
  el("open-sheet").href = source.startsWith("https://")
    ? source
    : `https://docs.google.com/spreadsheets/d/${encodeURIComponent(source)}/edit`;
  el("create").hidden = true;
  el("created").hidden = false;
  el("created-status").textContent = `Store created: ${host(created.url)}`;
  el("created").focus();
  el("copy-store").onclick = (event) => copy(created.url, event.currentTarget);
}
el("again").addEventListener("click", () => {
  el("create").reset();
  el("created").hidden = true;
  el("create").hidden = false;
  el("source").focus();
});
