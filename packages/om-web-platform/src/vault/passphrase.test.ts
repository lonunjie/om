// Method 3: passphrase layer over method 1/2 (DEVICE_VAULT_CLIENT_HOST_AND_MIGRATION.md §5 rule 3).
import { describe, expect, it } from "vitest";
import { argon2id } from "@noble/hashes/argon2.js";
import type { DeviceVault, PassphraseFactor, ProfileId } from "@om/platform-api";
import { asProfileId } from "@om/platform-api";
import { createEncryptedKv, createWebDeviceVault } from "./index";
import { argon2idRaw, normalizePassphrase } from "./crypto";
import { DEVICE_KEY_ID, STORE_DEVICE_KEY, STORE_WRAPPED, WRAPPED_ID, committed, dbName, openExisting, openOrCreate, request } from "./idb";
import { PRODUCTION_KDF_POLICY, createVaultWithKdfPolicy } from "./vault";
import type { KdfPolicy } from "./vault";
import { FakeAuthenticator, fakeEnv } from "./__fixtures__/fake-env";
import type { FakeEnv } from "./__fixtures__/fake-env";

const A = asProfileId("profile-a");
const enc = new TextEncoder();
const text = (b: Uint8Array): string => new TextDecoder().decode(b);
const hex = (b: Uint8Array): string => Array.from(b, (v) => v.toString(16).padStart(2, "0")).join("");
const pass = (s: string): PassphraseFactor => ({ kind: "passphrase", secret: enc.encode(s) });

/** Tiny costs so tests stay fast; production uses PRODUCTION_KDF_POLICY and rejects records below its floor. */
const TEST_POLICY: KdfPolicy = { floor: { m: 8, t: 1, p: 1 }, standard: { m: 64, t: 1, p: 1 }, max: { m: 1024, t: 4, p: 1 } };

function setup(o: Parameters<typeof fakeEnv>[0] = {}): { env: FakeEnv; vault: DeviceVault } {
  const env = fakeEnv(o);
  return { env, vault: createVaultWithKdfPolicy(env, TEST_POLICY) };
}

interface StoredRecord {
  format: number;
  ciphertext: Uint8Array;
  nonce: Uint8Array;
  generation: number;
  method: string;
  passphrase?: { kdf: number; m: number; t: number; p: number; salt: Uint8Array };
}

async function rawStore(env: FakeEnv, p: ProfileId, store: string, run: (s: IDBObjectStore) => IDBRequest): Promise<unknown> {
  const ex = await openExisting(env.indexedDB, dbName(p));
  if (ex.kind !== "open") throw new Error(ex.kind);
  try {
    const tx = ex.db.transaction(store, "readwrite");
    const done = committed(tx);
    const r: unknown = await request(run(tx.objectStore(store)));
    await done;
    return r;
  } finally {
    ex.db.close();
  }
}
const getRec = async (env: FakeEnv): Promise<StoredRecord> => await rawStore(env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID)) as StoredRecord;
const putRec = async (env: FakeEnv, r: unknown): Promise<void> => { await rawStore(env, A, STORE_WRAPPED, (s) => s.put(r, WRAPPED_ID)); };

async function sealOk(v: DeviceVault, s: string): Promise<Uint8Array> {
  const r = await v.seal(A, "local-db", enc.encode(s));
  if (!r.ok) throw new Error(r.reason);
  return r.sealed;
}
async function opened(v: DeviceVault, sealed: Uint8Array): Promise<string | undefined> {
  const r = await v.open(A, "local-db", sealed);
  return r.ok ? text(r.bytes) : undefined;
}

describe("passphrase layer: create and unlock", () => {
  for (const method of ["device-key", "webauthn-prf"] as const) {
    it(`round-trips over ${method}; missing and wrong passphrases are refused without side effects`, async () => {
      const auth = method === "webauthn-prf" ? new FakeAuthenticator({ prf: "ok", backupEligible: false }) : null;
      const { vault } = setup({ auth });
      const factor = pass("correct horse battery staple");
      const r = await vault.createProfileVault(A, { additionalFactor: factor });
      expect(r).toMatchObject({ ok: true, wrapping: { method, passphraseProtected: true } });
      expect(factor.secret.every((b) => b === 0)).toBe(true); // caller's buffer zeroed
      expect(await vault.wrapping(A)).toMatchObject({ ok: true, wrapping: { passphraseProtected: true } });
      const sealed = await sealOk(vault, "behind two layers");
      vault.lock(A);

      const ceremonies = auth?.getCalls ?? 0;
      expect(await vault.unlock(A)).toEqual({ ok: false, reason: "passphrase_required" });
      expect(await vault.unlock(A, { passphrase: pass("") })).toEqual({ ok: false, reason: "passphrase_required" });
      expect(auth?.getCalls ?? 0).toBe(ceremonies); // no passkey prompt spent on a missing passphrase
      const wrong = pass("correct horse battery stapler");
      expect(await vault.unlock(A, { passphrase: wrong })).toEqual({ ok: false, reason: "wrong_passphrase" });
      expect(wrong.secret.every((b) => b === 0)).toBe(true);
      expect(await vault.status(A)).toBe("locked");
      // No lockout or counter: a later correct passphrase still works (and nothing here is a rate limit).
      expect(await vault.unlock(A, { passphrase: pass("correct horse battery staple") })).toEqual({ ok: true });
      expect(await opened(vault, sealed)).toBe("behind two layers");
    });
  }

  it("normalises with NFKC: composed/decomposed accents, full-width digits and no-break spaces match", async () => {
    const { vault } = setup({ auth: null });
    expect((await vault.createProfileVault(A, { additionalFactor: pass("caf\u00E9 \uFF11\u00A0x") })).ok).toBe(true);
    vault.lock(A);
    expect(await vault.unlock(A, { passphrase: pass("cafe\u0301 1 x") })).toEqual({ ok: true });
    vault.lock(A);
    expect(await vault.unlock(A, { passphrase: pass("Cafe\u0301 1 x") })).toEqual({ ok: false, reason: "wrong_passphrase" }); // case is kept
  });

  it("stores a format 2 record with the versioned cost header; the passphrase never appears in storage", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A, { additionalFactor: pass("hunter2hunter2") });
    const rec = await getRec(env);
    expect(rec.format).toBe(2);
    expect(rec.ciphertext.length).toBe(12 + 32 + 16 + 16);
    expect({ ...rec.passphrase, salt: rec.passphrase?.salt.length }).toEqual({ kdf: 1, m: 64, t: 1, p: 1, salt: 16 });
    const pw = hex(enc.encode("hunter2hunter2"));
    const dump = (v: unknown): string => v instanceof Uint8Array ? hex(v) : typeof v === "object" && v !== null ? Object.values(v).map(dump).join("|") : String(v);
    expect(dump(rec).includes(pw)).toBe(false);
  });

  it("a passphrase given for a vault without one is ignored and zeroed", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    vault.lock(A);
    const f = pass("unused");
    expect(await vault.unlock(A, { passphrase: f })).toEqual({ ok: true });
    expect(f.secret.every((b) => b === 0)).toBe(true);
  });

  it("concurrent unlocks share one derivation; the joining call's passphrase is zeroed", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A, { additionalFactor: pass("pw-one") });
    vault.lock(A);
    const second = pass("pw-one");
    expect(await Promise.all([vault.unlock(A, { passphrase: pass("pw-one") }), vault.unlock(A, { passphrase: second })])).toEqual([{ ok: true }, { ok: true }]);
    expect(second.secret.every((b) => b === 0)).toBe(true);
  });

  it("a lock during the derivation wins", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A, { additionalFactor: pass("pw") });
    vault.lock(A);
    const p = vault.unlock(A, { passphrase: pass("pw") });
    vault.lock(A);
    expect(await p).toEqual({ ok: false, reason: "conflict" });
    expect(await vault.status(A)).toBe("locked");
  });
});

describe("passphrase layer: header integrity and cost floor", () => {
  async function protectedVault(): Promise<{ env: FakeEnv; vault: DeviceVault; rec: StoredRecord }> {
    const s = setup({ auth: null });
    await s.vault.createProfileVault(A, { additionalFactor: pass("pw") });
    s.vault.lock(A);
    return { ...s, rec: await getRec(s.env) };
  }
  const withPass = (rec: StoredRecord, patch: Partial<NonNullable<StoredRecord["passphrase"]>>): StoredRecord =>
    ({ ...rec, passphrase: { ...(rec.passphrase ?? { kdf: 1, m: 64, t: 1, p: 1, salt: new Uint8Array(16) }), ...patch } });

  it("edited cost or salt inside the accepted range fails authentication (cannot be downgraded or swapped)", async () => {
    const { env, vault, rec } = await protectedVault();
    const salt = rec.passphrase?.salt.slice() ?? new Uint8Array(16);
    salt[0] = (salt[0] ?? 0) ^ 1;
    for (const patch of [{ m: 32 }, { m: 128 }, { t: 2 }, { salt }]) {
      await putRec(env, withPass(rec, patch));
      expect(await vault.unlock(A, { passphrase: pass("pw") }), JSON.stringify(Object.keys(patch))).toEqual({ ok: false, reason: "unwrap_failed" });
    }
    await putRec(env, rec);
    expect(await vault.unlock(A, { passphrase: pass("pw") })).toEqual({ ok: true });
  });

  it("a header below the floor, above the max, of an unknown KDF version, or a stripped layer is corrupt", async () => {
    const { env, vault, rec } = await protectedVault();
    const bad: unknown[] = [
      withPass(rec, { m: 4 }), withPass(rec, { t: 0 }), withPass(rec, { m: 2048 }), withPass(rec, { t: 5 }), withPass(rec, { p: 2 }),
      withPass(rec, { kdf: 2 }), withPass(rec, { salt: new Uint8Array(8) }), { ...rec, passphrase: undefined },
      { ...rec, format: 1 }, { ...rec, format: 1, passphrase: undefined },
    ];
    for (const r of bad) {
      await putRec(env, r);
      expect(await vault.status(A)).toBe("corrupt");
      expect(await vault.unlock(A, { passphrase: pass("pw") })).toEqual({ ok: false, reason: "corrupt" });
    }
  });

  it("the production vault rejects a record whose cost is below the production floor", async () => {
    const { env } = await protectedVault(); // written with the tiny test cost (64 KiB, t = 1)
    const prod = createWebDeviceVault(env);
    expect(await prod.status(A)).toBe("corrupt");
    expect(await prod.unlock(A, { passphrase: pass("pw") })).toEqual({ ok: false, reason: "corrupt" });
  });

  it("the production vault writes the standard cost (64 MiB, t = 3, p = 1) and the floor is below it", async () => {
    const env = fakeEnv({ auth: null });
    const prod = createWebDeviceVault(env);
    expect((await prod.createProfileVault(A, { wrapping: "device-key", additionalFactor: pass("pw") })).ok).toBe(true);
    expect((await getRec(env)).passphrase).toMatchObject({ kdf: 1, m: 65_536, t: 3, p: 1 });
    const { floor, standard, max } = PRODUCTION_KDF_POLICY;
    expect(floor.m <= standard.m && floor.t <= standard.t && standard.m <= max.m && standard.t <= max.t).toBe(true);
    expect(Object.isFrozen(PRODUCTION_KDF_POLICY) && Object.isFrozen(PRODUCTION_KDF_POLICY.floor)).toBe(true);
  }, 60_000);
});

describe("passphrase layer: calibration", () => {
  it("only raises the pass count, bounded by the policy maximum", async () => {
    const cost = async (targetMs: number | undefined): Promise<number | undefined> => {
      const base = fakeEnv({ auth: null });
      let t = 0;
      const env: FakeEnv = { ...base, now: () => (t += 10) }; // every derivation pass "takes" 10 ms
      const v = createVaultWithKdfPolicy(env, TEST_POLICY);
      const r = await v.createProfileVault(A, { additionalFactor: pass("pw"), ...(targetMs === undefined ? {} : { passphraseTargetMs: targetMs }) });
      expect(r.ok).toBe(true);
      return (await getRec(env)).passphrase?.t;
    };
    expect(await cost(undefined)).toBe(1);
    expect(await cost(-5)).toBe(1);
    expect(await cost(Number.NaN)).toBe(1);
    expect(await cost(5)).toBe(1); // never below the standard
    expect(await cost(30)).toBe(3);
    expect(await cost(1_000_000)).toBe(4); // capped at max.t
  });
});

describe("passphrase layer: rotation keeps it", () => {
  it("re-wraps device key -> PRF -> device key without the passphrase; the layer and data survive unchanged", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: false });
    const { env, vault } = setup({ auth });
    await vault.createProfileVault(A, { wrapping: "device-key", additionalFactor: pass("keep me") });
    const kv = createEncryptedKv(vault, env, A);
    await kv.put("k", enc.encode("survives"));
    const before = await getRec(env);

    expect(await vault.rotateWrapping(A)).toMatchObject({ ok: true, wrapping: { method: "webauthn-prf", passphraseProtected: true } });
    const mid = await getRec(env);
    expect(mid.format).toBe(2);
    expect(mid.passphrase).toEqual(before.passphrase); // same salt and cost
    expect(mid.nonce).not.toEqual(before.nonce);
    expect(await rawStore(env, A, STORE_DEVICE_KEY, (s) => s.count())).toBe(0);

    expect(await vault.rotateWrapping(A, { wrapping: "device-key" })).toMatchObject({ ok: true, wrapping: { method: "device-key", passphraseProtected: true } });
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "passphrase_required" });
    expect(await vault.unlock(A, { passphrase: pass("nope") })).toEqual({ ok: false, reason: "wrong_passphrase" });
    expect(await vault.unlock(A, { passphrase: pass("keep me") })).toEqual({ ok: true });
    const g = await kv.get("k");
    expect(g.ok && g.value && text(g.value)).toBe("survives");
  });
});

describe("format 1 compatibility", () => {
  it("a format 1 record built with the original construction (written out here) still unlocks", async () => {
    const env = fakeEnv({ auth: null });
    const db = await openOrCreate(env.indexedDB, dbName(A));
    db?.close();
    const pvk = crypto.getRandomValues(new Uint8Array(32));
    const deviceKey = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"]);
    const nonce = crypto.getRandomValues(new Uint8Array(12));
    const aad = new Uint8Array([...enc.encode("om:v1:pvk-wrap"), 0, 1, 0, ...enc.encode("device-key"), 0, ...enc.encode(A)]);
    const ciphertext = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: aad }, deviceKey, pvk));
    await rawStore(env, A, STORE_DEVICE_KEY, (s) => s.put(deviceKey, DEVICE_KEY_ID));
    await putRec(env, { format: 1, profileId: A, method: "device-key", reason: "webauthn_absent", generation: 1, nonce, ciphertext });
    const vault = createWebDeviceVault(env);
    expect(await vault.wrapping(A)).toEqual({ ok: true, wrapping: { method: "device-key", reason: "webauthn_absent", reducedAtRestProperty: false, passphraseProtected: false } });
    expect(await vault.unlock(A)).toEqual({ ok: true });
  });
});

describe("Argon2id known answers", () => {
  it("the library matches RFC 9106 §5.3 (Argon2id, with secret and associated data)", () => {
    const tag = argon2id(new Uint8Array(32).fill(1), new Uint8Array(16).fill(2),
      { m: 32, t: 3, p: 4, dkLen: 32, key: new Uint8Array(8).fill(3), personalization: new Uint8Array(12).fill(4) });
    expect(hex(tag)).toBe("0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659");
  });

  it("our derivation (NFKC UTF-8, no secret/AD, 32-byte output) is pinned", async () => {
    // Regression vector computed once with @noble/hashes 2.4.0 (no second implementation was available to cross-check;
    // the RFC vector above covers the primitive). Inputs: "Pa\u0308ssword \uFF11 x" (decomposed a-umlaut, full-width 1), salt 16 x 0x02.
    const pw = normalizePassphrase(enc.encode("Pa\u0308ssword \uFF11 x"));
    if (pw === undefined) throw new Error("normalize");
    expect(text(pw)).toBe("P\u00E4ssword 1 x");
    expect(hex(await argon2idRaw(pw, new Uint8Array(16).fill(2), { m: 64, t: 2, p: 1 })))
      .toBe("cab6e8b647da249e172266ce0abf73826ac3ff5739368c1ba85b400795944291");
  });

  it("normalisation zeroes its input and refuses empty or non-UTF-8 bytes", () => {
    const b = enc.encode("x");
    expect(normalizePassphrase(b)).toEqual(enc.encode("x"));
    expect(b).toEqual(new Uint8Array([0]));
    expect(normalizePassphrase(new Uint8Array(0))).toBeUndefined();
    expect(normalizePassphrase(new Uint8Array([0xc3]))).toBeUndefined();
  });
});
