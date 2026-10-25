import { proof } from "/proof.js";
import { qr } from "/qr.js";
import {
  el,
  node,
  action,
  request,
  notice,
  price,
  paymentGroups,
  paymentOffers,
  paymentPicker,
  setStepper,
  unitLabel,
  copy,
  countdown,
  progressPercent,
  drawQR,
  openDialog,
  orderAssets,
  initializeUI,
  icon,
  confirmAction,
  setIconLabel,
  deliveryGroups,
  canonicalOrders,
  emptyState,
  safeURL,
  assetInfo,
  amount,
  defaultAsset,
  networkTone,
  orderTone,
  tokenTone,
} from "/ui.js";

initializeUI();

const state = {
  catalog: null,
  cart: { items: [], groups: [] },
  busy: false,
  // The asset last chosen anywhere, and the one chosen for each payment group.
  asset: "",
  choices: {},
  // Which bag payments have their token and network chips open.
  payOpen: {},
  paying: "",
  view: "",
  order: null,
  sku: "",
  quantity: 1,
  // The payment entry picked on the product page.
  pick: null,
};
// An order is polled until it is delivered or failed. Once its payment window has
// ended it is polled slowly, since a late payment can still be delivered.
const final = new Set(["delivered", "failed"]);
const active = new Set(["creating", "awaiting_payment", "partial", "seen"]);
let pollTimer;
let orderGeneration = 0;
function storedOrders() {
  try {
    return canonicalOrders(JSON.parse(sessionStorage.getItem("stor_orders") || "[]"));
  } catch {
    return [];
  }
}
function remember(id) {
  try {
    sessionStorage.setItem(
      "stor_orders",
      JSON.stringify([...storedOrders().filter((value) => value !== id), id].slice(-20)),
    );
  } catch {
    /* Cookie authorization still works without browser storage. */
  }
}
// One operation at a time. The control that started it shows its progress
// (its spinner replaces the icon). Problems and confirmations are toasts.
async function run(work, success = "") {
  if (state.busy) return;
  const trigger = document.activeElement?.closest?.("button");
  const focusLabel = trigger?.getAttribute("aria-label");
  state.busy = true;
  document.body.setAttribute("aria-busy", "true");
  trigger?.setAttribute("aria-busy", "true");
  renderCart();
  updateBuy();
  try {
    await work();
    if (success) notice(success);
  } catch (error) {
    notice(error.message || "Connection interrupted. Refresh before paying.", true);
  } finally {
    state.busy = false;
    trigger?.removeAttribute("aria-busy");
    document.body.setAttribute("aria-busy", "false");
    renderCart();
    updateBuy();
    if (focusLabel) {
      const button = [...document.querySelectorAll("button[aria-label]")].find(
        (value) =>
          value.getAttribute("aria-label") === focusLabel &&
          !value.disabled &&
          !value.closest("[hidden]") &&
          (!value.closest("dialog") || value.closest("dialog").open),
      );
      button?.focus();
    }
  }
}
async function load() {
  state.catalog = await request("/v1/catalog");
  state.cart = await request("/v1/cart");
  const { title, description, website } = state.catalog.store;
  el("name").textContent = title;
  el("brand-name").textContent = title;
  el("description").textContent = description;
  el("description").hidden = !description;
  const site = website && safeURL(website);
  el("website").hidden = !site;
  if (site) {
    el("website").href = site;
    el("website").lastChild.textContent = new URL(site).host;
  }
  el("store-intro").hidden = !description && !site;
  el("store-status").hidden = state.catalog.status.status === "active";
  el("search-field").hidden = state.catalog.products.length <= 6;
  el("catalog-view").hidden = !state.catalog.products.length;
  renderCatalog();
  renderCart();
}
// Merchant photos load from their HTTPS host without a referrer. A missing or broken
// one becomes the box icon.
function photo(url, alt = "") {
  const src = safeURL(url);
  if (!src?.startsWith("https:")) return icon("box");
  const image = node("img");
  image.alt = alt;
  image.loading = "lazy";
  image.decoding = "async";
  image.referrerPolicy = "no-referrer";
  image.addEventListener("error", () => image.replaceWith(icon("box")));
  image.src = src;
  return image;
}
const productPath = (sku) => `/p/${encodeURIComponent(sku)}`;
// Cards or a compact list. The shopper's choice is remembered on this device.
function catalogView(view) {
  view = view === "list" ? "list" : "cards";
  try {
    localStorage.setItem("stor.catalog-view", view);
  } catch {}
  el("catalog").classList.toggle("product-list", view === "list");
  for (const button of el("catalog-view").querySelectorAll("[data-view]"))
    button.setAttribute("aria-pressed", String(button.dataset.view === view));
}
el("catalog-view").addEventListener("click", (event) => {
  const button = event.target.closest("[data-view]");
  if (button) catalogView(button.dataset.view);
});
try {
  catalogView(localStorage.getItem("stor.catalog-view"));
} catch {
  catalogView("cards");
}
function renderCatalog() {
  if (!state.catalog) return;
  const search = el("search").value.trim().toLocaleLowerCase();
  const products = state.catalog.products
    .filter((p) => `${p.title} ${p.description}`.toLocaleLowerCase().includes(search))
    .sort((a, b) => a.title.localeCompare(b.title));
  el("catalog").replaceChildren();
  const total = state.catalog.products.length;
  el("catalog-count").textContent = !total
    ? ""
    : products.length === total
      ? `${total} ${total === 1 ? "product" : "products"}`
      : `${products.length} of ${total} products`;
  if (!products.length) {
    const empty = emptyState(search ? "search" : "box", search ? "No matches" : "No products");
    empty.classList.add("catalog-empty");
    el("catalog").append(empty);
    return;
  }
  for (const product of products) {
    const card = node("a", null, "product-card");
    card.href = productPath(product.sku);
    card.dataset.nav = "";
    const media = node("span", null, "product-media");
    media.append(photo(product.images[0]));
    const soldOut = product.available <= 0;
    if (soldOut) card.classList.add("sold-out");
    const body = node("span", null, "product-body");
    body.append(node("h3", product.title, "product-title"));
    // The price as the merchant wrote it ($25), then what is left, as two cells of a
    // row. The tokens it can be paid in, with their exact amounts, are on its page.
    const groups = paymentGroups(product.payment_options, state.catalog.assets);
    const [lead] = paymentOffers(product.payment_options, state.catalog.assets);
    const meta = node("span", null, "product-meta");
    const [stock, tone] = stockLabel(product.available);
    const cost = node("span", null, "cell product-price");
    cost.textContent = product.price && lead ? product.price : lead ? `${lead.amount} ${lead.unit}` : "–";
    meta.append(cost, node("span", stock, `cell stock ${tone}`));
    body.append(meta);
    if (groups.length) {
      const tokens = node("span", null, "product-tokens");
      tokens.setAttribute("aria-label", `Pay with ${groups.map((group) => group.symbol).join(", ")}`);
      for (const group of groups.slice(0, 3)) {
        const chip = node("span", group.symbol, "token-chip");
        chip.dataset.tone = tokenTone(group.symbol);
        tokens.append(chip);
      }
      if (groups.length > 3) tokens.append(node("span", `+${groups.length - 3}`, "token-more"));
      body.append(tokens);
    }
    card.append(media, body);
    el("catalog").append(card);
  }
}
// What is left, for a cell: a text and its tone (a few left or none are flagged).
function stockLabel(available) {
  if (available <= 0) return ["Sold out", "out"];
  if (available < 10) return [`Only ${available} left`, "low"];
  return [`${available} in stock`, "in"];
}
async function addProduct(product, count) {
  const latest = await request("/v1/cart");
  const quantity = Math.min(
    100,
    (latest.items.find((item) => item.sku === product.sku)?.quantity || 0) + count,
  );
  state.cart = await request(`/v1/cart/${encodeURIComponent(product.sku)}`, {
    method: "PUT",
    body: { quantity },
  });
  if (state.view) state.asset = state.view;
  state.quantity = 1;
}
function setQuantity(sku, quantity) {
  return run(async () => {
    state.cart = await request(
      `/v1/cart/${encodeURIComponent(sku)}`,
      quantity ? { method: "PUT", body: { quantity } } : { method: "DELETE" },
    );
  });
}
function stepper(label, symbol, handler, disabled) {
  const button = node("button", null, "icon-button");
  button.type = "button";
  setIconLabel(button, label, symbol);
  button.disabled = disabled;
  button.addEventListener("click", handler);
  return button;
}
// A bag is paid in as few orders as possible: items that share an asset form one
// payment group, and a bag whose items share none is paid in several, one per group.
const groupKey = (group) => group.skus.join(",");
function chosenOption(group) {
  const options = group.payment_options;
  return (
    options.find((option) => option.asset === state.choices[groupKey(group)]) ||
    options.find((option) => option.asset === state.asset) ||
    options.find(
      (option) =>
        option.asset === defaultAsset(paymentGroups(options, state.catalog?.assets)),
    ) ||
    options[0]
  );
}
function cartRow(item, asset, assets) {
  const product = state.catalog?.products.find((p) => p.sku === item.sku);
  const option = item.payment_options.find((p) => p.asset === asset) || item.payment_options[0];
  const row = node("div", null, "cart-row cart-item");
  const link = node("a", null, "cart-name");
  link.href = productPath(item.sku);
  link.dataset.nav = "";
  link.append(node("h3", item.title));
  if (option)
    link.append(node("small", price({ asset: option.asset, amount: option.unit_amount }, assets)));
  row.append(thumbnail(product?.images[0], item.title), link);
  if (option)
    row.append(
      node("p", price({ asset: option.asset, amount: option.line_total }, assets), "cart-line-price"),
    );
  const controls = node("div", null, "cart-controls");
  const quantity = node("div", null, "quantity-control");
  const most = Math.max(1, Math.min(100, item.available));
  // The count can be typed: it is applied when the field is left or Enter is pressed.
  const field = node("input", null, "quantity-input");
  field.type = "number";
  field.min = "1";
  field.max = String(most);
  field.step = "1";
  field.inputMode = "numeric";
  field.value = String(item.quantity);
  field.disabled = state.busy;
  field.setAttribute("aria-label", `Quantity of ${item.title}`);
  field.addEventListener("change", () => {
    const typed = Math.floor(Number(field.value));
    const next = Math.min(most, typed);
    field.value = String(item.quantity);
    if (Number.isFinite(typed) && next >= 1 && next !== item.quantity) setQuantity(item.sku, next);
  });
  quantity.append(
    stepper(
      `Decrease quantity of ${item.title}`,
      "minus",
      () => setQuantity(item.sku, item.quantity - 1),
      state.busy,
    ),
    field,
    stepper(
      `Increase quantity of ${item.title}`,
      "plus",
      () => setQuantity(item.sku, item.quantity + 1),
      state.busy || item.quantity >= 100 || item.quantity >= item.available,
    ),
  );
  controls.append(
    quantity,
    stepper(`Remove ${item.title}`, "trash", () => setQuantity(item.sku, 0), state.busy),
  );
  row.append(controls);
  return row;
}
// Asset, total and the button that pays one group.
function paySection(group, index, count, assets) {
  const key = groupKey(group);
  const option = chosenOption(group);
  const section = node("div", null, "checkout stack");
  // What pays, in one line; the token and network chips open under it on demand, so a
  // store with many tokens never pushes the bag's lines out of view.
  const groups = paymentGroups(group.payment_options, assets);
  const chosen = groups.flatMap((value) => value.options).find((value) => value.asset === option.asset);
  const symbol = groups.find((value) => value.options.includes(chosen))?.symbol || "";
  const choices = node("details", null, "pay-with");
  const several = group.payment_options.length > 1;
  choices.open = several && !!state.payOpen[key];
  choices.addEventListener("toggle", () => (state.payOpen[key] = choices.open));
  const summary = node("summary", null, "pay-with-summary");
  const current = node("span", null, "pay-with-current");
  const dot = node("span", null, "badge token");
  dot.dataset.tone = tokenTone(symbol);
  dot.setAttribute("aria-hidden", "true");
  current.append(dot, node("strong", symbol));
  if (chosen?.network) current.append(node("span", `on ${chosen.network}`, "muted"));
  summary.append(node("span", "Pay with", "caption muted"), current);
  if (several) {
    summary.append(node("span", "Change", "pay-with-change"));
  } else summary.tabIndex = -1;
  choices.append(
    summary,
    paymentPicker(
      groups,
      option.asset,
      (asset) => {
        state.choices[key] = asset;
        state.asset = asset;
        renderCart();
      },
      { name: `payment-${index}`, disabled: state.busy },
    ),
  );
  const total = node("div", null, "cart-total-row");
  total.append(node("span", "Total", "total-label"), node("strong", price(option, assets), "amount"));
  const title = count > 1 ? `Pay ${index + 1} of ${count}` : "Checkout";
  const pay = node("button", null, "button full");
  pay.type = "button";
  pay.append(icon("arrow"), node("span", title));
  pay.setAttribute("aria-label", title);
  pay.disabled = state.busy || state.catalog?.status.status !== "active";
  if (state.busy && state.paying === key) pay.setAttribute("aria-busy", "true");
  pay.addEventListener("click", () => checkout(group, option.asset));
  section.append(choices, total, pay);
  return section;
}
// Lines in a bordered sheet, numbered like its rows.
function sheetOf(rows) {
  const sheet = node("div", null, "cart-sheet");
  sheet.append(...rows);
  return sheet;
}
function renderCart() {
  const cart = state.cart;
  const assets = state.catalog?.assets;
  const count = cart.items.reduce((sum, item) => sum + item.quantity, 0);
  el("cart-count").textContent = String(count);
  el("cart-count").dataset.count = String(count);
  el("cart-open").setAttribute("aria-label", `Open bag, ${count} items`);
  el("cart").replaceChildren();
  if (!cart.items.length) el("cart").append(emptyState("bag", "Your bag is empty"));
  el("cart-steps").hidden = !cart.items.length;
  setStepper(el("cart-steps"), ["current", "", "", ""]);
  const single = cart.groups.length === 1;
  cart.groups.forEach((group, index) => {
    const asset = chosenOption(group)?.asset;
    const rows = group.skus
      .map((sku) => cart.items.find((item) => item.sku === sku))
      .filter(Boolean)
      .map((item) => cartRow(item, asset, assets));
    if (single) {
      el("cart").append(sheetOf(rows));
      return;
    }
    // Several payments: each group is its own block with its own checkout.
    const block = node("section", null, "cart-group");
    const heading = node("h3", null, "cart-group-title");
    heading.append(icon("wallet"), node("span", `Payment ${index + 1} of ${cart.groups.length}`));
    block.append(heading, sheetOf(rows), paySection(group, index, cart.groups.length, assets));
    el("cart").append(block);
  });
  // Lines sold out since they were added are in no payment: shown to be removed.
  const grouped = new Set(cart.groups.flatMap((group) => group.skus));
  const gone = cart.items.filter((item) => !grouped.has(item.sku));
  if (gone.length) {
    const block = node("section", null, "cart-group");
    const heading = node("h3", null, "cart-group-title");
    heading.append(icon("alert"), node("span", "No longer available"));
    block.append(heading, sheetOf(gone.map((item) => cartRow(item, null, assets))));
    el("cart").append(block);
  }
  el("cart-summary").replaceChildren(...(single ? [paySection(cart.groups[0], 0, 1, assets)] : []));
  el("cart-summary").hidden = !single;
  el("clear").disabled = state.busy || !cart.items.length;
}
function checkout(group, asset) {
  return run(async () => {
    state.paying = groupKey(group);
    const altcha = await proof("/v1/altcha?purpose=checkout");
    const order = await request("/v1/checkout", {
      method: "POST",
      body: { asset, skus: group.skus, altcha },
    });
    remember(order.id);
    el("cart-dialog").close();
    clearTimeout(pollTimer);
    orderGeneration++;
    renderOrder(order);
    await navigate(`${location.pathname}#order/${order.id}`);
    state.cart = await request("/v1/cart");
    state.catalog = await request("/v1/catalog");
    // The rest of a split bag waits there for its own payment.
    if (state.cart.items.length) notice("The rest is still in your bag");
  }).finally(() => (state.paying = ""));
}
const gallery = { index: 0, count: 0, show: () => {} };
function stepGallery(by) {
  if (gallery.count > 1) gallery.show((gallery.index + by + gallery.count) % gallery.count);
}
el("product-gallery").addEventListener("keydown", (event) => {
  const by = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
  if (!by) return;
  event.preventDefault();
  stepGallery(by);
});
let swipeFrom = null;
el("product-image").addEventListener("pointerdown", (event) => (swipeFrom = event.clientX));
el("product-image").addEventListener("pointercancel", () => (swipeFrom = null));
el("product-image").addEventListener("pointerup", (event) => {
  if (swipeFrom !== null && Math.abs(event.clientX - swipeFrom) > 40)
    stepGallery(event.clientX < swipeFrom ? 1 : -1);
  swipeFrom = null;
});
function renderPayments(product) {
  const groups = paymentGroups(product.payment_options, state.catalog.assets);
  const options = groups.flatMap((group) => group.options);
  const wanted = state.view || state.asset || defaultAsset(groups);
  const pick = options.find((option) => option.asset === wanted) || options[0];
  state.pick = pick;
  const main = node("p", null, "price-main");
  // Exact amounts can carry up to 36 decimals: long ones get a smaller size.
  main.classList.toggle("long", pick.amount.length > 10);
  main.append(node("strong", pick.amount), node("span", pick.unit));
  if (pick.network) main.append(node("small", `on ${pick.network}`, "price-network"));
  el("product-payments").replaceChildren(
    main,
    paymentPicker(
      groups,
      pick.asset,
      (asset) => {
        state.view = asset;
        renderPayments(product);
        updateBuy();
      },
      { name: "product-pay" },
    ),
  );
}
// SKU, what is left and what it can be paid in, as the rows of a small table.
function renderFacts(product) {
  const groups = paymentGroups(product.payment_options, state.catalog.assets);
  const rows = [
    ["SKU", product.sku, "mono"],
    ["Stock", stockLabel(product.available)[0], ""],
    [
      "Accepts",
      groups
        .map((group) => {
          const networks = [...new Set(group.options.map((option) => option.network).filter(Boolean))];
          return networks.length ? `${group.symbol} on ${networks.join(", ")}` : group.symbol;
        })
        .join("\n"),
      "lines",
    ],
  ];
  el("product-facts").replaceChildren(
    ...rows.map(([name, value, kind]) => {
      const row = node("div", null, "fact");
      row.append(node("dt", name), node("dd", value, kind));
      return row;
    }),
  );
}
const currentProduct = () => state.catalog?.products.find((p) => p.sku === state.sku);
function renderProduct() {
  const product = currentProduct();
  el("product-found").hidden = !product;
  el("product-missing").hidden = !!product;
  const store = state.catalog.store.title;
  if (!product) {
    const back = node("a", null, "button secondary");
    back.href = "/";
    back.dataset.nav = "";
    back.append(icon("back"), node("span", store));
    el("product-missing").replaceChildren(emptyState("search", "Not found", back));
    document.title = store;
    return;
  }
  document.title = `${product.title} · ${store}`;
  el("product-title").textContent = product.title;
  el("product-description").textContent = product.description;
  el("product-description").hidden = !product.description;
  // The main photo and, with several, thumbnails that switch it. A photo that fails to
  // load leaves a compact placeholder, and its thumbnail disappears.
  const photos = product.images;
  const multiple = photos.length > 1;
  el("product-thumbs").replaceChildren();
  if (multiple) el("product-image").setAttribute("tabindex", "0");
  else el("product-image").removeAttribute("tabindex");
  gallery.count = photos.length;
  gallery.show = (index) => {
    gallery.index = index;
    el("product-image").replaceChildren(
      photo(photos[index], `${product.title}, photo ${index + 1}`),
    );
    [...el("product-thumbs").children].forEach((thumb, position) =>
      thumb.setAttribute("aria-pressed", String(position === index)),
    );
  };
  if (multiple)
    photos.forEach((url, index) => {
      const thumb = node("button");
      thumb.type = "button";
      thumb.setAttribute("aria-label", `Photo ${index + 1}`);
      thumb.append(photo(url));
      thumb.addEventListener("click", () => gallery.show(index));
      el("product-thumbs").append(thumb);
    });
  gallery.show(0);
  state.view = "";
  renderPayments(product);
  renderFacts(product);
  updateBuy();
}
function updateBuy() {
  const product = currentProduct();
  if (!product || el("product-view").hidden) return;
  const inBag = state.cart.items.find((item) => item.sku === product.sku)?.quantity || 0;
  const room = Math.min(product.available, 100) - inBag;
  const soldOut = product.available <= 0;
  const buyable =
    !state.busy && !soldOut && room > 0 && state.catalog.status.status === "active";
  state.quantity = Math.max(1, Math.min(state.quantity, room));
  const flags = [];
  const flag = (symbol, text, tone) => {
    const value = node("span", null, `pill ${tone}`);
    value.append(icon(symbol), node("span", text));
    flags.push(value);
  };
  if (soldOut) flag("alert", "Sold out", "danger");
  else {
    if (product.available < 10) flag("alert", `${product.available} left`, "warning");
    if (inBag) flag("bag", `${inBag} in bag`, "success");
  }
  el("product-status").replaceChildren(...flags);
  el("product-buy").hidden = soldOut;
  el("qty-value").textContent = String(state.quantity);
  el("qty-minus").disabled = !buyable || state.quantity <= 1;
  el("qty-plus").disabled = !buyable || state.quantity >= room;
  // The line total in the token picked above, exact.
  let total = "";
  try {
    const { asset, option } = state.pick;
    const cost = (BigInt(option.amount) * BigInt(state.quantity)).toString();
    total = `Total ${price({ asset, amount: cost }, state.catalog.assets)}`;
  } catch {
    /* No exact total without an exact amount. */
  }
  el("product-total").textContent = soldOut ? "" : total;
  el("product-add").disabled = !buyable;
}
// Order states: which of Pay → Confirm → Deliver it is at, whether it went wrong
// there, a title, and the one instruction that matters when something is off.
const orderStates = {
  creating: [0, false, "Preparing", ""],
  awaiting_payment: [0, false, "Awaiting payment", ""],
  partial: [0, false, "Partly paid", ""],
  seen: [1, false, "Payment detected", "Don't send it again."],
  delivered: [2, false, "Delivered", ""],
  expired: [
    0,
    true,
    "Expired",
    "Payment no longer expected. Sent late, it's delivered while in stock.",
  ],
  failed: [0, true, "Order failed", "Don't pay."],
  late_payment: [1, true, "Paid late", "Out of stock. Contact the merchant."],
  manual_review: [1, true, "Under review", "Contact the merchant. Don't pay again."],
};
// The window is the merchant's stock reservation, counted from the server's expires_at.
const lapsed = (order) =>
  ["awaiting_payment", "partial"].includes(order.status) &&
  Date.parse(order.expires_at) <= Date.now();
let expiryTimer;
function renderExpiry(order) {
  clearTimeout(expiryTimer);
  const left = Math.ceil((Date.parse(order.expires_at) - Date.now()) / 1000);
  const counting = ["awaiting_payment", "partial"].includes(order.status) && left > 0;
  el("payment-expiry").hidden = !counting;
  if (!counting) return;
  el("expiry-time").textContent = `${countdown(left)} left`;
  expiryTimer = setTimeout(() => (left > 1 ? renderExpiry(order) : renderOrder(order)), 1000);
}
// The address in monospace: its first six and last four characters stand out, the
// middle is muted. The whole value is the accessible name and what copy copies.
function showAddress(address) {
  const code = el("payment-address");
  if (!address) {
    code.removeAttribute("aria-label");
    code.textContent = "…";
    return;
  }
  code.setAttribute("aria-label", address);
  code.replaceChildren(
    node("b", address.slice(0, 6)),
    node("span", address.slice(6, -4)),
    node("b", address.slice(-4)),
  );
}
function thumbnail(url, label) {
  const frame = node("span", null, "thumb");
  frame.append(photo(url));
  frame.setAttribute("aria-hidden", "true");
  frame.title = label;
  return frame;
}
function renderOrder(order) {
  state.order = order;
  const [step, problem, title, help] = orderStates[lapsed(order) ? "expired" : order.status] || [
    0,
    false,
    "Checking",
    "Refresh before sending anything.",
  ];
  const paying = ["creating", "awaiting_payment", "partial"].includes(order.status) && !lapsed(order);
  const delivered = order.status === "delivered";
  el("order-id").textContent = `#${order.id.slice(-6)}`;
  // The stepper shows where the order is. Its name is announced, and only a problem
  // or a next step is spelled out.
  el("order-status-title").textContent = title;
  el("order-status-title").dataset.tone = orderTone(order.status, lapsed(order));
  el("order-help").dataset.tone = problem ? "problem" : "";
  el("order-help").textContent = help;
  el("order-help").hidden = !help;
  setStepper(el("order-steps"), [
    "done",
    ...[0, 1, 2].map((position) =>
      position < step || (delivered && position === step)
        ? "done"
        : position === step
          ? problem
            ? "problem"
            : "current"
          : "",
    ),
  ]);
  const assets = orderAssets(order);
  const info = assetInfo(order.asset, assets);
  el("payment-area").hidden = !paying;
  el("payment-amount").replaceChildren(
    amount({ asset: order.asset, amount: order.remaining }, assets),
    node("span", unitLabel(info), "unit"),
  );
  // What has arrived of what is due: a partial payment counts, the rest is still to send.
  const partial = order.paid !== "0";
  el("payment-caption").textContent = partial ? "Send the remaining" : "Send exactly";
  el("payment-progress-box").hidden = !partial;
  if (partial) {
    el("payment-received").textContent = `${price({ asset: order.asset, amount: order.paid }, assets)} of ${price({ asset: order.asset, amount: order.amount }, assets)}`;
    el("payment-progress").value = progressPercent(order.paid, order.amount);
  }
  // The one network this payment counts on, and what happens to the same token sent on
  // another.
  const chain = order.chain.replace("eip155:", "");
  const network = info.network || `chain ${chain}`;
  el("payment-network").textContent = network;
  el("payment-network").dataset.tone = networkTone(order.asset);
  el("payment-network").title = `Send on ${info.network || "this network"} only (chain ID ${chain})`;
  el("payment-warning-text").textContent = `Send only on ${network}. Funds sent on another network are lost.`;
  showAddress(order.address);
  el("payment-atomic").textContent = `${order.remaining} · ${order.asset}`;
  const mayPay = paying && order.status !== "creating" && !!order.address;
  el("address-copy").disabled = !mayPay;
  el("amount-copy").disabled = !mayPay;
  // The QR code sits beside the payment details, never inside them: it is only drawn
  // once the address can be paid.
  el("payment-qr-card").hidden = true;
  if (mayPay) {
    try {
      drawQR(el("payment-qr"), qr(order.address));
      el("payment-qr-card").hidden = false;
    } catch {
      /* Full address remains visible and copyable. */
    }
  }
  renderExpiry(order);
  el("payment-details").replaceChildren(
    node("p", order.id, "mono"),
    node("p", new Date().toLocaleTimeString()),
  );
  if (order.transaction) el("payment-details").append(node("p", order.transaction, "mono"));
  el("order-summary").hidden = delivered;
  el("order-items").replaceChildren(
    ...(order.items || []).map((item) => {
      const row = node("li", null, "order-line");
      const name = node("span", item.title);
      name.append(node("small", `× ${item.quantity}`));
      let total = "";
      try {
        total = price(
          { asset: order.asset, amount: (BigInt(item.unit_amount) * BigInt(item.quantity)).toString() },
          assets,
        );
      } catch {
        /* A line without an exact amount shows none. */
      }
      row.append(thumbnail(item.image, item.title), name, node("strong", total));
      return row;
    }),
  );
  el("order-total-label").textContent = order.transaction ? "Paid" : "Total";
  el("order-total").textContent = price({ asset: order.asset, amount: order.amount }, assets);
  // Delivery: one card per purchased item, holding each unit it delivered.
  el("delivery").hidden = !delivered;
  el("delivery").replaceChildren(
    ...(delivered
      ? deliveryGroups(order).map(({ item, units }) => {
          const card = node("section", null, "delivery-card");
          const head = node("div", null, "delivery-head");
          const name = node("span", item?.title || "Delivery");
          if (units.length > 1) name.append(node("small", `× ${units.length}`));
          head.append(thumbnail(item?.image, item?.title || ""), name);
          card.append(head);
          units.forEach((value) => {
            const unit = node("div", null, "delivery-item");
            const actions = node("span", null, "row");
            const link = safeURL(value.trim());
            if (link?.startsWith("https:") && !/\s/.test(value.trim())) {
              const open = node("a", null, "icon-button");
              open.href = link;
              open.target = "_blank";
              open.rel = "noopener noreferrer";
              open.append(icon("external"));
              open.setAttribute("aria-label", "Open link");
              open.title = "Open link";
              actions.append(open);
            }
            const copyButton = node("button", null, "icon-button");
            copyButton.type = "button";
            setIconLabel(copyButton, "Copy delivery", "copy");
            copyButton.addEventListener("click", () =>
              copy(value, copyButton)
                .then(() => notice("Copied"))
                .catch((error) => notice(error.message, true)),
            );
            actions.append(copyButton);
            unit.append(node("pre", value, "delivery"), actions);
            card.append(unit);
          });
          return card;
        })
      : []),
  );
}
// Three views, each with its own URL: the shop (/), a product (/p/<sku>) and one order
// (#order/<id>, with the page it was opened from kept underneath). History API
// navigation keeps Back and Forward, reloads and shared links working.
function orderInURL() {
  return /^#order\/(ord_[A-Za-z0-9_-]{1,120})$/.exec(location.hash)?.[1] || "";
}
const skuInURL = () => /^\/p\/([A-Za-z0-9._~-]{1,64})$/.exec(location.pathname)?.[1] || "";
function navigate(url) {
  history.pushState({ app: true }, "", url);
  return route();
}
async function route() {
  if (!state.catalog) return;
  const id = orderInURL();
  state.sku = skuInURL();
  el("shop-view").hidden = !!id || !!state.sku;
  el("product-view").hidden = !!id || !state.sku;
  el("order-panel").hidden = !id;
  window.scrollTo(0, 0);
  if (!id) {
    orderGeneration++;
    clearTimeout(pollTimer);
    clearTimeout(expiryTimer);
    state.order = null;
    document.title = state.catalog.store.title;
    if (state.sku) {
      state.quantity = 1;
      renderProduct();
      el("product-title").focus({ preventScroll: true });
    }
    return;
  }
  document.title = `#${id.slice(-6)} · ${state.catalog.store.title}`;
  if (state.order?.id === id) {
    el("order-heading").focus();
    schedulePoll(orderGeneration);
    return;
  }
  await run(async () => {
    try {
      await selectOrder(id);
    } catch {
      history.replaceState(null, "", location.pathname);
      route();
      throw new Error("Only the browser that placed this order can open it.");
    }
  });
}
window.addEventListener("popstate", route);
document.addEventListener("click", (event) => {
  const link = event.target.closest?.("a[data-nav]");
  if (
    !link ||
    event.defaultPrevented ||
    event.button ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  )
    return;
  event.preventDefault();
  el("cart-dialog").close();
  navigate(link.getAttribute("href"));
});
el("order-back").addEventListener("click", () => {
  if (history.state?.app) history.back();
  else {
    history.replaceState(null, "", location.pathname);
    route();
  }
});
el("order-resume-open").addEventListener("click", () =>
  navigate(`${location.pathname}#order/${el("order-resume-open").dataset.id}`),
);
async function selectOrder(id, focus = true) {
  const generation = ++orderGeneration;
  clearTimeout(pollTimer);
  const order = await request(`/v1/orders/${encodeURIComponent(id)}`);
  if (generation !== orderGeneration) return;
  renderOrder(order);
  if (focus) el("order-heading").focus();
  schedulePoll(generation);
}
function schedulePoll(generation) {
  clearTimeout(pollTimer);
  if (document.hidden || !state.order || final.has(state.order.status)) return;
  pollTimer = setTimeout(async () => {
    if (generation !== orderGeneration) return;
    try {
      const order = await request(`/v1/orders/${encodeURIComponent(state.order.id)}`);
      if (generation !== orderGeneration) return;
      renderOrder(order);
    } catch {
      notice("Status unavailable. Don't pay again.", true);
    }
    if (generation === orderGeneration) schedulePoll(generation);
  }, active.has(state.order.status) ? 5000 : 15000);
}
el("search").addEventListener("input", renderCatalog);
el("cart-open").addEventListener("click", () => {
  renderCart();
  openDialog("cart-dialog");
});
document
  .querySelectorAll("[data-close]")
  .forEach((button) => button.addEventListener("click", () => el(button.dataset.close).close()));
el("clear").addEventListener("click", async () => {
  if (state.busy || !(await confirmAction("Empty bag?", "", true))) return;
  run(async () => {
    state.cart = await request("/v1/cart", { method: "DELETE" });
  });
});
el("qty-minus").addEventListener("click", () => {
  state.quantity--;
  updateBuy();
});
el("qty-plus").addEventListener("click", () => {
  state.quantity++;
  updateBuy();
});
el("product-add").addEventListener("click", () => {
  const product = currentProduct();
  if (product) run(() => addProduct(product, state.quantity), "Added to bag");
});
el("order-refresh").addEventListener("click", () => {
  if (state.order) run(() => selectOrder(state.order.id, false));
});
el("address-copy").addEventListener("click", () =>
  copy(state.order.address, el("address-copy"))
    .then(() => notice("Copied"))
    .catch((error) => notice(error.message, true)),
);
el("amount-copy").addEventListener("click", () => {
  const order = { asset: state.order.asset, amount: state.order.remaining };
  copy(amount(order, orderAssets(state.order)), el("amount-copy"))
    .then(() => notice("Copied"))
    .catch((error) => notice(error.message, true));
});
// Orders opened in this browser, newest first, as the rows of a sheet: the order, what it
// holds, what it cost and where it is. Each row opens that order.
el("history-open").addEventListener("click", () =>
  run(async () => {
    el("history-list").replaceChildren();
    openDialog("history-dialog");
    const ids = storedOrders().reverse();
    if (!ids.length) {
      el("history-list").append(emptyState("tray", "No orders"));
      return;
    }
    const table = node("table", null, "sheet-table history-table");
    table.append(node("caption", "Your orders", "sr-only"));
    const head = node("tr");
    for (const [name, className] of [
      ["Order", ""],
      ["Items", "wide-only"],
      ["Total", "number"],
      ["Status", ""],
    ]) {
      const cell = node("th", name, className);
      cell.scope = "col";
      head.append(cell);
    }
    table.append(node("thead"), node("tbody"));
    table.tHead.append(head);
    el("history-list").append(table);
    for (const id of ids) {
      const row = node("tr");
      try {
        const order = await request(`/v1/orders/${encodeURIComponent(id)}`);
        const ended = lapsed(order);
        const open = action(
          "",
          () => {
            el("history-dialog").close();
            navigate(`${location.pathname}#order/${id}`);
          },
          "text-button",
        );
        open.append(icon("arrow"), node("span", `#${id.slice(-6)}`, "mono"));
        open.setAttribute("aria-label", `Open order ${id.slice(-6)}`);
        const first = order.items?.[0]?.title || "";
        const more = (order.items?.length || 0) - 1;
        const status = node("span", orderStates[ended ? "expired" : order.status]?.[2] || "Checking", "status-chip");
        status.dataset.tone = orderTone(order.status, ended);
        const [name, items, total, where] = ["th", "td", "td", "td"].map((tag) => node(tag));
        name.scope = "row";
        name.append(open);
        items.className = "wide-only";
        items.textContent = more > 0 ? `${first} +${more}` : first;
        total.className = "number";
        total.textContent = price({ asset: order.asset, amount: order.amount }, orderAssets(order));
        where.append(status);
        row.append(name, items, total, where);
      } catch {
        const cell = node("td", `#${id.slice(-6)} is unavailable`, "muted");
        cell.colSpan = 4;
        row.append(cell);
      }
      table.tBodies[0].append(row);
    }
  }),
);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) clearTimeout(pollTimer);
  else schedulePoll(orderGeneration);
});
// The catalog loads first. The URL's view (a product, an order) is routed after that
// operation ends: an order's route() starts its own, and run() allows one at a time.
run(async () => {
  await load();
  if (orderInURL()) return;
  // An order still in progress is offered from the shop, not opened over it.
  const id = storedOrders().at(-1);
  if (!id) return;
  try {
    const order = await request(`/v1/orders/${encodeURIComponent(id)}`);
    if (!active.has(order.status) || orderInURL()) return;
    el("order-resume-text").textContent =
      order.status === "seen" ? "Confirming payment" : "Awaiting payment";
    el("order-resume-open").dataset.id = id;
    el("order-resume").hidden = false;
  } catch {
    /* An order from another browser isn't offered. */
  }
}).then(() => {
  if (state.catalog) route();
  else el("catalog").replaceChildren(emptyState("alert", "Unavailable"));
});
