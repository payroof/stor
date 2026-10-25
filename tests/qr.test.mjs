import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { qr } from "../web/qr.js";

// Independently compared against Nayuki QR Code generator, version 10-L,
// byte mode, mask 0, no error-correction boosting. No network needed to run.
test("browser QR matches independent reference matrices", () => {
  for (const [url, expected] of [
    [
      "https://api.example/v1/pairings/pair_" + "a".repeat(43),
      "6dbe45c6d1c58b05fafb8fbd67067f3acf622581a84fb523a5f4e4257472dd9a",
    ],
    [
      "x".repeat(271),
      "db7fc11593ca852a168d907c322ecaef377bb96213d17098856e31961136bc63",
    ],
    [
      "hello",
      "7d101f5eb80633c5e2decd2bfe422d083952dbfe85039e0e4cbace11eea4c6b2",
    ],
  ]) {
    const matrix = qr(url);
    assert.equal(matrix.length, 57);
    assert.ok(
      matrix.every(
        (row) => row.length === 57 && row.every((v) => typeof v === "boolean"),
      ),
    );
    assert.equal(
      createHash("sha256")
        .update(matrix.flat().map(Number).join(""))
        .digest("hex"),
      expected,
    );
  }
});
test("QR byte capacity is enforced including UTF-8", () => {
  assert.throws(() => qr("x".repeat(272)), /too long/);
  assert.throws(() => qr("é".repeat(136)), /too long/);
  assert.equal(qr("é".repeat(135)).length, 57);
});
