// Test-only: fake WebAuthn authenticator (RP-ID bound, PRF = HMAC like CTAP hmac-secret), fake focus/clock, crypto spy.
import { IDBFactory } from "fake-indexeddb";
import type { CredentialsLike, PublicKeyCreateOptionsLike, PublicKeyGetOptionsLike, VaultEnv } from "../env";

export type PrfMode = "ok" | "enabled_no_output" | "not_enabled";

interface StoredCred { readonly id: Uint8Array; readonly rpId: string; readonly secret: CryptoKey; readonly backupEligible: boolean }

const enc = new TextEncoder();
const hex = (b: Uint8Array): string => Array.from(b, (v) => v.toString(16).padStart(2, "0")).join("");
const notAllowed = (): Error => Object.assign(new Error("not allowed"), { name: "NotAllowedError" });

export class FakeAuthenticator {
  prf: PrfMode;
  backupEligible: boolean;
  authData = true;
  failCreate = false;
  createCalls = 0;
  getCalls = 0;
  /** Runs inside every ceremony, before it resolves (to interleave operations deterministically). */
  onCeremony: (() => Promise<void>) | undefined;
  readonly creds = new Map<string, StoredCred>();

  constructor(o: { prf?: PrfMode; backupEligible?: boolean } = {}) {
    this.prf = o.prf ?? "ok";
    this.backupEligible = o.backupEligible ?? true;
  }

  private flags(be: boolean): ArrayBuffer {
    const ad = new Uint8Array(37);
    ad[32] = 0x01 | 0x04 | (be ? 0x18 : 0);
    return ad.buffer;
  }

  private async evalPrf(c: StoredCred, input: Uint8Array): Promise<ArrayBuffer> {
    const msg = new Uint8Array([...enc.encode("WebAuthn PRF"), 0, ...input]);
    return crypto.subtle.sign("HMAC", c.secret, msg);
  }

  /** The browser binds ceremonies to the page's origin: RP ID must equal this origin's hostname. */
  credentialsFor(originHost: string): CredentialsLike {
    return {
      create: async ({ publicKey }: { publicKey: PublicKeyCreateOptionsLike }) => {
        this.createCalls++;
        if (publicKey.rp.id !== originHost) throw Object.assign(new Error("rp"), { name: "SecurityError" });
        if (this.onCeremony) await this.onCeremony();
        if (this.failCreate) throw notAllowed();
        const id = crypto.getRandomValues(new Uint8Array(16));
        const secret = await crypto.subtle.generateKey({ name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
        const cred: StoredCred = { id, rpId: originHost, secret, backupEligible: this.backupEligible };
        this.creds.set(hex(id), cred);
        const enabled = this.prf !== "not_enabled";
        const first = this.prf === "ok" ? await this.evalPrf(cred, publicKey.extensions.prf.eval.first) : undefined;
        const authData = this.authData ? this.flags(cred.backupEligible) : undefined;
        return {
          type: "public-key",
          rawId: id.slice().buffer,
          response: authData === undefined ? {} : { getAuthenticatorData: () => authData },
          getClientExtensionResults: () => ({ prf: first === undefined ? { enabled } : { enabled, results: { first } } }),
        };
      },
      get: async ({ publicKey }: { publicKey: PublicKeyGetOptionsLike }) => {
        this.getCalls++;
        if (publicKey.rpId !== originHost) throw Object.assign(new Error("rp"), { name: "SecurityError" });
        if (this.onCeremony) await this.onCeremony();
        const cred = publicKey.allowCredentials.map((a) => this.creds.get(hex(a.id))).find((c) => c?.rpId === publicKey.rpId);
        if (cred === undefined) throw notAllowed();
        const ext = this.prf === "ok" ? { prf: { results: { first: await this.evalPrf(cred, publicKey.extensions.prf.eval.first) } } } : { prf: {} };
        return {
          type: "public-key",
          rawId: cred.id.slice().buffer,
          response: { authenticatorData: this.flags(cred.backupEligible) },
          getClientExtensionResults: () => ext,
        };
      },
    };
  }
}

export interface CryptoSpy {
  readonly crypto: Crypto;
  readonly randoms: Uint8Array[];
  readonly keys: CryptoKey[];
  exports: number;
}

/** Records every random buffer and every key WebCrypto hands back; counts export/wrap calls. */
export function cryptoSpy(): CryptoSpy {
  const real = globalThis.crypto;
  const spy: CryptoSpy = { crypto: undefined as unknown as Crypto, randoms: [], keys: [], exports: 0 };
  const subtle = new Proxy(real.subtle, {
    get(target, prop) {
      const v: unknown = Reflect.get(target, prop, target);
      if (typeof v !== "function") return v;
      return async (...args: unknown[]) => {
        if (prop === "exportKey" || prop === "wrapKey") spy.exports++;
        const r: unknown = await (v as (...a: unknown[]) => Promise<unknown>).apply(target, args);
        if (r instanceof CryptoKey) spy.keys.push(r);
        return r;
      };
    },
  });
  (spy as { crypto: Crypto }).crypto = {
    subtle,
    getRandomValues: <T extends ArrayBufferView>(a: T): T => {
      real.getRandomValues(a as unknown as Uint8Array<ArrayBuffer>);
      spy.randoms.push(new Uint8Array(a.buffer, a.byteOffset, a.byteLength).slice());
      return a;
    },
    randomUUID: () => real.randomUUID(),
  } as unknown as Crypto;
  return spy;
}

export interface FakeEnv extends VaultEnv {
  readonly clock: { t: number };
  focused: () => boolean;
  readonly signalled: string[];
}

export function fakeEnv(o: { rpId?: string; idb?: IDBFactory; auth?: FakeAuthenticator | null; crypto?: Crypto } = {}): FakeEnv {
  const rpId = o.rpId ?? "app.om.test";
  const clock = { t: 0 };
  const signalled: string[] = [];
  const auth = o.auth === undefined ? new FakeAuthenticator() : o.auth;
  const env: FakeEnv = {
    crypto: o.crypto ?? globalThis.crypto,
    indexedDB: (o.idb ?? new IDBFactory()) as unknown as globalThis.IDBFactory,
    rpId,
    rpName: "OM test",
    ...(auth === null ? {} : { credentials: auth.credentialsFor(rpId) }),
    clock,
    focused: () => true,
    hasFocus: () => env.focused(),
    now: () => clock.t,
    sleep: (ms) => { clock.t += ms; return new Promise((r) => setTimeout(r, 0)); },
    signalUnknownCredential: async ({ credentialId }) => { signalled.push(credentialId); },
    signalled,
  };
  return env;
}
