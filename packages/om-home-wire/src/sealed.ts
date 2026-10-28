// SPDX-License-Identifier: MPL-2.0
// What a mailbox Home may look at in a deposit: the outer SealedDeliveryEnvelope (object type 23, ADR-0142) or, for
// first contact, the MessageRequest (object type 19, ADR-0126). The Home checks shape and size only; it cannot open
// either, and an outer wrapper never carries a sender AccountID, DeviceID, identity key, ratchet header or ConversationID.
import { CborError, decode } from "./cbor";
import { Obj } from "./fields";

/** Profile-1 ciphertext lengths: plaintext buckets 1024/4096/16384/65536/131072 plus the 16-byte AEAD tag. */
export const PROFILE1_CIPHERTEXT_LENGTHS: readonly number[] = [1040, 4112, 16400, 65552, 131088];
export const PROFILE1_HPKE_ENC_LENGTH = 1120;

export interface SealedEnvelopeView {
  readonly kind: "sealed";
  readonly cryptoProfile: number;
  readonly deliveryKeyId: Uint8Array;
  readonly deliveryId: Uint8Array;
  readonly expiresAt: number;
}
export interface MessageRequestView {
  readonly kind: "message_request";
  readonly ticketId: Uint8Array;
}
export type DepositView = SealedEnvelopeView | MessageRequestView;

/** Parses and bounds-checks a deposit. Throws `CborError` for anything that is neither object; never reads inside ciphertext. */
export function parseDeposit(bytes: Uint8Array): DepositView {
  const o = new Obj(decode(bytes));
  const type = o.uint(0);
  if (o.uint(1) !== 1) throw new CborError("unsupported object version");
  if (type === 23) {
    const cryptoProfile = o.uint(2);
    if (cryptoProfile < 1 || cryptoProfile > 65535) throw new CborError("crypto profile out of range");
    const deliveryKeyId = o.bytes(3, 16, 16);
    const deliveryId = o.bytes(4, 16, 16);
    const enc = o.bytes(5, 32, 2048);
    const ct = o.bytes(6, 17, 131088);
    const expiresAt = o.uint(7);
    o.optExtensions();
    o.done();
    if (cryptoProfile === 1 && (enc.length !== PROFILE1_HPKE_ENC_LENGTH || !PROFILE1_CIPHERTEXT_LENGTHS.includes(ct.length))) throw new CborError("profile-1 lengths");
    return { kind: "sealed", cryptoProfile, deliveryKeyId, deliveryId, expiresAt };
  }
  if (type === 19) {
    const ticketId = o.bytes(2, 16, 16);
    o.digest(3);
    const profile = o.uint(4);
    if (profile < 1 || profile > 65535) throw new CborError("crypto profile out of range");
    o.bytes(5, 32, 2048);
    o.bytes(6, 17, 65552);
    o.optBytes(7, 1, 4096);
    o.optExtensions();
    o.done();
    return { kind: "message_request", ticketId };
  }
  throw new CborError("not a SealedDeliveryEnvelope or MessageRequest");
}
