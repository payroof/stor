function hexToBytes(hex) {
  if (hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) {
    throw new Error("invalid hex string");
  }
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

function bytesToHex(bytes) {
  let hex = "";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

function counterBytes(counter) {
  const buffer = new Uint8Array(4);
  buffer[0] = (counter >>> 24) & 0xff;
  buffer[1] = (counter >>> 16) & 0xff;
  buffer[2] = (counter >>> 8) & 0xff;
  buffer[3] = counter & 0xff;
  return buffer;
}

// SHA-256 and PBKDF2 for pages without Web Crypto: plain HTTP from another device (a
// phone on the local network) is not a secure context, so crypto.subtle is missing there.
// The HMAC pad states are computed once, so each PBKDF2 iteration is two compressions.
const ROUND = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
const INITIAL = new Uint32Array([
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);
const schedule = new Uint32Array(64);
/** One SHA-256 compression of a 16-word block from `state`, written to `out`. */
function compress(state, block, out) {
  const w = schedule;
  for (let i = 0; i < 16; i++) w[i] = block[i];
  for (let i = 16; i < 64; i++) {
    const x = w[i - 15],
      y = w[i - 2];
    const s0 = ((x >>> 7) | (x << 25)) ^ ((x >>> 18) | (x << 14)) ^ (x >>> 3);
    const s1 = ((y >>> 17) | (y << 15)) ^ ((y >>> 19) | (y << 13)) ^ (y >>> 10);
    w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
  }
  let a = state[0],
    b = state[1],
    c = state[2],
    d = state[3],
    e = state[4],
    f = state[5],
    g = state[6],
    h = state[7];
  for (let i = 0; i < 64; i++) {
    const s1 = ((e >>> 6) | (e << 26)) ^ ((e >>> 11) | (e << 21)) ^ ((e >>> 25) | (e << 7));
    const t1 = (h + s1 + ((e & f) ^ (~e & g)) + ROUND[i] + w[i]) | 0;
    const s0 = ((a >>> 2) | (a << 30)) ^ ((a >>> 13) | (a << 19)) ^ ((a >>> 22) | (a << 10));
    const t2 = (s0 + ((a & b) ^ (a & c) ^ (b & c))) | 0;
    h = g;
    g = f;
    f = e;
    e = (d + t1) | 0;
    d = c;
    c = b;
    b = a;
    a = (t1 + t2) | 0;
  }
  out[0] = (state[0] + a) | 0;
  out[1] = (state[1] + b) | 0;
  out[2] = (state[2] + c) | 0;
  out[3] = (state[3] + d) | 0;
  out[4] = (state[4] + e) | 0;
  out[5] = (state[5] + f) | 0;
  out[6] = (state[6] + g) | 0;
  out[7] = (state[7] + h) | 0;
}
/** SHA-256 of `bytes`, continuing from `state` after `before` bytes were hashed. */
function sha256(bytes, state = INITIAL, before = 0) {
  const total = before + bytes.length;
  const padded = new Uint8Array(Math.ceil((bytes.length + 9) / 64) * 64);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor((total * 8) / 2 ** 32));
  view.setUint32(padded.length - 4, (total * 8) >>> 0);
  const out = Uint32Array.from(state);
  const block = new Uint32Array(16);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i++) block[i] = view.getUint32(offset + i * 4);
    compress(out, block, out);
  }
  return out;
}
function wordsToBytes(words, length = words.length * 4) {
  const bytes = new Uint8Array(words.length * 4);
  const view = new DataView(bytes.buffer);
  words.forEach((word, i) => view.setUint32(i * 4, word));
  return bytes.slice(0, length);
}
/** PBKDF2-HMAC-SHA-256, first block only (up to 32 bytes). Exported for tests. */
export function pbkdf2(password, salt, iterations, length) {
  if (length > 32) throw new Error("unsupported key length");
  const key = password.length > 64 ? wordsToBytes(sha256(password)) : password;
  const pad = (value) => {
    const bytes = new Uint8Array(64).fill(value);
    key.forEach((byte, i) => (bytes[i] ^= byte));
    const view = new DataView(bytes.buffer);
    return Uint32Array.from({ length: 16 }, (_, i) => view.getUint32(i * 4));
  };
  const inner = new Uint32Array(8),
    outer = new Uint32Array(8);
  compress(INITIAL, pad(0x36), inner);
  compress(INITIAL, pad(0x5c), outer);
  const first = new Uint8Array(salt.length + 4);
  first.set(salt);
  first[salt.length + 3] = 1;
  // A 32-byte message after the 64-byte pad: one block, its padding fixed.
  const block = new Uint32Array(16);
  block[8] = 0x80000000;
  block[15] = (64 + 32) * 8;
  const u = new Uint32Array(8);
  block.set(sha256(first, inner, 64));
  compress(outer, block, u);
  const result = Uint32Array.from(u);
  for (let i = 1; i < iterations; i++) {
    block.set(u);
    compress(inner, block, u);
    block.set(u);
    compress(outer, block, u);
    for (let j = 0; j < 8; j++) result[j] ^= u[j];
  }
  return wordsToBytes(result, length);
}

function password(parameters, counter) {
  const nonce = hexToBytes(parameters.nonce);
  const extra = counterBytes(counter);
  const value = new Uint8Array(nonce.length + extra.length);
  value.set(nonce, 0);
  value.set(extra, nonce.length);
  return value;
}

async function deriveKey(parameters, counter) {
  const subtle = globalThis.crypto?.subtle;
  const salt = hexToBytes(parameters.salt);
  if (!subtle) return pbkdf2(password(parameters, counter), salt, parameters.cost, parameters.keyLength);
  const key = await subtle.importKey("raw", password(parameters, counter), "PBKDF2", false, [
    "deriveBits",
  ]);
  const bits = await subtle.deriveBits(
    {
      name: "PBKDF2",
      salt,
      iterations: parameters.cost,
      hash: "SHA-256",
    },
    key,
    parameters.keyLength * 8,
  );
  return new Uint8Array(bits);
}

// Without Web Crypto the script-only PBKDF2 is several times slower, so the counters are
// split across one worker per core (this same module, started as a worker).
if (typeof WorkerGlobalScope !== "undefined" && globalThis instanceof WorkerGlobalScope) {
  globalThis.onmessage = ({ data: { parameters, prefix, start, step } }) => {
    const salt = hexToBytes(parameters.salt);
    for (let counter = start; counter <= 0xffffffff; counter += step) {
      const hex = bytesToHex(
        pbkdf2(password(parameters, counter), salt, parameters.cost, parameters.keyLength),
      );
      if (hex.startsWith(prefix)) {
        globalThis.postMessage({ counter, derivedKey: hex });
        return;
      }
    }
  };
}
function solveInWorkers(parameters, prefix, timeoutMs) {
  const count = Math.min(Math.max(globalThis.navigator?.hardwareConcurrency || 2, 1), 8);
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const workers = [];
    let timer;
    const finish = (settle, value) => {
      clearTimeout(timer);
      workers.forEach((worker) => worker.terminate());
      settle(value);
    };
    if (timeoutMs)
      timer = setTimeout(() => finish(reject, new Error("ALTCHA challenge timed out")), timeoutMs);
    for (let start = 0; start < count; start++) {
      const worker = new Worker(import.meta.url, { type: "module" });
      worker.onmessage = ({ data }) =>
        finish(resolve, { ...data, time: Date.now() - started });
      worker.onerror = () => finish(reject, new Error("proof unavailable"));
      worker.postMessage({ parameters, prefix, start, step: count });
      workers.push(worker);
    }
  });
}

/** Solves an ALTCHA PBKDF2/SHA-256 challenge. Exported for tests. */
export async function solveChallenge(challenge, timeoutMs = 30000) {
  const parameters = challenge && challenge.parameters;
  if (!parameters || parameters.algorithm !== "PBKDF2/SHA-256") {
    throw new Error("unsupported ALTCHA challenge");
  }
  if (parameters.cost !== 5000 || parameters.keyLength > 32)
    throw new Error("unsupported proof cost");
  const prefix = String(parameters.keyPrefix || "").toLowerCase();
  if (!/^[0-9a-f]+$/.test(prefix)) throw new Error("invalid ALTCHA key prefix");
  if (!globalThis.crypto?.subtle && typeof Worker !== "undefined")
    return solveInWorkers(parameters, prefix, timeoutMs);
  const started = Date.now();
  for (let counter = 0; ; counter = (counter + 1) >>> 0) {
    const derived = await deriveKey(parameters, counter);
    const hex = bytesToHex(derived);
    if (hex.startsWith(prefix)) {
      return {
        counter,
        derivedKey: hex,
        time: Date.now() - started,
      };
    }
    if (timeoutMs && Date.now() - started > timeoutMs) {
      throw new Error("ALTCHA challenge timed out");
    }
  }
}

export async function proof(url) {
  const r = await fetch(url, { cache: "no-store" });
  if (!r.ok) throw new Error("proof unavailable");
  const challenge = await r.json();
  return { challenge, solution: await solveChallenge(challenge) };
}
