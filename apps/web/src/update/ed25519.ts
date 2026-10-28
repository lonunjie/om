/** One pinned verification key. `alg` is part of the pin: an algorithm tag alone never grants authority (ADR-0149). */
export interface SignatureVerifier {
  readonly alg: string;
  verify(payload: Uint8Array, sig: Uint8Array): Promise<boolean>;
}

export const fromBase64 = (s: string): Uint8Array | null => {
  try {
    return Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
  } catch {
    return null;
  }
};
export const fromHex = (h: string): Uint8Array => Uint8Array.from(h.match(/../g) ?? [], (b) => parseInt(b, 16));

/** WebCrypto Ed25519 verifier for a raw 32-byte public key (available in Node 24 and current browsers). */
export const ed25519Verifier = (rawPublicKey: Uint8Array): SignatureVerifier => ({
  alg: "ed25519",
  verify: async (payload, sig) => {
    const key = await crypto.subtle.importKey("raw", rawPublicKey as BufferSource, { name: "Ed25519" }, false, ["verify"]);
    return crypto.subtle.verify({ name: "Ed25519" }, key, sig as BufferSource, payload as BufferSource);
  },
});
