// RFC 9605 sections 4.3–4.5. ADR-0218 selects AES_128_GCM_SHA256_128.
const MAX_UINT64 = (1n << 64n) - 1n;
const REPLAY_WINDOW = 128n;
const REPLAY_MASK = (1n << REPLAY_WINDOW) - 1n;
const TAG_BYTES = 16;
type Bytes = Uint8Array<ArrayBuffer>;

function join(a: Uint8Array, b: Uint8Array): Bytes {
  const bytes = new Uint8Array(a.length + b.length);
  bytes.set(a);
  bytes.set(b, a.length);
  return bytes;
}

function uint64(value: bigint): Bytes {
  if (value < 0n || value > MAX_UINT64) throw new Error("SFrame integer out of range");
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, value);
  return bytes;
}

function compact(value: bigint): Bytes {
  if (value >= 0n && value < 8n) return new Uint8Array(0);
  const bytes = uint64(value);
  return bytes.subarray(bytes.findIndex((byte) => byte !== 0));
}

export function encodeHeader(kid: bigint, counter: bigint): Bytes {
  const keyBytes = compact(kid);
  const counterBytes = compact(counter);
  const keyField = keyBytes.length === 0 ? Number(kid) : 8 + keyBytes.length - 1;
  const counterField = counterBytes.length === 0 ? Number(counter) : 8 + counterBytes.length - 1;
  return join(Uint8Array.of((keyField << 4) | counterField), join(keyBytes, counterBytes));
}

export function decodeHeader(frame: Uint8Array): { kid: bigint; counter: bigint; length: number } {
  const config = frame[0];
  if (config === undefined) throw new Error("Missing SFrame header");
  let length = 1;
  function read(field: number): bigint {
    if (field < 8) return BigInt(field);
    const width = field - 7;
    if (frame.length < length + width || frame[length] === 0) throw new Error("Invalid SFrame integer");
    let value = 0n;
    for (const byte of frame.subarray(length, length + width)) value = (value << 8n) | BigInt(byte);
    if (value < 8n) throw new Error("Noncanonical SFrame integer");
    length += width;
    return value;
  }
  const kid = read(config >> 4);
  const counter = read(config & 15);
  return { kid, counter, length };
}

interface Key {
  readonly kid: bigint;
  readonly aes: CryptoKey;
  readonly salt: Bytes;
}

/** Consumes the writable bytes supplied by Core. The derived AES key is non-extractable. */
async function importKey(kid: bigint, base: Bytes, usage: "encrypt" | "decrypt"): Promise<Key> {
  let secret: CryptoKey;
  try {
    secret = await crypto.subtle.importKey("raw", base, "HKDF", false, ["deriveKey", "deriveBits"]);
  } finally {
    base.fill(0);
  }
  const suffix = join(uint64(kid), Uint8Array.of(0, 4));
  const params = (field: "key" | "salt"): HkdfParams => ({
    name: "HKDF",
    hash: "SHA-256",
    salt: new Uint8Array(0),
    info: join(new TextEncoder().encode(`SFrame 1.0 Secret ${field} `), suffix),
  });
  const aes = await crypto.subtle.deriveKey(params("key"), secret, { name: "AES-GCM", length: 128 }, false, [usage]);
  const salt = new Uint8Array(await crypto.subtle.deriveBits(params("salt"), secret, 96));
  return { kid, aes, salt };
}

function aead(key: Key, counter: bigint, header: Uint8Array, metadata: Uint8Array): AesGcmParams {
  const iv = key.salt.slice();
  const bytes = uint64(counter);
  for (let i = 0; i < bytes.length; i++) iv[i + 4] = iv[i + 4]! ^ bytes[i]!;
  return { name: "AES-GCM", iv, additionalData: join(header, metadata), tagLength: 128 };
}

/** Share one sender across all local tracks. A counter is consumed before any asynchronous encryption. */
export class SFrameSender {
  #counter = 0n;
  #kid = -1n;
  #key: Key | undefined;
  #closed = false;

  async install(kid: bigint, base: Bytes): Promise<void> {
    if (this.#closed || kid <= this.#kid || kid > MAX_UINT64) {
      base.fill(0);
      throw new Error("Invalid SFrame sending epoch");
    }
    this.#kid = kid;
    this.#key?.salt.fill(0);
    this.#key = undefined;
    const key = await importKey(kid, base, "encrypt");
    if (this.#closed || this.#kid !== kid) {
      key.salt.fill(0);
      return;
    }
    this.#key = key;
    this.#counter = 0n;
  }

  async seal(frame: Bytes, metadata = new Uint8Array(0), counter = this.#counter): Promise<Bytes | undefined> {
    const key = this.#key;
    if (key === undefined) return;
    if (counter < this.#counter || counter > MAX_UINT64) throw new Error("SFrame counter reuse or exhaustion");
    this.#counter = counter + 1n;
    const header = encodeHeader(key.kid, counter);
    const cipher = await crypto.subtle.encrypt(aead(key, counter, header, metadata), key.aes, frame);
    if (key === this.#key) return join(header, new Uint8Array(cipher));
  }

  close(): void {
    this.#closed = true;
    this.#key?.salt.fill(0);
    this.#key = undefined;
  }
}

interface ReceiveKey {
  readonly key: Key;
  latest: bigint;
  window: bigint;
}

function seen(epoch: ReceiveKey, counter: bigint): boolean {
  const age = epoch.latest - counter;
  return age >= REPLAY_WINDOW || (age >= 0n && (epoch.window & (1n << age)) !== 0n);
}

function remember(epoch: ReceiveKey, counter: bigint): void {
  if (counter > epoch.latest) {
    const ahead = counter - epoch.latest;
    epoch.window = ahead >= REPLAY_WINDOW ? 0n : (epoch.window << ahead) & REPLAY_MASK;
    epoch.latest = counter;
  }
  epoch.window |= 1n << (epoch.latest - counter);
}

export class SFrameReceiver {
  #kid = -1n;
  #current: ReceiveKey | undefined;
  #previous: ReceiveKey | undefined;
  #expiry: ReturnType<typeof setTimeout> | undefined;
  #deadline = 0;
  #closed = false;

  /** Only authenticated key delivery may install an epoch. The overlap is local and defaults to zero. */
  async install(kid: bigint, base: Bytes, overlapMs = 0): Promise<void> {
    if (this.#closed || kid <= this.#kid || kid > MAX_UINT64 ||
        !Number.isSafeInteger(overlapMs) || overlapMs < 0 || overlapMs > 3000) {
      base.fill(0);
      throw new Error("Invalid SFrame receiving epoch");
    }
    this.#kid = kid;
    const key = await importKey(kid, base, "decrypt");
    if (this.#closed || this.#kid !== kid) {
      key.salt.fill(0);
      return;
    }
    this.#forgetPrevious();
    if (this.#current && overlapMs > 0) {
      this.#previous = this.#current;
      this.#deadline = performance.now() + overlapMs;
      this.#expiry = setTimeout(() => this.#forgetPrevious(), overlapMs);
    } else {
      this.#current?.key.salt.fill(0);
    }
    this.#current = { key, latest: -1n, window: 0n };
  }

  #forgetPrevious(): void {
    clearTimeout(this.#expiry);
    this.#previous?.key.salt.fill(0);
    this.#previous = undefined;
  }

  #find(kid: bigint): ReceiveKey | undefined {
    if (this.#current?.key.kid === kid) return this.#current;
    if (this.#previous && performance.now() >= this.#deadline) this.#forgetPrevious();
    if (this.#previous?.key.kid === kid) return this.#previous;
  }

  async open(frame: Bytes, metadata = new Uint8Array(0)): Promise<ArrayBuffer | undefined> {
    try {
      const { kid, counter, length } = decodeHeader(frame);
      const epoch = this.#find(kid);
      if (!epoch || seen(epoch, counter) || frame.length < length + TAG_BYTES) return;
      const { key } = epoch;
      const plain = await crypto.subtle.decrypt(
        aead(key, counter, frame.subarray(0, length), metadata), key.aes, frame.subarray(length),
      );
      // WebCrypto may complete out of order across tracks. Recheck before exposing plaintext.
      if (this.#find(kid) !== epoch || seen(epoch, counter)) return;
      remember(epoch, counter);
      return plain;
    } catch {
      return; // Invalid headers and authentication failures never reach a decoder.
    }
  }

  close(): void {
    this.#closed = true;
    this.#forgetPrevious();
    this.#current?.key.salt.fill(0);
    this.#current = undefined;
  }
}
