// Post-quantum co-signature on the release-root delegation (ADR-0182): ML-DSA-65 (FIPS 204), held by ONE software PQ
// root key that lives only in the operator's personal vault. It is a REQUIRED additional signature once the root has
// published a `pqRequired` delegation (or the build itself requires it); the Ed25519 2-of-3 root stays.
// This file holds the pin descriptor, the signed-bytes framing and the verifier. build/verify-release.mjs implements the
// same rules on node:crypto (an independent ML-DSA implementation); build/delegation.test.ts runs shared fixtures through
// both and requires the same verdicts. The only importer of @noble/post-quantum in the client.
import { ml_dsa65 } from "@noble/post-quantum/ml-dsa.js";
import { fromBase64, fromHex } from "./ed25519";
import { sha256Hex } from "./hash";

/** Algorithm tag of the only PQ root algorithm this build knows. An unknown tag, pinned or declared, never verifies. */
export const PQ_ALG = "ml-dsa-65";
/** Framing id (the shape of the signed bytes, below). Pinned with the key; a signature entry cannot choose it. */
export const PQ_FRAME = "om-pq-v1";
export const PQ_KNOWN_ALGS: readonly string[] = [PQ_ALG];
export const PQ_KNOWN_FRAMES: readonly string[] = [PQ_FRAME];
/** FIPS 204 sizes for ML-DSA-65. */
export const PQ_PUBLIC_KEY_BYTES = 1952;
export const PQ_SIGNATURE_BYTES = 3309;
/** Domain-separation label of the signed bytes; versioned with the frame. */
export const PQ_DOMAIN = "om.release-delegation.pq/1";

/** The pinned PQ root: signature entries are matched on `keyId`; `alg` and `frame` must equal these. `pub` is the raw 1952-byte key as lowercase hex. */
export interface PqRootPin {
  readonly keyId: string;
  readonly alg: string;
  readonly pub: string;
  readonly frame: string;
}

const ID = /^[A-Za-z0-9._-]{1,64}$/;
const HEX_PUB = new RegExp(`^[0-9a-f]{${String(PQ_PUBLIC_KEY_BYTES * 2)}}$`);
const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** A pinned descriptor, or null when malformed or when the algorithm or frame is unknown (fail closed). */
export const describePq = (v: unknown): PqRootPin | null => {
  if (!isRecord(v)) return null;
  const { keyId, alg, pub, frame } = v;
  if (typeof keyId !== "string" || !ID.test(keyId) || typeof alg !== "string" || typeof pub !== "string" || typeof frame !== "string") return null;
  return PQ_KNOWN_ALGS.includes(alg) && PQ_KNOWN_FRAMES.includes(frame) && HEX_PUB.test(pub) ? { keyId, alg, pub, frame } : null;
};

/**
 * Bytes the PQ root signs for a delegation whose canonical payload (`delegationPayload`, signatures excluded) is `payload`:
 *   "om.release-delegation.pq/1" 00 | alg tag ASCII | 00 | SHA-256(raw PQ public key) (32 bytes) | payload
 * The label separates this from every Ed25519 root frame; the alg tag and the key fingerprint bind the signature to this
 * algorithm and this pinned key; the payload is last, so the prefix is a fixed, unambiguous header. ML-DSA's own context
 * string is left empty (pure ML-DSA.Sign), so any conforming implementation interoperates.
 */
export const pqSigningBytes = async (pin: PqRootPin, payload: Uint8Array): Promise<Uint8Array> => {
  const head = new TextEncoder().encode(`${PQ_DOMAIN}\0${pin.alg}\0`);
  const fp = fromHex(await sha256Hex(fromHex(pin.pub)));
  const out = new Uint8Array(head.length + fp.length + payload.length);
  out.set(head, 0);
  out.set(fp, head.length);
  out.set(payload, head.length + fp.length);
  return out;
};

/** True only for a well-formed ML-DSA-65 signature by the pinned key over the framed payload. Never throws. */
export const verifyPqSignature = async (pin: PqRootPin, payload: Uint8Array, sigBase64: string): Promise<boolean> => {
  const sig = fromBase64(sigBase64);
  if (sig === null || sig.length !== PQ_SIGNATURE_BYTES) return false;
  try {
    return ml_dsa65.verify(sig, await pqSigningBytes(pin, payload), fromHex(pin.pub));
  } catch {
    return false;
  }
};
