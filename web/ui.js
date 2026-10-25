// Shared, dependency-free presentation primitives. All amounts are strings.
// Merchant content is rendered with textContent, never HTML interpolation.
export const el = (id) => document.getElementById(id);
export function node(tag, text, className) {
  const value = document.createElement(tag);
  if (text != null) value.textContent = String(text);
  if (className) value.className = className;
  return value;
}
export function action(title, handler, className = "button secondary") {
  const value = node("button", title, className);
  value.type = "button";
  if (!className.includes("text-button") && className) {
    const symbol = actionIcon(title);
    if (symbol) value.prepend(icon(symbol));
  }
  value.addEventListener("click", handler);
  return value;
}
const symbols = {
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  close: "m6 6 12 12M18 6 6 18",
  arrow: "M5 12h14m-6-6 6 6-6 6",
  back: "M19 12H5m6-6-6 6 6 6",
  bag: "M5 7h14l1 14H4L5 7Zm4 0V5a3 3 0 0 1 6 0v2",
  clock: "M12 8v4l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
  box: "m12 3 9 5v9l-9 5-9-5V8l9-5Zm0 9v10M3 8l9 4 9-4M7.5 5.5l9 5",
  link: "m10 14 4-4m-5 7-2 2a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0m2-2 2-2a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-6 0",
  refresh:
    "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5",
  copy: "M9 9h12v12H9ZM5 15H3V3h12v2",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Zm13 0a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
  "eye-off":
    "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Zm13 0a3 3 0 1 1-6 0 3 3 0 0 1 6 0M3 3l18 18",
  trash: "M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7",
  check: "m5 12 4 4L19 6",
  info: "M12 11v6M12 7h.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
  external: "M14 3h7v7m0-7L10 14M10 3H3v18h18v-7",
  key: "M15.75 5.25a3 3 0 0 1 3 3m3 0a6 6 0 0 1-7.029 5.912c-.563-.097-1.159.026-1.563.43L10.5 17.25H8.25v2.25H6v2.25H2.25v-2.818c0-.597.237-1.17.659-1.591l6.499-6.499c.404-.404.527-1 .43-1.563A6 6 0 1 1 21.75 8.25Z",
  sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1 1M18 18l1 1M5 19l1-1M18 6l1-1",
  moon: "M20 14A9 9 0 0 1 10 4a9 9 0 1 0 10 10",
  store: "M3 9 4.5 4h15L21 9M3 9v11h18V9M3 9h18M9 20v-6h6v6",
  upload: "M12 16V4m-5 5 5-5 5 5M4 16v4h16v-4",
  download: "M12 4v12m-5-5 5 5 5-5M4 16v4h16v-4",
  palette:
    "M12 3a9 9 0 0 0 0 18c1 0 1.5-.8 1.5-1.5 0-1.1-1-1.5-1-2.5s.9-2 2-2H17a4 4 0 0 0 4-4c0-4.4-4-8-9-8ZM7.5 11h.01M10 7h.01M15 7h.01",
  lock: "M6 11h12v10H6ZM8 11V7a4 4 0 0 1 8 0v4",
  chevron: "m9 6 6 6-6 6",
  alert:
    "M12 9v4m0 4h.01M10.3 3.9 2.4 18a2 2 0 0 0 1.7 3h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z",
  sheet: "M3 4h18v16H3ZM3 10h18M3 15h18M9 4v16",
  wallet: "M3 7h16a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm0 0V6a2 2 0 0 1 2-2h11M16 13h.01",
  search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14Zm5-2 5 5",
  tray: "M3 13h5l1 3h6l1-3h5M5 5h14l2 8v6H3v-6Z",
  undo: "M9 14 4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3",
  edit: "M4 20h4L19 9l-4-4L4 16v4Zm9-13 4 4",
  grid: "M4 4h6v6H4ZM14 4h6v6h-6ZM4 14h6v6H4ZM14 14h6v6h-6Z",
  list: "M9 6h11M9 12h11M9 18h11M4 6h1M4 12h1M4 18h1",
};
export function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("ui-icon");
  const path = document.createElementNS(svg.namespaceURI, "path");
  path.setAttribute("d", symbols[name] || symbols.info);
  svg.append(path);
  return svg;
}
// Marks a row of steps done, current or problem. The rest stay to do.
export function setStepper(list, states) {
  [...list.children].forEach((item, index) => {
    const state = states[index] || "";
    const marker = item.querySelector(".stepper-marker");
    item.dataset.state = state;
    marker.replaceChildren(
      state === "done"
        ? icon("check")
        : state === "problem"
          ? icon("alert")
          : node("b", index + 1),
    );
    if (state === "done" || state === "problem")
      marker.append(node("span", state === "done" ? "Done" : "Problem", "sr-only"));
    if (state === "current" || state === "problem")
      item.setAttribute("aria-current", "step");
    else item.removeAttribute("aria-current");
  });
}
export function setIconLabel(button, title, symbol) {
  button.replaceChildren(icon(symbol));
  button.setAttribute("aria-label", title);
  button.title = title;
}
function actionIcon(title) {
  if (/^(cancel|clear)/i.test(title)) return "close";
  if (/copy/i.test(title)) return "copy";
  if (/^add/i.test(title)) return "plus";
  if (/confirm/i.test(title)) return "check";
  if (/^(continue|open)/i.test(title)) return "arrow";
  return null;
}

// An empty list: a soft icon badge on top and a short title, optionally one action.
export function emptyState(symbol, title, button = null) {
  const value = node("div", null, "empty");
  const badge = node("span", null, "empty-icon");
  badge.append(icon(symbol));
  value.append(badge, node("h3", title));
  if (button) value.append(button);
  return value;
}

export function help(text, title = "More information") {
  const wrapper = node("span", null, "help-control");
  const button = node("button", null, "icon-button help-trigger");
  button.type = "button";
  button.append(icon("info"));
  button.setAttribute("aria-label", title);
  button.setAttribute("aria-expanded", "false");
  const content = node("span", text, "help-content");
  content.hidden = true;
  content.setAttribute("role", "tooltip");
  content.id = `help-${++helpID}`;
  button.setAttribute("aria-controls", content.id);
  bindHelp(wrapper, button, content);
  wrapper.append(button, content);
  return wrapper;
}
function bindHelp(wrapper, button, content) {
  content.setAttribute("popover", "manual");
  button.setAttribute("aria-describedby", content.id);
  const show = (visible) => {
    if (!visible) content.hidePopover?.();
    content.hidden = !visible;
    button.setAttribute("aria-expanded", String(visible));
    if (visible && button.getBoundingClientRect) {
      content.showPopover?.();
      const anchor = button.getBoundingClientRect(),
        bounds = content.getBoundingClientRect();
      content.style.left = `${Math.max(8, Math.min(anchor.right - bounds.width, window.innerWidth - bounds.width - 8))}px`;
      content.style.top = `${Math.max(8, anchor.bottom + bounds.height + 8 > window.innerHeight ? anchor.top - bounds.height - 8 : anchor.bottom + 8)}px`;
    }
  };
  button.addEventListener("click", () => show(true));
  button.addEventListener("focus", () => show(true));
  wrapper.addEventListener("pointerenter", (event) => {
    if (event.pointerType === "mouse") show(true);
  });
  wrapper.addEventListener("pointerleave", (event) => {
    if (event.pointerType === "mouse") show(false);
  });
  wrapper.addEventListener("focusout", (event) => {
    if (!wrapper.contains(event.relatedTarget)) show(false);
  });
  wrapper.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !content.hidden) {
      event.preventDefault();
      event.stopPropagation();
      show(false);
      button.focus();
    }
  });
}
let helpID = 0;

// Only explicitly marked explanatory copy is collapsed. Errors, amounts,
// permissions, credentials and payment warnings always remain visible.
export function initializeUI() {
  document.querySelectorAll("[data-help]").forEach((value) => {
    const control = help(
      value.textContent,
      value.dataset.help || "More information",
    );
    const previous = value.previousElementSibling;
    if (previous && /^(H[1-6]|DIV)$/.test(previous.tagName)) {
      previous.append(control);
      value.remove();
    } else if (
      previous &&
      previous.tagName === "LABEL" &&
      previous.firstElementChild?.tagName === "SPAN"
    ) {
      // Help for a labelled input belongs beside the label text, where the
      // icon is vertically centered with the label, not the taller input. The
      // label is tied to its own field so the help button, now its first
      // labelable child, doesn't take the field's hover and clicks.
      const field = previous.querySelector("input, select, textarea");
      if (field?.id) previous.htmlFor = field.id;
      previous.firstElementChild.append(control);
      value.remove();
    } else if (previous && /^(LABEL|BUTTON|SUMMARY)$/.test(previous.tagName)) {
      const row = node("div", null, "row help-associated");
      previous.before(row);
      row.append(previous, control);
      value.remove();
    } else {
      control.prepend(
        node("span", value.dataset.help || "Details", "caption muted"),
      );
      value.replaceWith(control);
    }
  });
  document.querySelectorAll("[data-icon]").forEach((value) => {
    const title = value.getAttribute("aria-label") || value.textContent.trim();
    value.setAttribute("aria-label", title);
    value.title = title;
    if (value.hasAttribute("data-icon-only")) {
      const counters = [...value.querySelectorAll("[id]")];
      value.replaceChildren(icon(value.dataset.icon), ...counters);
      value.classList.add("icon-only");
    } else {
      value.prepend(icon(value.dataset.icon));
    }
  });
  // Rows that open something end in a chevron, as in the apps' lists.
  document
    .querySelectorAll("button.row-button")
    .forEach((row) => row.append(icon("chevron")));
  document.querySelectorAll("[data-overlay]").forEach((button) => {
    bindHelp(button.parentElement, button, el(button.dataset.overlay));
  });
  initializeTheme();
  document.addEventListener("pointerdown", (event) => {
    document.querySelectorAll(".help-control").forEach((control) => {
      if (!control.contains(event.target)) {
        const content = control.querySelector(".help-content");
        content.hidePopover?.();
        content.hidden = true;
        control
          .querySelector(".help-trigger")
          .setAttribute("aria-expanded", "false");
      }
    });
  });
  document.querySelectorAll(".roof-panel").forEach(roof);
  keyboardAwareDialogs();
  document.querySelectorAll("dialog").forEach((dialog) => {
    dialog.addEventListener("close", () => {
      dialog.querySelectorAll(".help-content").forEach((content) => {
        content.hidePopover?.();
        content.hidden = true;
      });
      dialog
        .querySelectorAll(".help-trigger")
        .forEach((button) => button.setAttribute("aria-expanded", "false"));
    });
    dialog.addEventListener("click", (event) => {
      if (event.target !== dialog) return;
      const bounds = dialog.getBoundingClientRect();
      if (
        event.clientX < bounds.left ||
        event.clientX > bounds.right ||
        event.clientY < bounds.top ||
        event.clientY > bounds.bottom
      ) {
        const cancel = new Event("cancel", { cancelable: true });
        if (dialog.dispatchEvent(cancel)) dialog.close();
      }
    });
  });
}
// Which appearance applies: the merchant's lock wins, then the visitor's saved choice,
// then the system's. Anything else reads as the system's.
export function resolveTheme({ lock = "", saved = "", system = false } = {}) {
  const modes = ["light", "dark"];
  if (modes.includes(lock)) return { mode: lock, locked: true, chosen: true };
  if (modes.includes(saved)) return { mode: saved, locked: false, chosen: true };
  return { mode: system ? "dark" : "light", locked: false, chosen: false };
}
// Time left as m:ss, or h:mm:ss from an hour. Never negative.
export function countdown(seconds) {
  const left = Math.max(0, Math.floor(Number(seconds) || 0));
  const s = String(left % 60).padStart(2, "0");
  if (left >= 3600)
    return `${Math.floor(left / 3600)}:${String(Math.floor((left % 3600) / 60)).padStart(2, "0")}:${s}`;
  return `${Math.floor(left / 60)}:${s}`;
}
// Share of an exact total already received, 0 to 100 with one decimal.
export function progressPercent(paid, total) {
  try {
    const whole = BigInt(total);
    if (whole <= 0n) return 0;
    const share = Number((BigInt(paid) * 1000n) / whole) / 10;
    return Math.max(0, Math.min(100, share));
  } catch {
    return 0;
  }
}
// The light/dark toggle. A store whose merchant chose light or dark (data-lock, set
// before first paint) is fixed to it and gets no toggle. Otherwise the appearance
// follows the system's until the visitor picks one, which is remembered on the device.
function initializeTheme() {
  const modes = ["light", "dark"];
  const root = document.documentElement;
  const lock = modes.includes(root.dataset.lock) ? root.dataset.lock : "";
  const store = !!document.body?.classList?.contains("store");
  const colors = store
    ? { light: "#ffffff", dark: "#202124" }
    : { light: "#fefcfa", dark: "#1c2025" };
  const theme = node("button", null, "icon-button theme-toggle");
  theme.type = "button";
  const system = globalThis.matchMedia?.("(prefers-color-scheme: dark)");
  let saved = "";
  try {
    saved = localStorage.getItem("stor.appearance") || "";
  } catch {
    /* Theme remains usable without storage. */
  }
  let { mode, chosen } = resolveTheme({ lock, saved, system: !!system?.matches });
  system?.addEventListener?.("change", (event) => {
    if (chosen) return;
    mode = resolveTheme({ system: event.matches }).mode;
    apply();
  });
  function apply() {
    root.dataset.theme = mode;
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", colors[mode]);
    theme.replaceChildren(icon(mode === "light" ? "moon" : "sun"));
    theme.setAttribute(
      "aria-label",
      `Switch to ${mode === "light" ? "Dark" : "Light"}`,
    );
    theme.title = theme.getAttribute("aria-label");
  }
  theme.addEventListener("click", () => {
    mode = modes[(modes.indexOf(mode) + 1) % modes.length];
    chosen = true;
    apply();
    try {
      localStorage.setItem("stor.appearance", mode);
    } catch {
      /* Nonessential preference only. */
    }
  });
  apply();
  if (!lock) document.querySelector("[data-theme-slot]")?.prepend(theme);
}
// The Payroof roofline (PitchedPanelShape in the apps): a shallow gable with rounded
// eaves and ridge, drawn to the panel's size so the outline stays crisp at any width.
export function roofPath(width, height, pitch = 16, radius = 18) {
  const w = width - 1,
    h = height - 1,
    eave = Math.min(pitch, h / 3),
    c = Math.max(0, Math.min(radius, w / 4, (h - eave) / 2)),
    half = w / 2,
    slope = Math.hypot(half, eave) || 1,
    // A point `d` along the left roof slope from its eave or ridge end.
    up = (d) => [(d * half) / slope, eave - (d * eave) / slope],
    ridge = half ? (2 * c * eave) / half : 0,
    [ex, ey] = up(c),
    [rx, ry] = up(slope - ridge),
    n = (value) => Math.round(value * 100) / 100,
    k = 0.5523,
    // From the current point `a`, round the vertex `v` to end at `b`.
    corner = (a, v, b) =>
      `C${n(a[0] + (v[0] - a[0]) * k)},${n(a[1] + (v[1] - a[1]) * k)} ${n(b[0] + (v[0] - b[0]) * k)},${n(b[1] + (v[1] - b[1]) * k)} ${n(b[0])},${n(b[1])}`,
    segments = [
      [[0, eave + c], [0, eave], [ex, ey]],
      [[rx, ry], [half, 0], [w - rx, ry]],
      [[w - ex, ey], [w, eave], [w, eave + c]],
      [[w, h - c], [w, h], [w - c, h]],
      [[c, h], [0, h], [0, h - c]],
    ];
  return `M${segments
    .map(([a, v, b]) => `${n(a[0])},${n(a[1])} ${corner(a, v, b)}`)
    .join(" L")} Z`;
}
// Draws the roofline behind a .roof-panel and redraws it when the panel resizes.
function roof(panel) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("roof-shape");
  const path = document.createElementNS(svg.namespaceURI, "path");
  svg.append(path);
  panel.prepend(svg);
  const draw = () => {
    const { width, height } = panel.getBoundingClientRect();
    if (!width || !height) return;
    svg.setAttribute("viewBox", `-0.5 -0.5 ${width} ${height}`);
    path.setAttribute("d", roofPath(width, height));
  };
  if (globalThis.ResizeObserver) new ResizeObserver(draw).observe(panel);
  draw();
}
export function formatAtomic(amount, decimals) {
  if (
    !/^(0|[1-9][0-9]*)$/.test(amount) ||
    !Number.isInteger(decimals) ||
    decimals < 0 ||
    decimals > 36
  )
    throw new Error("Invalid exact amount");
  if (!decimals) return amount;
  const value = amount.padStart(decimals + 1, "0");
  const fraction = value.slice(-decimals).replace(/0+$/, "");
  return value.slice(0, -decimals) + (fraction ? "." + fraction : "");
}
export function networkName(chain) {
  return (
    {
      "eip155:1": "Ethereum",
      "eip155:8453": "Base",
      "eip155:42161": "Arbitrum",
      "eip155:137": "Polygon",
      "eip155:10": "Optimism",
      "eip155:31337": "Local Anvil",
    }[chain] || `EVM chain ${chain.replace("eip155:", "")}`
  );
}
// How to show an asset: the operator's symbol and decimals when known. Otherwise
// only what the ID itself proves: an EVM chain's ETH (coin type 60) always has 18
// decimals. A token's decimals are never guessed, so its amount stays exact units.
export function assetInfo(id, assets = []) {
  const known = assets.find((item) => item.id === id);
  const [chain = "", kind = ""] = String(id).split("/");
  const network =
    known?.network || (chain.startsWith("eip155:") ? networkName(chain) : "");
  if (known)
    return {
      symbol: known.symbol,
      name: known.symbol,
      decimals: known.decimals,
      network,
    };
  if (network && kind === "slip44:60")
    return { symbol: "ETH", name: "ETH", decimals: 18, network };
  const contract = kind.startsWith("erc20:") ? kind.slice(6) : "";
  const short =
    contract.length > 12 ? `${contract.slice(0, 6)}…${contract.slice(-4)}` : contract;
  return {
    symbol: short || "units",
    name: short ? `Token ${short}` : "Unknown asset",
    decimals: null,
    network,
  };
}
// The amount alone: formatted when decimals are known, else exact units.
export function amount(option, assets = []) {
  const { decimals } = assetInfo(option.asset, assets);
  return decimals == null ? option.amount : formatAtomic(option.amount, decimals);
}
// The unit that follows an amount: the symbol, or exact units of an unknown token.
export function unitLabel(info) {
  if (info.decimals != null) return info.symbol;
  return info.symbol === "units" ? "units" : `units of ${info.symbol}`;
}
export function price(option, assets = []) {
  return `${amount(option, assets)} ${unitLabel(assetInfo(option.asset, assets))}`;
}
// Payment options grouped by token symbol, readable amounts first. A token payable
// on several networks is one group with an entry per network.
export function paymentGroups(options, assets = []) {
  const groups = new Map();
  for (const option of options) {
    const info = assetInfo(option.asset, assets);
    if (!groups.has(info.symbol))
      groups.set(info.symbol, { symbol: info.symbol, options: [] });
    groups.get(info.symbol).options.push({
      option,
      asset: option.asset,
      chain: chainOf(option.asset),
      amount: amount(option, assets),
      network: info.network,
      exact: info.decimals == null,
      unit: unitLabel(info),
    });
  }
  return [...groups.values()].sort(
    (a, b) => a.options[0].exact - b.options[0].exact,
  );
}
export const chainOf = (asset) => String(asset).split("/")[0];
// The cheapest entry of one token: the smallest exact amount, the first on a tie.
export function cheapest(entries) {
  let best = entries[0];
  for (const entry of entries) {
    try {
      if (BigInt(entry.option.amount) < BigInt(best.option.amount)) best = entry;
    } catch {
      /* An amount that isn't a whole number can't be compared. */
    }
  }
  return best;
}
// What is selected before the buyer chooses: the first token, on its cheapest network.
export function defaultAsset(groups) {
  return groups.length ? cheapest(groups[0].options).asset : "";
}
// Colour keys for the dots and badges (styled in style.css): a known network or token
// has its own, any other one of six by its chain ID or name, so it is stable.
const networkKeys = {
  "eip155:1": "ethereum",
  "eip155:8453": "base",
  "eip155:42161": "arbitrum",
  "eip155:137": "polygon",
  "eip155:10": "optimism",
};
export function networkTone(asset) {
  const chain = chainOf(asset);
  if (networkKeys[chain]) return networkKeys[chain];
  const id = Number(chain.replace("eip155:", ""));
  return `n${Number.isSafeInteger(id) ? id % 6 : 0}`;
}
const tokenKeys = { USDC: "usdc", USDT: "usdt", ETH: "ethereum", DAI: "dai" };
export function tokenTone(symbol) {
  const key = String(symbol).toUpperCase();
  if (tokenKeys[key]) return tokenKeys[key];
  let sum = 0;
  for (const char of key) sum += char.codePointAt(0);
  return `n${sum % 6}`;
}
// The tone of an order's status chip: success, pending, warning or danger.
export function orderTone(status, lapsed = false) {
  if (lapsed) return "danger";
  return (
    {
      delivered: "success",
      creating: "pending",
      awaiting_payment: "pending",
      partial: "pending",
      seen: "pending",
      late_payment: "warning",
      manual_review: "warning",
    }[status] || "danger"
  );
}
// How to pay, in two levels. First the token: a card per symbol with the exact amount
// it costs. Then the network: a card per network the chosen token is accepted on, each
// with its own exact amount, the lowest marked when they differ. One option alone is a
// line, not a choice. `choose` receives the chosen asset ID.
export function paymentPicker(groups, selected, choose, { name, disabled = false }) {
  const flat = groups.flatMap((group) => group.options);
  const current = flat.find((option) => option.asset === selected) || flat[0];
  const group = groups.find((value) => value.options.includes(current));
  const focused = document.activeElement;
  const refocus =
    focused?.type === "radio" && focused.name.startsWith(name)
      ? [focused.name, focused.value]
      : null;
  const section = (label) => {
    const value = node("div", null, "picker-step");
    value.append(node("span", label, "picker-label"));
    return value;
  };
  const cards = (label) => {
    const value = node("div", null, "choices");
    value.setAttribute("role", "radiogroup");
    value.setAttribute("aria-label", label);
    return value;
  };
  const badge = (text, tone, kind) => {
    const value = node("span", [...String(text)][0]?.toUpperCase() || "?", `badge ${kind}`);
    value.dataset.tone = tone;
    value.setAttribute("aria-hidden", "true");
    return value;
  };
  const card = (field, value, checked, pick, parts) => {
    const label = node("label", null, "choice");
    const radio = node("input");
    radio.type = "radio";
    radio.name = field;
    radio.value = value;
    radio.checked = checked;
    radio.disabled = disabled;
    radio.addEventListener("change", pick);
    // Compact chips: the name, and the amount only where it tells options apart.
    const text = node("span", null, "choice-text");
    text.append(node("strong", parts.name, "choice-name"));
    if (parts.price) text.append(node("span", parts.price, "choice-price"));
    if (parts.title) label.title = parts.title;
    label.append(radio, parts.badge, text);
    if (parts.tag) label.append(node("span", parts.tag, "choice-tag"));
    label.append(icon("check"));
    if (refocus?.[0] === field && refocus[1] === value)
      queueMicrotask(() => radio.focus());
    return label;
  };
  const priced = (entry) => `${entry.amount} ${entry.unit}`;
  const root = node("div", null, "picker");
  if (flat.length === 1) {
    const only = node("div", null, "choice fixed");
    const text = node("span", null, "choice-text");
    text.append(
      node("strong", current.network ? `${group.symbol} on ${current.network}` : group.symbol, "choice-name"),
    );
    only.title = priced(current);
    only.append(badge(group.symbol, tokenTone(group.symbol), "token"), text);
    root.append(only);
    return root;
  }
  if (groups.length > 1) {
    const step = section("Token");
    const tokens = cards("Token");
    for (const item of groups) {
      const target =
        item.options.find((option) => option.network === current.network) ||
        cheapest(item.options);
      tokens.append(
        card(`${name}-token`, item.symbol, item === group, () => choose(target.asset), {
          name: item.symbol,
          title: priced(target),
          badge: badge(item.symbol, tokenTone(item.symbol), "token"),
        }),
      );
    }
    step.append(tokens);
    root.append(step);
  }
  if (group.options.length > 1) {
    const step = section("Network");
    const networks = cards("Network");
    const low = cheapest(group.options);
    const differ = group.options.some((option) => option.amount !== low.amount);
    for (const option of group.options) {
      const network = option.network || `Chain ${option.chain.replace("eip155:", "")}`;
      networks.append(
        card(`${name}-network`, option.asset, option === current, () => choose(option.asset), {
          name: network,
          price: differ ? priced(option) : "",
          title: priced(option),
          badge: badge(network, networkTone(option.asset), "net"),
          tag: differ && option === low ? "Lowest" : "",
        }),
      );
    }
    step.append(networks);
    root.append(step);
  } else if (current.network) {
    const step = section("Network");
    const only = node("div", null, "choice fixed");
    const text = node("span", null, "choice-text");
    text.append(node("strong", current.network, "choice-name"));
    only.append(badge(current.network, networkTone(current.asset), "net"), text);
    step.append(only);
    root.append(step);
  }
  return root;
}
// What a buyer reads: one entry per token and amount, naming every network that
// accepts it. A token priced differently per network gets an entry for each.
export function paymentOffers(options, assets = []) {
  return paymentGroups(options, assets).flatMap(({ symbol, options }) => {
    const byAmount = new Map();
    for (const option of options) {
      if (!byAmount.has(option.amount))
        byAmount.set(option.amount, {
          symbol,
          amount: option.amount,
          unit: option.unit,
          option: option.option,
          networks: [],
          assets: [],
        });
      const offer = byAmount.get(option.amount);
      offer.networks.push(option.network);
      offer.assets.push(option.asset);
    }
    return [...byAmount.values()];
  });
}
export function orderAssets(order) {
  // A missing historical display snapshot is unknown, not permission to reuse
  // the current catalog's (possibly different) token decimal count.
  return order.asset_display ? [order.asset_display] : [];
}
export function canonicalOrders(value) {
  if (!Array.isArray(value)) return [];
  return [
    ...new Set(
      value.filter(
        (id) => typeof id === "string" && /^ord_[A-Za-z0-9_-]{1,120}$/.test(id),
      ),
    ),
  ].slice(-20);
}
// Delivery grouped by purchased item. The backend stores one delivery per unit in
// item order. Each item takes as many as it has units. Any surplus forms a final
// group with no item.
export function deliveryGroups(order) {
  const units = [...(order.delivery || [])];
  const groups = [];
  for (const item of order.items || []) {
    const taken = units.splice(0, Math.max(0, item.quantity || 0));
    if (taken.length) groups.push({ item, units: taken });
  }
  if (units.length) groups.push({ item: null, units });
  return groups;
}
export function safeURL(raw) {
  try {
    const url = new URL(raw);
    return ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}
export async function request(
  path,
  { method = "GET", body, signal } = {},
) {
  const headers = {};
  if (body && !(body instanceof FormData))
    headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method,
    headers,
    body:
      body instanceof FormData ? body : body ? JSON.stringify(body) : undefined,
    credentials: "same-origin",
    redirect: "error",
    cache: "no-store",
    signal: signal || AbortSignal.timeout(45000),
  });
  const value = response.status === 204 ? {} : await response.json();
  if (!response.ok) {
    const known = {
      incompatible_payment_options: "Asset not accepted for these items",
      cart_changed: "Bag changed. Check it and pay again",
      inventory_unavailable: "Not enough stock",
      unsupported_asset: "Asset not supported",
      checkout_in_progress: "Previous checkout still preparing",
      body_too_large: "Too large",
    };
    const error = new Error(
      response.status === 429
        ? "Too many requests. Wait a moment."
        : known[value.error] || value.message || "Request failed",
    );
    error.status = response.status;
    error.code = value.error;
    error.requestID = value.request_id;
    throw error;
  }
  return value;
}
// One toast per surface: at the bottom of the page, centred on its main column, or
// at the top of an open dialog (whose actions sit at its bottom). Results fade on
// their own (problems stay longer). A pointer resting on the toast holds it, and
// clicking it closes it. Nothing reserves layout space.
let toastTimer;
// A page toast is centred on #workspace (beside the sidebar, where there is one)
// and sits above a bottom tab bar while one shows.
function placeToasts(region) {
  const area = document.getElementById("workspace")?.getBoundingClientRect();
  const bar = [...document.querySelectorAll(".nav")]
    .filter((nav) => !nav.hidden && getComputedStyle(nav).position === "fixed")
    .map((nav) => nav.getBoundingClientRect())
    .find((rect) => rect.top < innerHeight && rect.bottom >= innerHeight - 1);
  region.style.setProperty("--toast-center", area ? `${area.left + area.width / 2}px` : "50%");
  region.style.setProperty("--toast-lift", bar ? `${innerHeight - bar.top}px` : "0px");
}
let followingResize = false;
function followResize() {
  if (followingResize) return;
  followingResize = true;
  addEventListener("resize", () =>
    document.querySelectorAll(".toast-region.page").forEach(placeToasts),
  );
}
export function notice(message, error = false) {
  clearTimeout(toastTimer);
  document.querySelectorAll(".toast").forEach((value) => value.remove());
  if (!message) return null;
  const host = document.querySelector("dialog[open]") || document.body;
  let region = [...host.children].find((child) =>
    child.classList?.contains("toast-region"),
  );
  if (!region) {
    region = node("div", null, host === document.body ? "toast-region page" : "toast-region");
    region.setAttribute("aria-live", "polite");
    host.append(region);
  }
  if (host === document.body) {
    placeToasts(region);
    followResize();
  }
  const value = node("div", null, `toast ${error ? "error" : "success"}`);
  value.setAttribute("role", error ? "alert" : "status");
  const dismiss = node("button", null, "icon-button");
  dismiss.type = "button";
  setIconLabel(dismiss, "Dismiss", "close");
  value.append(icon(error ? "alert" : "check"), node("span", message), dismiss);
  const close = () => {
    clearTimeout(toastTimer);
    value.remove();
  };
  const schedule = () => {
    clearTimeout(toastTimer);
    toastTimer = setTimeout(close, error ? 8000 : 4000);
  };
  value.addEventListener("click", close);
  value.addEventListener("pointerenter", () => clearTimeout(toastTimer));
  value.addEventListener("pointerleave", schedule);
  schedule();
  region.append(value);
  return value;
}
// Touch screens: opening a dialog must not raise the on-screen keyboard over it, so the
// first text field isn't auto-focused there (the dialog's first control is instead).
const touchScreen = () => globalThis.matchMedia?.("(pointer: coarse)").matches === true;
function showDialog(dialog) {
  const fields = touchScreen() ? [...dialog.querySelectorAll("[autofocus]")] : [];
  fields.forEach((field) => field.removeAttribute("autofocus"));
  try {
    dialog.showModal();
  } finally {
    fields.forEach((field) => field.setAttribute("autofocus", ""));
  }
}
// iOS overlays the keyboard on the page without resizing it (100dvh stays put), which
// hid the bottom-sheet dialogs. While a dialog is open, the visual viewport's height and
// the keyboard's overlap are published as CSS variables. The dialog CSS sits above it.
function keyboardAwareDialogs() {
  const viewport = globalThis.visualViewport;
  if (!viewport) return;
  const root = document.documentElement.style;
  let frame = 0;
  const update = () => {
    frame = 0;
    if (!document.querySelector("dialog[open]")) {
      root.removeProperty("--keyboard-inset");
      root.removeProperty("--viewport-height");
      return;
    }
    const layout = document.documentElement.clientHeight;
    const inset = Math.max(0, Math.round(layout - viewport.offsetTop - viewport.height));
    root.setProperty("--keyboard-inset", `${inset}px`);
    root.setProperty("--viewport-height", `${Math.round(viewport.height)}px`);
    const field = document.activeElement;
    if (inset > 0 && field?.closest("dialog[open]") && field.matches("input, textarea, select")) {
      field.scrollIntoView({ block: "nearest" });
    }
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  viewport.addEventListener("resize", schedule);
  viewport.addEventListener("scroll", schedule);
  document.addEventListener("focusin", schedule);
  document.addEventListener("close", schedule, true);
  keyboardAwareDialogs.update = schedule;
}
// A dialog is a new context: earlier page messages don't follow it.
export function openDialog(id) {
  notice("");
  showDialog(el(id));
  keyboardAwareDialogs.update?.();
}
// Optional: `symbol` (heading tile), `confirm`/`cancel` labels, `confirmIcon`, and a short
// highlighted `note`. Actions stack full-width on phones (primary on top), side by side
// from tablets up.
export function confirmAction(title, detail, destructive = false, options = {}) {
  const dialog = node("dialog");
  dialog.className = "confirmation";
  dialog.setAttribute("aria-labelledby", "confirmation-title");
  const body = node("div", null, "dialog-body stack");
  const heading = node("div", null, "dialog-heading");
  const label = node("h2", title);
  label.id = "confirmation-title";
  if (options.symbol) {
    const tile = node("span", null, destructive ? "tile danger" : "tile");
    tile.setAttribute("aria-hidden", "true");
    tile.append(icon(options.symbol));
    heading.append(tile);
  }
  heading.append(label);
  const content = [heading];
  if (detail) {
    const text = node("p", detail, "muted");
    text.id = "confirmation-detail";
    dialog.setAttribute("aria-describedby", text.id);
    content.push(text);
  }
  if (options.note) {
    const note = node("p", null, destructive ? "note error" : "note");
    note.append(icon(destructive ? "alert" : "info"), node("span", options.note));
    content.push(note);
  }
  const cancel = action(options.cancel || "Cancel", () => dialog.close("cancel"));
  const approve = action(
    options.confirm || (destructive ? "Confirm change" : "Continue"),
    () => dialog.close("confirm"),
    destructive ? "button danger" : "button",
  );
  if (options.confirmIcon) approve.prepend(icon(options.confirmIcon));
  const actions = node("div", null, "dialog-actions");
  actions.append(cancel, approve);
  body.append(...content, actions);
  dialog.append(body);
  document.body.append(dialog);
  return new Promise((resolve) => {
    dialog.addEventListener(
      "close",
      () => {
        const allowed = dialog.returnValue === "confirm";
        dialog.remove();
        resolve(allowed);
      },
      { once: true },
    );
    showDialog(dialog);
    keyboardAwareDialogs.update?.();
    cancel.focus();
  });
}
export async function copy(value, button) {
  if (navigator.clipboard) await navigator.clipboard.writeText(value);
  else {
    // Plain HTTP from another device has no Clipboard API: copy through a selected field.
    const field = node("textarea");
    field.value = value;
    field.setAttribute("readonly", "");
    field.style.cssText = "position:fixed;top:0;left:0;opacity:0";
    (button?.closest("dialog") || document.body).append(field);
    field.select();
    field.setSelectionRange(0, value.length);
    const copied = document.execCommand("copy");
    field.remove();
    if (!copied) throw new Error("Clipboard unavailable");
  }
  if (button) {
    const previous = button.querySelector("svg");
    const check = icon("check");
    previous?.replaceWith(check);
    setTimeout(() => check.replaceWith(previous), 1200);
  }
}
export function drawQR(canvas, matrix) {
  const size = 5,
    n = matrix.length + 8;
  canvas.width = canvas.height = n * size;
  const context = canvas.getContext("2d");
  context.fillStyle = "white";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = "#102c26";
  matrix.forEach((row, y) =>
    row.forEach((v, x) => {
      if (v) context.fillRect((x + 4) * size, (y + 4) * size, size, size);
    }),
  );
  canvas.hidden = false;
}
