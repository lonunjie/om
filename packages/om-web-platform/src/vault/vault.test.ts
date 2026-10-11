import { describe, expect, it } from "vitest";
import { IDBFactory } from "fake-indexeddb";
import type { DeviceVault, PassphraseFactor, ProfileId } from "@om/platform-api";
import { VAULT_PURPOSES, asProfileId } from "@om/platform-api";
import { createEncryptedKv, createWebDeviceVault } from "./index";
import { sealBytes, openBytes, parseSealed, deriveSubkey, importPvk } from "./crypto";
import { DEVICE_KEY_ID, STORE_DEVICE_KEY, STORE_KV, STORE_WRAPPED, WRAPPED_ID, committed, dbName, openExisting, openOrCreate, request } from "./idb";
import { FakeAuthenticator, cryptoSpy, fakeEnv } from "./__fixtures__/fake-env";
import type { FakeEnv } from "./__fixtures__/fake-env";

const A = asProfileId("profile-a");
const B = asProfileId("profile-b");
const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);
const text = (b: Uint8Array): string => new TextDecoder().decode(b);

function setup(o: Parameters<typeof fakeEnv>[0] = {}): { env: FakeEnv; vault: DeviceVault } {
  const env = fakeEnv(o);
  return { env, vault: createWebDeviceVault(env) };
}

async function sealOk(v: DeviceVault, p: ProfileId, s: string, purpose: (typeof VAULT_PURPOSES)[number] = "local-db"): Promise<Uint8Array> {
  const r = await v.seal(p, purpose, bytes(s));
  if (!r.ok) throw new Error(r.reason);
  return r.sealed;
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

const deleteRaw = (env: FakeEnv, name: string): Promise<void> => new Promise((res) => {
  const r = env.indexedDB.deleteDatabase(name);
  r.onsuccess = () => { res(); };
});

describe("seal/open", () => {
  it("round-trips every purpose and binds the purpose", async () => {
    const { vault } = setup({ auth: null });
    expect((await vault.createProfileVault(A)).ok).toBe(true);
    for (const purpose of VAULT_PURPOSES) {
      const sealed = await sealOk(vault, A, `hello ${purpose}`, purpose);
      const r = await vault.open(A, purpose, sealed);
      expect(r.ok && text(r.bytes)).toBe(`hello ${purpose}`);
      const other = VAULT_PURPOSES.find((p) => p !== purpose) ?? "local-db";
      expect(await vault.open(A, other, sealed)).toEqual({ ok: false, reason: "wrong_purpose" });
    }
  });

  it("fails closed on tamper, truncation, unknown version, bad magic and a relabelled purpose", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const sealed = await sealOk(vault, A, "secret");
    const flip = (i: number): Uint8Array => { const c = sealed.slice(); c[i] = (c[i] ?? 0) ^ 1; return c; };
    expect(await vault.open(A, "local-db", flip(sealed.length - 1))).toEqual({ ok: false, reason: "integrity_failure" });
    expect(await vault.open(A, "local-db", flip(10))).toEqual({ ok: false, reason: "integrity_failure" }); // nonce
    expect(await vault.open(A, "local-db", flip(0))).toEqual({ ok: false, reason: "malformed" });
    expect(await vault.open(A, "local-db", flip(4))).toEqual({ ok: false, reason: "unsupported_version" });
    expect(await vault.open(A, "local-db", sealed.slice(0, 20))).toEqual({ ok: false, reason: "malformed" });
    const relabelled = sealed.slice();
    relabelled[5] = 2; // header now claims search-index; AAD no longer matches
    expect(await vault.open(A, "search-index", relabelled)).toEqual({ ok: false, reason: "integrity_failure" });
    expect(await vault.open(A, "nope" as "local-db", sealed)).toEqual({ ok: false, reason: "invalid_purpose" });
  });

  it("binds the profile in AAD even under the same key", async () => {
    const key = await deriveSubkey(crypto, await importPvk(crypto, crypto.getRandomValues(new Uint8Array(32))), "local-db");
    const s = parseSealed(await sealBytes(crypto, key, "profile-a", "local-db", new Uint8Array([1, 2, 3])));
    if (!s.ok) throw new Error("parse");
    expect(await openBytes(crypto, key, "profile-a", s)).toEqual(new Uint8Array([1, 2, 3]));
    expect(await openBytes(crypto, key, "profile-b", s)).toBeUndefined();
  });

  it("another profile's vault cannot open the blob", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    await vault.createProfileVault(B);
    expect(await vault.open(B, "local-db", await sealOk(vault, A, "x"))).toEqual({ ok: false, reason: "integrity_failure" });
  });

  it("uses a fresh nonce for every seal", async () => {
    const { vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const nonces = new Set<string>();
    for (let i = 0; i < 2000; i++) {
      const s = parseSealed(await sealOk(vault, A, "same"));
      if (!s.ok) throw new Error("parse");
      nonces.add(Array.from(s.nonce).join(","));
    }
    expect(nonces.size).toBe(2000);
  });
});

describe("wrapper selection by test (R-391)", () => {
  it("PRF advertised and enabled but no output: falls back to the device key (third-party provider)", async () => {
    const auth = new FakeAuthenticator({ prf: "enabled_no_output" });
    const { vault } = setup({ auth });
    const r = await vault.createProfileVault(A);
    expect(r).toEqual({ ok: true, wrapping: { method: "device-key", reason: "prf_no_output", reducedAtRestProperty: false, passphraseProtected: false } });
    expect([auth.createCalls, auth.getCalls]).toEqual([1, 1]);
  });

  it("PRF works with a synced credential: reduced at-rest property is exposed", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: true });
    const { vault } = setup({ auth });
    const r = await vault.createProfileVault(A);
    const info = { method: "webauthn-prf", reason: "prf_verified", credentialBackup: "synced", reducedAtRestProperty: true, passphraseProtected: false };
    expect(r).toEqual({ ok: true, wrapping: info });
    expect(await vault.wrapping(A)).toEqual({ ok: true, wrapping: info });
    const sealed = await sealOk(vault, A, "via prf");
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: true });
    expect(auth.getCalls).toBe(2);
    const o = await vault.open(A, "local-db", sealed);
    expect(o.ok && text(o.bytes)).toBe("via prf");
  });

  it("PRF with a device-bound credential: no reduced property", async () => {
    const { vault } = setup({ auth: new FakeAuthenticator({ prf: "ok", backupEligible: false }) });
    const r = await vault.createProfileVault(A);
    expect(r.ok && r.wrapping).toEqual({ method: "webauthn-prf", reason: "prf_verified", credentialBackup: "device_bound", reducedAtRestProperty: false, passphraseProtected: false });
  });

  it("missing authenticator data is treated as synced", async () => {
    const auth = new FakeAuthenticator({ prf: "ok", backupEligible: false });
    auth.authData = false;
    const { vault } = setup({ auth });
    const r = await vault.createProfileVault(A);
    expect(r.ok && r.wrapping.credentialBackup).toBe("unknown");
    expect(r.ok && r.wrapping.reducedAtRestProperty).toBe(true);
  });

  it("device-bound credential without PRF is not used: device key, no get()", async () => {
    const auth = new FakeAuthenticator({ prf: "not_enabled", backupEligible: false });
    const { vault } = setup({ auth });
    const r = await vault.createProfileVault(A);
    expect(r.ok && r.wrapping).toEqual({ method: "device-key", reason: "prf_not_enabled", reducedAtRestProperty: false, passphraseProtected: false });
    expect(auth.getCalls).toBe(0);
  });

  it("no WebAuthn, a cancelled create, or an explicit request all use the device key", async () => {
    expect(await setup({ auth: null }).vault.createProfileVault(A)).toMatchObject({ ok: true, wrapping: { reason: "webauthn_absent" } });
    const auth = new FakeAuthenticator();
    auth.failCreate = true;
    expect(await setup({ auth }).vault.createProfileVault(A)).toMatchObject({ ok: true, wrapping: { reason: "webauthn_create_failed" } });
    const auth2 = new FakeAuthenticator();
    expect(await setup({ auth: auth2 }).vault.createProfileVault(A, { wrapping: "device-key" }))
      .toMatchObject({ ok: true, wrapping: { method: "device-key", reason: "device_key_requested" } });
    expect(auth2.createCalls).toBe(0);
  });

  it("reports the passphrase factor as available; refuses an unknown factor kind or an empty passphrase", async () => {
    const { vault } = setup();
    expect(vault.capabilities()).toEqual({ webauthnPresent: true, deviceKeyPresent: true, passphraseFactor: "available", reliesOnPersistentStorage: false });
    expect(await vault.createProfileVault(A, { additionalFactor: { kind: "pin", secret: bytes("1") } as unknown as PassphraseFactor }))
      .toEqual({ ok: false, reason: "unsupported_factor" });
    expect(await vault.createProfileVault(A, { additionalFactor: { kind: "passphrase", secret: bytes("") } }))
      .toEqual({ ok: false, reason: "passphrase_required" });
    expect(await vault.createProfileVault(A, { additionalFactor: { kind: "passphrase", secret: new Uint8Array([0xff, 0xfe]) } }))
      .toEqual({ ok: false, reason: "passphrase_required" });
    expect(await vault.status(A)).toBe("absent");
  });
});

describe("chained prompts", () => {
  it("focus never arrives: needs_user_gesture, no ceremony, nothing persisted", async () => {
    const auth = new FakeAuthenticator();
    const { env, vault } = setup({ auth });
    env.focused = () => false;
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "needs_user_gesture", stage: "enroll_create" });
    expect(auth.createCalls).toBe(0);
    expect(env.clock.t).toBeGreaterThanOrEqual(3000);
    expect(env.clock.t).toBeLessThan(3200);
    expect(await vault.status(A)).toBe("absent");
  });

  it("focus lost between create and get: needs a new gesture for the second prompt", async () => {
    const auth = new FakeAuthenticator();
    const { env, vault } = setup({ auth });
    auth.onCeremony = () => { env.focused = () => false; return Promise.resolve(); };
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "needs_user_gesture", stage: "enroll_verify" });
    expect(auth.getCalls).toBe(0);
  });

  it("focus returning after ~1.4 s proceeds; unlock without focus asks for a gesture", async () => {
    const auth = new FakeAuthenticator();
    const { env, vault } = setup({ auth });
    env.focused = () => env.clock.t >= 1400;
    expect((await vault.createProfileVault(A)).ok).toBe(true);
    vault.lock(A);
    env.focused = () => false;
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "needs_user_gesture", stage: "unlock" });
    expect(await vault.status(A)).toBe("locked");
  });
});

describe("storage", () => {
  it("one database per profile; profile ids are validated", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    await vault.createProfileVault(B);
    const names = (await env.indexedDB.databases()).map((d) => d.name).sort();
    expect(names).toEqual(["om-vault-profile-a", "om-vault-profile-b"]);
    for (const bad of ["", "../x", "a b", "a".repeat(65), "-x", "x/y"]) {
      expect(await vault.createProfileVault(asProfileId(bad))).toEqual({ ok: false, reason: "invalid_profile_id" });
      expect(await vault.status(asProfileId(bad))).toBe("corrupt");
    }
  });

  it("EncryptedKv round-trips, lists, deletes, and stores only ciphertext", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const kv = createEncryptedKv(vault, env, A);
    expect(await kv.put("draft:1", bytes("plain words"))).toEqual({ ok: true });
    const g = await kv.get("draft:1");
    expect(g.ok && g.value && text(g.value)).toBe("plain words");
    expect(await kv.list()).toEqual({ ok: true, keys: ["draft:1"] });
    const raw = await rawStore(env, A, STORE_KV, (s) => s.get("draft:1"));
    expect(raw instanceof Uint8Array && text(raw).includes("plain words")).toBe(false);
    expect(await kv.get("missing")).toEqual({ ok: true, value: undefined });
    expect(await kv.delete("draft:1")).toEqual({ ok: true });
    expect(await kv.list()).toEqual({ ok: true, keys: [] });
    vault.lock(A);
    expect(await kv.put("k", bytes("v"))).toEqual({ ok: false, reason: "locked" });
  });

  it("a value moved to another key fails closed", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const kv = createEncryptedKv(vault, env, A);
    await kv.put("a", bytes("for a"));
    const sealed = await rawStore(env, A, STORE_KV, (s) => s.get("a"));
    await rawStore(env, A, STORE_KV, (s) => s.put(sealed, "b"));
    expect(await kv.get("b")).toEqual({ ok: false, reason: "integrity_failure" });
  });

  it("cross-profile isolation", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    await vault.createProfileVault(B);
    const kvA = createEncryptedKv(vault, env, A);
    const kvB = createEncryptedKv(vault, env, B);
    await kvA.put("k", bytes("only A"));
    expect(await kvB.list()).toEqual({ ok: true, keys: [] });
    expect(await kvB.get("k")).toEqual({ ok: true, value: undefined });
    const sealedA = await rawStore(env, A, STORE_KV, (s) => s.get("k"));
    await rawStore(env, B, STORE_KV, (s) => s.put(sealedA, "k"));
    expect(await kvB.get("k")).toEqual({ ok: false, reason: "integrity_failure" });
    vault.lock(B);
    const g = await kvA.get("k");
    expect(g.ok && g.value && text(g.value)).toBe("only A");
  });

  it("eviction: known profile whose database vanished reports evicted", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    expect(await vault.status(A)).toBe("unlocked");
    await deleteRaw(env, dbName(A));
    expect(await vault.status(A, { known: true })).toBe("evicted");
    expect(await vault.status(A)).toBe("absent");
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "not_found" });
    expect(await vault.seal(A, "local-db", bytes("x"))).toEqual({ ok: false, reason: "locked" });
  });

  it("corrupt record: reported, never re-created over existing data", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    await createEncryptedKv(vault, env, A).put("k", bytes("v"));
    await rawStore(env, A, STORE_WRAPPED, (s) => s.put({ format: 9 }, WRAPPED_ID));
    vault.lock(A);
    expect(await vault.status(A)).toBe("corrupt");
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "corrupt" });
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "corrupt" });
    expect(await rawStore(env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID))).toEqual({ format: 9 });
    // Record removed but data present: still corrupt, still refused.
    await rawStore(env, A, STORE_WRAPPED, (s) => s.delete(WRAPPED_ID));
    expect(await vault.status(A)).toBe("corrupt");
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "corrupt" });
    expect(await rawStore(env, A, STORE_KV, (s) => s.count())).toBe(1);
  });

  it("data that appears while enrolling is never overwritten (checked inside the write transaction)", async () => {
    const auth = new FakeAuthenticator();
    const { env, vault } = setup({ auth });
    const db = await openOrCreate(env.indexedDB, dbName(A)); // empty database left by an interrupted enrollment
    db?.close();
    expect(await vault.status(A)).toBe("absent");
    auth.onCeremony = async () => {
      auth.onCeremony = undefined;
      await rawStore(env, A, STORE_KV, (s) => s.put(new Uint8Array([1]), "late"));
    };
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "corrupt" });
    expect(await rawStore(env, A, STORE_WRAPPED, (s) => s.count())).toBe(0);
    expect(await rawStore(env, A, STORE_KV, (s) => s.get("late"))).toEqual(new Uint8Array([1]));
  });

  it("missing device key or a foreign layout is corrupt", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    await rawStore(env, A, STORE_DEVICE_KEY, (s) => s.delete(DEVICE_KEY_ID));
    expect(await vault.status(A)).toBe("corrupt");
    await new Promise<void>((res) => {
      const r = env.indexedDB.open(dbName(B), 3);
      r.onupgradeneeded = () => { r.result.createObjectStore("other"); };
      r.onsuccess = () => { r.result.close(); res(); };
    });
    expect(await vault.status(B)).toBe("corrupt");
    expect(await vault.createProfileVault(B)).toEqual({ ok: false, reason: "corrupt" });
  });

  it("refuses to create over an existing vault, including concurrent creates", async () => {
    const { vault } = setup({ auth: null });
    const rs = await Promise.all([vault.createProfileVault(A), vault.createProfileVault(A), vault.createProfileVault(A)]);
    expect(rs.filter((r) => r.ok)).toHaveLength(1);
    expect(rs.filter((r) => !r.ok)).toEqual([{ ok: false, reason: "already_exists" }, { ok: false, reason: "already_exists" }]);
    expect(await vault.createProfileVault(A)).toEqual({ ok: false, reason: "already_exists" });
  });

  it("delete removes the database, is verified, and signals the passkey as unknown", async () => {
    const { env, vault } = setup();
    await vault.createProfileVault(A);
    await createEncryptedKv(vault, env, A).put("k", bytes("v"));
    expect(await vault.deleteProfileVault(A)).toEqual({ ok: true });
    expect(await env.indexedDB.databases()).toEqual([]);
    expect(await vault.status(A)).toBe("absent");
    expect(env.signalled).toHaveLength(1);
    expect(await vault.deleteProfileVault(A)).toEqual({ ok: true });
  });

  it("delete blocked by a connection that will not close is reported unverified", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const foreign = await new Promise<IDBDatabase>((res) => { const r = env.indexedDB.open(dbName(A)); r.onsuccess = () => { res(r.result); }; });
    expect(await vault.deleteProfileVault(A)).toEqual({ ok: false, reason: "delete_unverified" });
    foreign.close();
  });
});

describe("lock model", () => {
  it("lock drops the key; unlock restores it; concurrent unlocks share one ceremony", async () => {
    const auth = new FakeAuthenticator();
    const { vault } = setup({ auth });
    await vault.createProfileVault(A);
    const sealed = await sealOk(vault, A, "kept");
    vault.lock(A);
    expect(await vault.status(A)).toBe("locked");
    expect(await vault.seal(A, "local-db", bytes("x"))).toEqual({ ok: false, reason: "locked" });
    expect(await vault.open(A, "local-db", sealed)).toEqual({ ok: false, reason: "locked" });
    const before = auth.getCalls;
    expect(await Promise.all([vault.unlock(A), vault.unlock(A)])).toEqual([{ ok: true }, { ok: true }]);
    expect(auth.getCalls - before).toBe(1);
    const o = await vault.open(A, "local-db", sealed);
    expect(o.ok && text(o.bytes)).toBe("kept");
  });

  it("a lock during a pending unlock wins", async () => {
    const auth = new FakeAuthenticator();
    const { vault } = setup({ auth });
    await vault.createProfileVault(A);
    vault.lock(A);
    auth.onCeremony = () => { vault.lock(A); return Promise.resolve(); };
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "conflict" });
    expect(await vault.status(A)).toBe("locked");
  });

  it("a wrapper that does not authenticate fails closed", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const rec = await rawStore(env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID)) as { ciphertext: Uint8Array };
    rec.ciphertext[0] = (rec.ciphertext[0] ?? 0) ^ 1;
    await rawStore(env, A, STORE_WRAPPED, (s) => s.put(rec, WRAPPED_ID));
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: false, reason: "unwrap_failed" });
  });
});

describe("rotation", () => {
  it("re-wraps the same PVK under a fresh device key without touching data", async () => {
    const { env, vault } = setup({ auth: null });
    await vault.createProfileVault(A);
    const kv = createEncryptedKv(vault, env, A);
    await kv.put("k", bytes("survives"));
    const sealedBefore = await rawStore(env, A, STORE_KV, (s) => s.get("k"));
    const before = await rawStore(env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID)) as { nonce: Uint8Array; generation: number };
    expect(await vault.rotateWrapping(A)).toEqual({ ok: true, wrapping: { method: "device-key", reason: "webauthn_absent", reducedAtRestProperty: false, passphraseProtected: false } });
    const after = await rawStore(env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID)) as { nonce: Uint8Array; generation: number };
    expect(after.generation).toBe(before.generation + 1);
    expect(after.nonce).not.toEqual(before.nonce);
    expect(await rawStore(env, A, STORE_KV, (s) => s.get("k"))).toEqual(sealedBefore);
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: true });
    const g = await kv.get("k");
    expect(g.ok && g.value && text(g.value)).toBe("survives");
  });

  it("device key -> PRF removes the device key in the same transaction; PRF -> device key signals the old passkey", async () => {
    const auth = new FakeAuthenticator();
    const { env, vault } = setup({ auth });
    await vault.createProfileVault(A, { wrapping: "device-key" });
    const sealed = await sealOk(vault, A, "x");
    expect(await vault.rotateWrapping(A)).toMatchObject({ ok: true, wrapping: { method: "webauthn-prf", reducedAtRestProperty: true } });
    expect(await rawStore(env, A, STORE_DEVICE_KEY, (s) => s.count())).toBe(0);
    expect(await vault.rotateWrapping(A, { wrapping: "device-key" })).toMatchObject({ ok: true, wrapping: { method: "device-key" } });
    expect(env.signalled).toHaveLength(1);
    vault.lock(A);
    await vault.unlock(A);
    const o = await vault.open(A, "local-db", sealed);
    expect(o.ok && text(o.bytes)).toBe("x");
  });

  it("a concurrent rotation is detected (compare-and-swap) and leaves a consistent wrapper", async () => {
    const auth = new FakeAuthenticator();
    const { vault } = setup({ auth });
    await vault.createProfileVault(A, { wrapping: "device-key" });
    let inner: unknown;
    auth.onCeremony = async () => {
      auth.onCeremony = undefined;
      inner = await vault.rotateWrapping(A, { wrapping: "device-key" });
    };
    expect(await vault.rotateWrapping(A)).toEqual({ ok: false, reason: "conflict" });
    expect(inner).toMatchObject({ ok: true });
    vault.lock(A);
    expect(await vault.unlock(A)).toEqual({ ok: true });
  });
});

describe("migration to an unrelated origin (R-392)", () => {
  it("old passkeys are rejected at the new RP ID; a fresh vault works there without them", async () => {
    const auth = new FakeAuthenticator(); // one passkey provider, two origins
    const a = setup({ rpId: "old.om.test", auth });
    expect((await a.vault.createProfileVault(A)).ok).toBe(true);
    const oldCred = [...auth.creds.values()][0];
    if (oldCred === undefined) throw new Error("no credential");

    const idbB = new IDBFactory();
    const b = setup({ rpId: "new.om.test", auth, idb: idbB });
    const credsB = b.env.credentials;
    if (credsB === undefined) throw new Error("no credentials");
    await expect(credsB.get({ publicKey: {
      challenge: new Uint8Array(32), rpId: "new.om.test", userVerification: "required", timeout: 1000,
      allowCredentials: [{ type: "public-key", id: new Uint8Array(oldCred.id) }], extensions: { prf: { eval: { first: new Uint8Array(32) } } },
    } })).rejects.toMatchObject({ name: "NotAllowedError" });
    expect(await b.vault.status(A)).toBe("absent");
    expect(await b.vault.createProfileVault(A)).toMatchObject({ ok: true, wrapping: { method: "webauthn-prf" } });
    expect(await b.vault.createProfileVault(B, { wrapping: "device-key" })).toMatchObject({ ok: true });

    // Copying the old origin's records is not migration: the old wrapper is never usable here.
    const c = setup({ rpId: "copy.om.test", auth });
    await c.vault.createProfileVault(A, { wrapping: "device-key" });
    const oldRec = await rawStore(a.env, A, STORE_WRAPPED, (s) => s.get(WRAPPED_ID));
    await rawStore(c.env, A, STORE_DEVICE_KEY, (s) => s.delete(DEVICE_KEY_ID));
    await rawStore(c.env, A, STORE_WRAPPED, (s) => s.put(oldRec, WRAPPED_ID));
    c.vault.lock(A);
    expect(await c.vault.unlock(A)).toEqual({ ok: false, reason: "prf_unavailable", stage: "unlock" });
  });
});

describe("no API returns key material", () => {
  it("results never contain CryptoKeys, the PVK, or any random secret; every key is non-extractable", async () => {
    const spy = cryptoSpy();
    const auth = new FakeAuthenticator();
    const env = fakeEnv({ auth, crypto: spy.crypto });
    const vault = createWebDeviceVault(env);
    const kv = createEncryptedKv(vault, env, A);
    const results: unknown[] = [vault.capabilities()];
    const rec = async (p: Promise<unknown>): Promise<void> => { results.push(await p); };
    await rec(vault.createProfileVault(A));
    await rec(vault.createProfileVault(B, { wrapping: "device-key" }));
    await rec(vault.status(A));
    await rec(vault.wrapping(A));
    await rec(vault.seal(A, "local-db", bytes("x")));
    await rec(kv.put("k", bytes("v")));
    await rec(kv.get("k"));
    await rec(kv.list());
    vault.lock(A);
    await rec(vault.unlock(A));
    await rec(vault.rotateWrapping(A));
    await rec(vault.rotateWrapping(B));
    await rec(vault.deleteProfileVault(B));

    const secrets = spy.randoms.filter((r) => r.length === 32).map((r) => Array.from(r).join(","));
    expect(secrets.length).toBeGreaterThan(0);
    const walk = (v: unknown, path: string): void => {
      expect(v instanceof CryptoKey, path).toBe(false);
      if (v instanceof Uint8Array) {
        for (let i = 0; i + 32 <= v.length; i++) expect(secrets.includes(Array.from(v.subarray(i, i + 32)).join(",")), path).toBe(false);
        return;
      }
      if (typeof v === "object" && v !== null) for (const [k, x] of Object.entries(v)) { expect(/key$|pvk|secret|kek/i.test(k), `${path}.${k}`).toBe(false); walk(x, `${path}.${k}`); }
    };
    results.forEach((r, i) => { walk(r, `result${String(i)}`); });
    expect(spy.keys.length).toBeGreaterThan(0);
    expect(spy.keys.every((k) => !k.extractable)).toBe(true);
    expect(spy.exports).toBe(0);
  });
});
