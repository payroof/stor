// QR Model 2, version 10-L, byte mode, mask 0. Encodes locally: no URL leaves the browser.
export function qr(url) {
  const bytes = new TextEncoder().encode(url);
  if (bytes.length > 271)
    throw new Error("Pairing URL is too long for QR. Use the link instead.");
  const bits = [];
  const push = (value, count) => {
    for (let i = count - 1; i >= 0; i--) bits.push((value >>> i) & 1);
  };
  push(4, 4);
  push(bytes.length, 16);
  bytes.forEach((b) => push(b, 8));
  push(0, Math.min(4, 2192 - bits.length));
  while (bits.length % 8) bits.push(0);
  const data = [];
  for (let i = 0; i < bits.length; i += 8)
    data.push(bits.slice(i, i + 8).reduce((n, b) => n * 2 + b, 0));
  for (let pad = 0; data.length < 274; pad++) data.push(pad % 2 ? 0x11 : 0xec);
  const mul = (a, b) => {
    let n = 0;
    for (; b; b >>>= 1) {
      if (b & 1) n ^= a;
      a <<= 1;
      if (a & 256) a ^= 0x11d;
    }
    return n;
  };
  let generator = [1],
    root = 1;
  for (let i = 0; i < 18; i++, root = mul(root, 2)) {
    const next = Array(generator.length + 1).fill(0);
    generator.forEach((v, j) => {
      next[j] ^= v;
      next[j + 1] ^= mul(v, root);
    });
    generator = next;
  }
  const blocks = [],
    parity = [];
  let offset = 0;
  for (const length of [68, 68, 69, 69]) {
    const block = data.slice(offset, (offset += length)),
      remainder = Array(18).fill(0);
    for (const b of block) {
      const factor = b ^ remainder.shift();
      remainder.push(0);
      for (let j = 0; j < 18; j++)
        remainder[j] ^= mul(generator[j + 1], factor);
    }
    blocks.push(block);
    parity.push(remainder);
  }
  const stream = [];
  for (let i = 0; i < 69; i++)
    for (const b of blocks) if (i < b.length) stream.push(b[i]);
  for (let i = 0; i < 18; i++) for (const b of parity) stream.push(b[i]);
  const n = 57,
    matrix = Array.from({ length: n }, () => Array(n).fill(false)),
    reserved = matrix.map((r) => r.slice());
  const set = (x, y, dark) => {
    matrix[y][x] = !!dark;
    reserved[y][x] = true;
  };
  for (const [cx, cy] of [
    [3, 3],
    [53, 3],
    [3, 53],
  ])
    for (let dy = -4; dy <= 4; dy++)
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx,
          y = cy + dy,
          d = Math.max(Math.abs(dx), Math.abs(dy));
        if (x >= 0 && y >= 0 && x < n && y < n) set(x, y, d !== 2 && d !== 4);
      }
  for (let i = 8; i < n - 8; i++) {
    set(i, 6, i % 2 === 0);
    set(6, i, i % 2 === 0);
  }
  for (const cx of [6, 28, 50])
    for (const cy of [6, 28, 50]) {
      if ((cx === 6 && (cy === 6 || cy === 50)) || (cx === 50 && cy === 6))
        continue;
      for (let dy = -2; dy <= 2; dy++)
        for (let dx = -2; dx <= 2; dx++)
          set(cx + dx, cy + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
    }
  const format = 0x77c4,
    bit = (v, i) => (v >>> i) & 1;
  for (let i = 0; i <= 5; i++) set(8, i, bit(format, i));
  set(8, 7, bit(format, 6));
  set(8, 8, bit(format, 7));
  set(7, 8, bit(format, 8));
  for (let i = 9; i < 15; i++) set(14 - i, 8, bit(format, i));
  for (let i = 0; i < 8; i++) set(n - 1 - i, 8, bit(format, i));
  for (let i = 8; i < 15; i++) set(8, n - 15 + i, bit(format, i));
  set(8, n - 8, true);
  let rem = 10;
  for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
  const version = (10 << 12) | rem;
  for (let i = 0; i < 18; i++) {
    const a = n - 11 + (i % 3),
      b = Math.floor(i / 3);
    set(a, b, bit(version, i));
    set(b, a, bit(version, i));
  }
  let index = 0;
  for (let right = n - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vertical = 0; vertical < n; vertical++) {
      const y = ((right + 1) & 2) === 0 ? n - 1 - vertical : vertical;
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        if (!reserved[y][x]) {
          matrix[y][x] = !!(
            bit(stream[index >>> 3] || 0, 7 - (index & 7)) ^
            ((x + y) % 2 === 0)
          );
          index++;
        }
      }
    }
  }
  return matrix;
}
