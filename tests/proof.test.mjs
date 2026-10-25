import { test } from "node:test";
import assert from "node:assert/strict";
import { pbkdf2Sync } from "node:crypto";
import { pbkdf2, solveChallenge } from "../web/proof.js";
test("browser proof matches independent PBKDF2", async () => {
  const p = {
    algorithm: "PBKDF2/SHA-256",
    nonce: "00112233445566778899aabbccddeeff",
    salt: "ffeeddccbbaa99887766554433221100",
    cost: 5000,
    keyLength: 32,
    keyPrefix: "00",
  };
  const result = await solveChallenge({ parameters: p });
  const counter = Buffer.alloc(4);
  counter.writeUInt32BE(result.counter);
  const key = pbkdf2Sync(
    Buffer.concat([Buffer.from(p.nonce, "hex"), counter]),
    Buffer.from(p.salt, "hex"),
    p.cost,
    p.keyLength,
    "sha256",
  ).toString("hex");
  assert.equal(result.derivedKey, key);
  assert.ok(key.startsWith(p.keyPrefix));
});
test("invalid proof algorithm and cost fail closed", async () => {
  await assert.rejects(() =>
    solveChallenge({ parameters: { algorithm: "bad" } }),
  );
  await assert.rejects(() =>
    solveChallenge({
      parameters: {
        algorithm: "PBKDF2/SHA-256",
        cost: 999999999,
        keyLength: 32,
      },
    }),
  );
});
test("the fallback PBKDF2 matches node for short and long passwords", () => {
  for (const [password, salt, iterations, length] of [
    [Buffer.from("00112233445566778899aabbccddeeff00000007", "hex"), Buffer.from("salt"), 5000, 32],
    [Buffer.alloc(100, 7), Buffer.alloc(70, 3), 3, 20],
    [Buffer.from("p"), Buffer.alloc(0), 1, 32],
  ])
    assert.equal(
      Buffer.from(pbkdf2(new Uint8Array(password), new Uint8Array(salt), iterations, length)).toString("hex"),
      pbkdf2Sync(password, salt, iterations, length, "sha256").toString("hex"),
    );
});
test("a page without Web Crypto still solves the proof", async () => {
  const crypto = Object.getOwnPropertyDescriptor(globalThis, "crypto");
  Object.defineProperty(globalThis, "crypto", { value: {}, configurable: true });
  try {
    const p = {
      algorithm: "PBKDF2/SHA-256",
      nonce: "00112233445566778899aabbccddeeff",
      salt: "ffeeddccbbaa99887766554433221100",
      cost: 5000,
      keyLength: 32,
      keyPrefix: "00",
    };
    const result = await solveChallenge({ parameters: p });
    const counter = Buffer.alloc(4);
    counter.writeUInt32BE(result.counter);
    const key = pbkdf2Sync(
      Buffer.concat([Buffer.from(p.nonce, "hex"), counter]),
      Buffer.from(p.salt, "hex"),
      p.cost,
      p.keyLength,
      "sha256",
    ).toString("hex");
    assert.equal(result.derivedKey, key);
  } finally {
    Object.defineProperty(globalThis, "crypto", crypto);
  }
});
