// Everything WebDeviceVault touches, injected so tests run in Node (fake-indexeddb + fake WebAuthn).
export type Bytes = Uint8Array<ArrayBuffer>;

export interface PrfValues { readonly first: Bytes; readonly second?: Bytes }

export interface PublicKeyCreateOptionsLike {
  readonly rp: { readonly name: string; readonly id: string };
  readonly user: { readonly id: Bytes; readonly name: string; readonly displayName: string };
  readonly challenge: Bytes;
  readonly pubKeyCredParams: readonly { readonly type: "public-key"; readonly alg: number }[];
  readonly authenticatorSelection: { readonly residentKey: "preferred"; readonly userVerification: "required" };
  readonly timeout: number;
  readonly extensions: { readonly prf: { readonly eval: PrfValues } };
}

export interface PublicKeyGetOptionsLike {
  readonly challenge: Bytes;
  readonly rpId: string;
  readonly userVerification: "required";
  readonly allowCredentials: readonly { readonly type: "public-key"; readonly id: Bytes }[];
  readonly timeout: number;
  readonly extensions: { readonly prf: { readonly eval: PrfValues } };
}

/** `navigator.credentials` slice. Results are `unknown` and parsed defensively. */
export interface CredentialsLike {
  create(options: { readonly publicKey: PublicKeyCreateOptionsLike }): Promise<unknown>;
  get(options: { readonly publicKey: PublicKeyGetOptionsLike }): Promise<unknown>;
}

export interface VaultEnv {
  readonly crypto: Crypto;
  readonly indexedDB: IDBFactory;
  /** Absent: WebAuthn unavailable, method 2 only. */
  readonly credentials?: CredentialsLike;
  /** RP ID for ceremonies (the app origin's hostname). */
  readonly rpId: string;
  readonly rpName: string;
  readonly hasFocus: () => boolean;
  readonly now: () => number;
  readonly sleep: (ms: number) => Promise<void>;
  /** Best effort at vault deletion: `PublicKeyCredential.signalUnknownCredential`. */
  readonly signalUnknownCredential?: (o: { readonly rpId: string; readonly credentialId: string }) => Promise<void>;
}

interface BrowserGlobals {
  readonly crypto?: Crypto;
  readonly indexedDB?: IDBFactory;
  readonly location?: { readonly hostname: string };
  readonly navigator?: { readonly credentials?: CredentialsLike };
  readonly document?: { hasFocus(): boolean };
  readonly performance?: { now(): number };
  readonly PublicKeyCredential?: {
    readonly signalUnknownCredential?: (o: { rpId: string; credentialId: string }) => Promise<void>;
  };
}

/** The live browser, or `undefined` when WebCrypto/IndexedDB are missing (no durable boundary: fail closed). */
export function browserVaultEnv(rpName: string): VaultEnv | undefined {
  const g = globalThis as unknown as BrowserGlobals;
  const { crypto, indexedDB, location, document } = g;
  if (crypto?.subtle === undefined || indexedDB === undefined || location === undefined) return undefined;
  const pkc = g.PublicKeyCredential;
  const signal = pkc?.signalUnknownCredential;
  return {
    crypto,
    indexedDB,
    rpId: location.hostname,
    rpName,
    ...(g.navigator?.credentials === undefined || pkc === undefined ? {} : { credentials: g.navigator.credentials }),
    hasFocus: () => document?.hasFocus() ?? false,
    now: () => g.performance?.now() ?? Date.now(),
    sleep: (ms) => new Promise((resolve) => { setTimeout(resolve, ms); }),
    ...(signal === undefined ? {} : { signalUnknownCredential: (o) => signal.call(pkc, o) }),
  };
}
