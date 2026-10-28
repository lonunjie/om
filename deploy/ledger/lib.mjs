// Dependency-free pieces of the Ledger helper, so they are testable without a device (lib.test.mjs).
// The envelope format is the Solana off-chain message, header version 0, message format 0 (restricted ASCII), as parsed by
// LedgerHQ/app-solana libsol/parser.c parse_offchain_message_header. apps/web/build/verify-release.mjs builds the same bytes.
import { createPublicKey, verify } from "node:crypto";

const DOMAIN = Buffer.concat([Buffer.from([0xff]), Buffer.from("solana offchain", "ascii")]); // 16 bytes
const SPKI_ED25519 = Buffer.from("302a300506032b6570032100", "hex");
export const HEADER_LENGTH = 85; // domain 16 + version 1 + app domain 32 + format 1 + signer count 1 + one signer 32 + length 2
export const MAX_TEXT = 1212; // Solana format 0 limit

/** Strictly parses the one envelope shape the ceremony signs. Throws on anything else. */
export const parseEnvelope = (buf) => {
  if (!Buffer.isBuffer(buf) || buf.length <= HEADER_LENGTH) throw new Error("payload is too short to be an off-chain message envelope");
  if (!buf.subarray(0, 16).equals(DOMAIN)) throw new Error("payload does not start with the Solana off-chain signing domain");
  if (buf[16] !== 0) throw new Error("only off-chain header version 0 is supported");
  if (buf[49] !== 0) throw new Error("only message format 0 (restricted ASCII, shown in clear) is supported");
  if (buf[50] !== 1) throw new Error("exactly one signer is expected");
  const len = buf.readUInt16LE(83);
  if (len === 0 || len > MAX_TEXT || buf.length !== HEADER_LENGTH + len) throw new Error("payload length field does not match the payload");
  const text = buf.subarray(HEADER_LENGTH);
  if (!text.every((b) => b >= 0x20 && b <= 0x7e)) throw new Error("message text is not restricted ASCII (0x20..0x7e)");
  return { signerPub: buf.subarray(51, 83).toString("hex"), appDomain: buf.subarray(17, 49).toString("latin1").replace(/\0+$/, ""), text: text.toString("ascii") };
};

export const verifyEd25519 = (pubHex, message, signature) => {
  try {
    const key = createPublicKey({ key: Buffer.concat([SPKI_ED25519, Buffer.from(pubHex, "hex")]), format: "der", type: "spki" });
    return verify(null, message, key, signature);
  } catch { return false; }
};

const B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
/** Base58 (Solana address form) of a public key, so the operator can compare it with what the Ledger shows. */
export const base58 = (buf) => {
  let n = BigInt(`0x${buf.toString("hex") || "0"}`);
  let out = "";
  while (n > 0n) { out = B58[Number(n % 58n)] + out; n /= 58n; }
  for (const b of buf) { if (b !== 0) break; out = `1${out}`; }
  return out;
};

export const PATH_RE = /^44'\/501'(\/[0-9]{1,9}'){1,2}$/;
export const checkPath = (p) => { if (!PATH_RE.test(p)) throw new Error(`derivation path ${JSON.stringify(p)} must look like 44'/501'/0' (hardened, Solana)`); return p; };

/**
 * Signs `envelope` on a device through the injected Solana app object `{ getAddress(path), signOffchainMessage(path, buf) }`.
 * Refuses unless the envelope names the device's own key, and verifies the returned signature locally before returning it.
 */
export const signEnvelope = async (solana, path, envelope) => {
  checkPath(path);
  const { signerPub, text } = parseEnvelope(envelope);
  const { address } = await solana.getAddress(path, false);
  const devicePub = Buffer.from(address).toString("hex");
  if (devicePub !== signerPub) throw new Error(`the payload was prepared for public key ${signerPub} but the device at ${path} has ${devicePub}; wrong device, path or payload (nothing was signed)`);
  const { signature } = await solana.signOffchainMessage(path, envelope);
  const sig = Buffer.from(signature);
  if (sig.length !== 64 || !verifyEd25519(devicePub, envelope, sig)) throw new Error("the device returned a signature that does not verify over the payload; discard it");
  return { pub: devicePub, text, sigBase64: sig.toString("base64"), sigHex: sig.toString("hex") };
};
