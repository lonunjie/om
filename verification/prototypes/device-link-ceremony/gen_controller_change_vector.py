#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Independent deterministic encoding and transcript vector for the ADR-0216 private ceremony."""

import base64
import hashlib
import hmac
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "cbor"))
from python_canonical import enc


def b(data):
    return {"$bytes": data.hex()}


def mac(key, data):
    return hmac.digest(key, data, "sha256")


def main():
    expiry = 1800000000
    secret = bytes(range(16))
    a_cap, b_recv = bytes([0xA1] * 24), bytes([0xB2] * 24)
    origin = bytes([0x31] * 16)
    uri = b"https://home.example"
    token = bytes([1]) + expiry.to_bytes(4, "big") + secret
    token += bytes([len(a_cap)]) + a_cap + bytes([len(b_recv)]) + b_recv + uri
    uri_token = base64.urlsafe_b64encode(token).rstrip(b"=").decode()
    link = f"om:controller-change/0/{origin.hex()}/{uri_token}"
    binding = enc([
        "om.controller_change.v1", 0,
        b(bytes([0x11] * 32)), b(bytes([0x22] * 32)), b(origin),
        b(bytes([0x42] * 16)), b(bytes([0x53] * 32)), b(bytes([0x64] * 32)),
    ])
    route = enc(["recovery_approval", 1, expiry, b(uri), b(a_cap), b(b_recv)])
    context = enc([b(binding), b(route)])
    label = "OM-RECOVERY-APPROVAL-KDF-v1"
    ceremony_id = hashlib.sha256(enc([label, b(context)])).digest()
    prk = mac(ceremony_id, secret)
    keys = {purpose: mac(prk, enc([label, purpose]) + b"\x01").hex()
            for purpose in ("req", "resp", "code", "bind")}
    proof = enc(["om.controller_change.v1", b(ceremony_id), b(binding)])
    code_mac = mac(bytes.fromhex(keys["code"]), enc(["OM-RECOVERY-APPROVAL-MAC-v1", 1, b(proof)]))
    print(json.dumps({
        "link": link,
        "binding": binding.hex(),
        "ceremony_id": ceremony_id.hex(),
        "keys": keys,
        "proof": proof.hex(),
        "code": f"{int.from_bytes(code_mac[:8], 'big') % 1_000_000:06d}",
    }, indent=2))


if __name__ == "__main__":
    main()
