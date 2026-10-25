import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  formatAtomic,
  price,
  assetInfo,
  safeURL,
  orderAssets,
  deliveryGroups,
  canonicalOrders,
  paymentGroups,
  paymentOffers,
  cheapest,
  defaultAsset,
  networkTone,
  tokenTone,
  orderTone,
  countdown,
  progressPercent,
  resolveTheme,
} from "../web/ui.js";

test("prices retain all digits and known decimal precision", () => {
  assert.equal(formatAtomic("1000000000000000001", 18), "1.000000000000000001");
  assert.equal(formatAtomic("5000", 6), "0.005");
  assert.equal(formatAtomic("0", 18), "0");
  const maximum = (2n ** 256n - 1n).toString();
  assert.equal(formatAtomic(maximum, 0), maximum);
  for (const value of ["-1", "+1", "1e6", "NaN", "1,2", "١", "0.1"])
    assert.throws(() => formatAtomic(value, 6), value);
});
test("unknown token decimals never become a guessed human price", () => {
  assert.equal(price({ asset: "unknown", amount: "5000" }), "5000 units");
  const token = "eip155:31337/erc20:0x5fbdb2315678afecb367f032d93f642f64180aa3";
  assert.equal(
    price({ asset: token, amount: "25000000" }),
    "25000000 units of 0x5fbd…0aa3",
  );
  assert.equal(assetInfo(token).name, "Token 0x5fbd…0aa3");
  assert.equal(assetInfo(token).network, "Local Anvil");
  // ETH (coin type 60) has 18 decimals on every EVM chain: proven, not guessed.
  assert.equal(
    price({ asset: "eip155:31337/slip44:60", amount: "30000000000000000" }),
    "0.03 ETH",
  );
  assert.equal(
    price({ asset: "eip155:31337/slip44:966", amount: "5" }),
    "5 units",
  );
  assert.equal(
    price({ asset: "token", amount: "5000" }, [
      { id: "token", decimals: 6, symbol: "TEST" },
    ]),
    "0.005 TEST",
  );
  assert.deepEqual(orderAssets({ asset: "token", asset_display: null }), []);
  assert.deepEqual(orderAssets({ asset: "token" }), []);
});
test("store links cannot execute scripts or carry credentials", () => {
  for (const value of [
    "javascript:alert(1)",
    "data:text/html,test",
    "https://user:secret@example.com",
    "invalid",
  ])
    assert.equal(safeURL(value), null);
  assert.equal(
    safeURL("http://shop.stor.localhost:8081"),
    "http://shop.stor.localhost:8081/",
  );
});
test("delivery is grouped by purchased item, unit by unit", () => {
  const guide = { title: "Guide", quantity: 2 };
  const course = { title: "Course", quantity: 1 };
  assert.deepEqual(
    deliveryGroups({ items: [guide, course], delivery: ["g1", "g2", "c1"] }),
    [
      { item: guide, units: ["g1", "g2"] },
      { item: course, units: ["c1"] },
    ],
  );
  assert.deepEqual(deliveryGroups({ delivery: ["unlabelled"] }), [
    { item: null, units: ["unlabelled"] },
  ]);
  assert.deepEqual(deliveryGroups({}), []);
  const one = { title: "One", quantity: 1 };
  assert.deepEqual(
    deliveryGroups({
      items: [{ title: "Skip", quantity: 0 }, one],
      delivery: ["one", "extra"],
    }),
    [
      { item: one, units: ["one"] },
      { item: null, units: ["extra"] },
    ],
  );
});
test("canonical browser orders are bounded, unique public identifiers only", () => {
  assert.deepEqual(canonicalOrders(null), []);
  assert.deepEqual(canonicalOrders("ord_one"), []);
  assert.deepEqual(
    canonicalOrders(["ord_one", "ord_one", "javascript:bad", null, "ord_two"]),
    ["ord_one", "ord_two"],
  );
  assert.equal(
    canonicalOrders(Array.from({ length: 30 }, (_, index) => `ord_${index}`))
      .length,
    20,
  );
});
const L1 = "eip155:31337";
const L2 = "eip155:8453";
const assets = [
  { id: `${L1}/erc20:0xa`, symbol: "USDC", decimals: 6, network: "Anvil" },
  { id: `${L2}/erc20:0xb`, symbol: "USDC", decimals: 6, network: "Base" },
  { id: `${L1}/slip44:60`, symbol: "ETH", decimals: 18, network: "Anvil" },
  { id: `${L2}/slip44:60`, symbol: "ETH", decimals: 18, network: "Base" },
];
test("the network name comes from the project view, with the built-in names as fallback", () => {
  assert.equal(assetInfo(`${L2}/slip44:60`, assets).network, "Base");
  assert.equal(assetInfo("eip155:999999/slip44:60").network, "EVM chain 999999");
});
test("one token on several networks is one offer unless its amounts differ", () => {
  const same = [
    { asset: `${L1}/erc20:0xa`, amount: "9000000" },
    { asset: `${L2}/erc20:0xb`, amount: "9000000" },
    { asset: `${L2}/slip44:60`, amount: "3000000000000000" },
  ];
  assert.equal(paymentGroups(same, assets).length, 2);
  assert.deepEqual(
    paymentOffers(same, assets).map(({ symbol, amount, networks }) => [symbol, amount, networks]),
    [
      ["USDC", "9", ["Anvil", "Base"]],
      ["ETH", "0.003", ["Base"]],
    ],
  );
  const differ = [
    { asset: `${L1}/erc20:0xa`, amount: "9000000" },
    { asset: `${L2}/erc20:0xb`, amount: "9500000" },
  ];
  assert.deepEqual(
    paymentOffers(differ, assets).map(({ amount, networks }) => [amount, networks]),
    [
      ["9", ["Anvil"]],
      ["9.5", ["Base"]],
    ],
  );
});
test("both page contracts contain every literal JS element reference exactly once", async () => {
  for (const page of ["create", "storefront"]) {
    const html = await readFile(
      new URL(`../web/${page}/index.html`, import.meta.url),
      "utf8",
    );
    const js = await readFile(
      new URL(`../web/${page}/app.js`, import.meta.url),
      "utf8",
    );
    const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map((value) => value[1]);
    assert.equal(new Set(ids).size, ids.length, `${page}: duplicate HTML IDs`);
    for (const match of html.matchAll(/\bpattern="([^"]+)"/g))
      assert.doesNotThrow(
        () => new RegExp(`^(?:${match[1]})$`, "v"),
        `${page}: invalid pattern attribute`,
      );
    for (const match of js.matchAll(/\bel\("([^"]+)"\)/g))
      assert.ok(ids.includes(match[1]), `${page}: missing #${match[1]}`);
    assert.doesNotMatch(
      js,
      /\.innerHTML\s*=/,
      `${page}: unsafe merchant-content rendering`,
    );
    assert.ok(html.includes('aria-live="polite"'));
  }
  const merchant = await readFile(
    new URL("../web/create/app.js", import.meta.url),
    "utf8",
  );
  // The merchant's page keeps nothing: there is no session, key or token to keep.
  assert.doesNotMatch(merchant, /(?:localStorage|sessionStorage|document\.cookie)/);
});

test("the picker preselects the first token on its cheapest network", () => {
  const options = [
    { asset: `${L1}/erc20:0xa`, amount: "9500000" },
    { asset: `${L2}/erc20:0xb`, amount: "9000000" },
    { asset: `${L2}/slip44:60`, amount: "3000000000000000" },
  ];
  const groups = paymentGroups(options, assets);
  assert.equal(defaultAsset(groups), `${L2}/erc20:0xb`);
  assert.equal(cheapest(groups[0].options).amount, "9");
  assert.equal(defaultAsset([]), "");
});
test("colour keys are stable and orders map to a status tone", () => {
  assert.equal(networkTone(`${L2}/slip44:60`), "base");
  assert.equal(networkTone("eip155:12/slip44:60"), "n0");
  assert.equal(networkTone("eip155:13/slip44:60"), networkTone("eip155:13/erc20:0x1"));
  assert.equal(tokenTone("usdc"), "usdc");
  assert.equal(tokenTone("XYZ"), tokenTone("XYZ"));
  assert.equal(orderTone("delivered"), "success");
  assert.equal(orderTone("partial"), "pending");
  assert.equal(orderTone("late_payment"), "warning");
  assert.equal(orderTone("failed"), "danger");
  assert.equal(orderTone("awaiting_payment", true), "danger");
});
test("the payment window counts down as m:ss and never goes negative", () => {
  assert.equal(countdown(0), "0:00");
  assert.equal(countdown(-5), "0:00");
  assert.equal(countdown(65), "1:05");
  assert.equal(countdown(3599), "59:59");
  assert.equal(countdown(3725), "1:02:05");
  assert.equal(countdown("x"), "0:00");
});
test("received progress is an exact share clamped to 0 to 100", () => {
  assert.equal(progressPercent("0", "1000"), 0);
  assert.equal(progressPercent("250", "1000"), 25);
  assert.equal(progressPercent("1", "3"), 33.3);
  assert.equal(progressPercent("2000", "1000"), 100);
  assert.equal(progressPercent("1", "0"), 0);
  assert.equal(progressPercent("bad", "10"), 0);
});
test("a merchant's light or dark wins, else the visitor's choice, else the system's", () => {
  assert.deepEqual(resolveTheme({ lock: "dark", saved: "light", system: false }), {
    mode: "dark",
    locked: true,
    chosen: true,
  });
  assert.equal(resolveTheme({ saved: "dark", system: false }).mode, "dark");
  assert.equal(resolveTheme({ saved: "dark" }).locked, false);
  assert.equal(resolveTheme({ system: true }).mode, "dark");
  assert.equal(resolveTheme({ system: false }).chosen, false);
  assert.equal(resolveTheme({ lock: "sepia", saved: "x" }).mode, "light");
  assert.equal(resolveTheme().mode, "light");
});
