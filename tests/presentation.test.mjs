import test from "node:test";
import assert from "node:assert/strict";
import {
  emptyState,
  help,
  initializeUI,
  roofPath,
  setIconLabel,
} from "../web/ui.js";

// A deliberately small DOM double: these tests exercise presentation behavior
// without starting a browser, installing dependencies or rendering screenshots.
class Element {
  constructor(tag, namespaceURI) {
    this.tag = tag;
    this.namespaceURI = namespaceURI;
    this.children = [];
    this.attributes = new Map();
    this.listeners = new Map();
    this.hidden = false;
    this.dataset = {};
    this.textContent = "";
    this.classList = {
      add: (...names) => {
        this.className = [this.className || "", ...names].join(" ").trim();
      },
    };
  }
  setAttribute(key, value) {
    this.attributes.set(key, String(value));
  }
  getAttribute(key) {
    return this.attributes.get(key) ?? null;
  }
  hasAttribute(key) {
    return this.attributes.has(key);
  }
  append(...values) {
    this.children.push(...values);
  }
  prepend(...values) {
    this.children.unshift(...values);
  }
  replaceChildren(...values) {
    this.children = values;
  }
  replaceWith(value) {
    this.replacement = value;
  }
  remove() {
    this.removed = true;
  }
  querySelector(selector) {
    const tags = selector.split(",").map((tag) => tag.trim());
    for (const child of this.children) {
      if (!(child instanceof Element)) continue;
      if (tags.includes(child.tag)) return child;
      const found = child.querySelector(selector);
      if (found) return found;
    }
    return null;
  }
  get firstElementChild() {
    return this.children[0] ?? null;
  }
  get tagName() {
    return this.tag.toUpperCase();
  }
  addEventListener(event, handler) {
    this.listeners.set(event, handler);
  }
  dispatch(event, value = {}) {
    this.listeners.get(event)?.(value);
  }
  focus() {
    this.focused = true;
  }
  querySelectorAll(selector) {
    assert.equal(selector, "[id]");
    return this.children.filter(
      (value) => value.id || value.hasAttribute("id"),
    );
  }
}
function withDOM(work, { marked = [], controls = [], storage } = {}) {
  const previousDocument = globalThis.document,
    previousStorage = globalThis.localStorage;
  const header = new Element("div");
  globalThis.document = {
    createElement: (tag) => new Element(tag),
    createElementNS: (namespace, tag) => new Element(tag, namespace),
    documentElement: { dataset: {} },
    addEventListener() {},
    querySelectorAll: (selector) =>
      selector === "[data-help]"
        ? marked
        : selector === "[data-icon]"
          ? controls
          : [],
    querySelector: () => header,
  };
  globalThis.localStorage = storage;
  try {
    work({ header, document: globalThis.document });
  } finally {
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
    if (previousStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = previousStorage;
  }
}

test("help opens by activation and closes by Escape with focus restored", () =>
  withDOM(() => {
    const control = help("Private explanatory content", "Delivery help");
    const [button, content] = control.children;
    assert.equal(button.type, "button");
    assert.equal(button.getAttribute("aria-label"), "Delivery help");
    assert.equal(button.getAttribute("aria-controls"), content.id);
    assert.equal(content.hidden, true);
    button.dispatch("click");
    assert.equal(content.hidden, false);
    assert.equal(button.getAttribute("aria-expanded"), "true");
    let stopped = false;
    control.dispatch("keydown", {
      key: "Escape",
      preventDefault() {},
      stopPropagation() {
        stopped = true;
      },
    });
    assert.equal(content.hidden, true);
    assert.equal(button.getAttribute("aria-expanded"), "false");
    assert.equal(button.focused, true);
    assert.equal(stopped, true);
  }));

test("initialization preserves bag counters and themes work without storage", () => {
  const bag = new Element("button");
  bag.dataset.icon = "bag";
  bag.textContent = "Bag 3";
  bag.setAttribute("aria-label", "Open bag");
  bag.setAttribute("data-icon-only", "");
  const count = new Element("span");
  count.id = "cart-count";
  count.textContent = "3";
  bag.append(count);
  const copy = new Element("p");
  copy.textContent = "Help text";
  copy.dataset.help = "About";
  const storage = {
    getItem() {
      throw new Error("unavailable");
    },
    setItem() {
      throw new Error("unavailable");
    },
  };
  withDOM(
    ({ header, document }) => {
      initializeUI();
      assert.equal(bag.children[0].tag, "svg");
      assert.equal(bag.children[1], count);
      assert.equal(bag.getAttribute("aria-label"), "Open bag");
      assert.equal(copy.replacement.children[2].textContent, "Help text");
      assert.equal(copy.replacement.children[0].textContent, "About");
      const theme = header.children[0];
      assert.equal(document.documentElement.dataset.theme, "light");
      theme.dispatch("click");
      assert.equal(document.documentElement.dataset.theme, "dark");
      theme.dispatch("click");
      assert.equal(document.documentElement.dataset.theme, "light");
      assert.match(theme.getAttribute("aria-label"), /Switch to Dark/);
    },
    { controls: [bag], marked: [copy], storage },
  );
});

test("icon-only action labels stay text-free with an accessible name", () =>
  withDOM(() => {
    const button = new Element("button");
    setIconLabel(button, "Reveal store key", "eye");
    assert.deepEqual(
      button.children.map((child) => child.tag),
      ["svg"],
    );
    assert.equal(button.getAttribute("aria-label"), "Reveal store key");
    assert.equal(button.title, "Reveal store key");
    setIconLabel(button, "Hide store key", "eye-off");
    assert.deepEqual(
      button.children.map((child) => child.tag),
      ["svg"],
    );
    assert.equal(button.getAttribute("aria-label"), "Hide store key");
    assert.equal(
      button.children[0].children[0].getAttribute("d"),
      "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Zm13 0a3 3 0 1 1-6 0 3 3 0 0 1 6 0M3 3l18 18",
    );
  }));

const label = new Element("label");
const title = new Element("span");
title.textContent = "Store address";
const input = new Element("input");
input.id = "slug";
label.append(title, input);
const helpText = new Element("p");
helpText.textContent = "Use lowercase letters.";
helpText.dataset.help = "Store address";
helpText.previousElementSibling = label;

test("help for a labelled input joins the label text, not the tall input", () =>
  withDOM(
    () => {
      initializeUI();
      assert.equal(label.children.length, 2);
      assert.equal(label.children[0], title);
      const control = title.children.at(-1);
      assert.equal(control.className, "help-control");
      assert.equal(control.children[0].tag, "button");
      assert.equal(control.children[1].textContent, "Use lowercase letters.");
      assert.equal(helpText.removed, true);
      // The label stays tied to its field, not to the help button now inside it.
      assert.equal(label.htmlFor, "slug");
    },
    { marked: [helpText] },
  ));

test("help supports mouse and focus dismissal and remains an overlay", () =>
  withDOM(() => {
    const control = help("Contextual detail", "Product information");
    control.contains = (value) => control.children.includes(value);
    const [button, content] = control.children;
    assert.equal(content.getAttribute("popover"), "manual");
    assert.equal(content.getAttribute("role"), "tooltip");
    assert.equal(button.getAttribute("aria-describedby"), content.id);
    control.dispatch("pointerenter", { pointerType: "touch" });
    assert.equal(content.hidden, true);
    control.dispatch("pointerenter", { pointerType: "mouse" });
    assert.equal(content.hidden, false);
    control.dispatch("focusout", { relatedTarget: null });
    assert.equal(content.hidden, true);
    button.dispatch("click");
    control.dispatch("pointerleave", { pointerType: "mouse" });
    assert.equal(content.hidden, true);
  }));

test("empty lists share one layout: icon badge, title, optional action", () =>
  withDOM(() => {
    const button = new Element("button");
    const value = emptyState("bag", "Your bag is empty", button);
    assert.equal(value.className, "empty");
    const [badge, title, action] = value.children;
    assert.equal(badge.className, "empty-icon");
    assert.equal(badge.children[0].tag, "svg");
    assert.equal(title.tag, "h3");
    assert.equal(title.textContent, "Your bag is empty");
    assert.equal(action, button);
    assert.equal(emptyState("box", "No products").children.length, 2);
  }));

test("the roofline is a closed gable that stays inside its panel", () => {
  for (const [width, height] of [
    [601, 241],
    [320, 180],
    [40, 40],
  ]) {
    const path = roofPath(width, height);
    assert.match(path, /^M0,\d/);
    assert.match(path, / Z$/);
    const numbers = path.match(/-?\d+(\.\d+)?/g).map(Number);
    const xs = numbers.filter((_, index) => index % 2 === 0);
    const ys = numbers.filter((_, index) => index % 2 === 1);
    assert.ok(Math.min(...xs) >= 0 && Math.max(...xs) <= width - 1);
    assert.ok(Math.min(...ys) >= 0 && Math.max(...ys) <= height - 1);
    if (width > 200) {
      // On a panel the ridge peaks at the top. The side starts below the rounded eave.
      assert.ok(Math.min(...ys) < 1);
      assert.equal(numbers[1], 16 + 18);
    }
  }
});
