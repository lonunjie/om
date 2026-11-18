#!/usr/bin/env python3
"""ADR-0203 wire and transcript vectors. All inputs are synthetic."""

import base64
import gzip
import hashlib
import hmac
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/recovery-notice-wire"))
sys.path.insert(0, str(REPO / "verification/prototypes/device-link-ceremony"))
import gen_recovery_notice_vectors as n
from gen_device_link_ceremony_vectors import hkdf

g = n.g
B, M, encode = g.B, g.M, g.py_enc
sha = lambda value: hashlib.sha256(value).digest()
ds = lambda label, body: sha(encode([label, B(body)]))


def make_vectors():
    vectors = []

    def add(name, value, case, error=None):
        vectors.append({
            "id": f"{name}/{case}", "object": name, "form": "bare",
            "expect": "reject" if error else "accept", "error": error,
            "hex": encode(value).hex(), "derived": [],
            **({"value": value} if not error else {}),
        })

    checkpoint = g.top("OwnerRecoveryCheckpoint", [2, B(g.ACC)], [3, 1], [4, g.ACS1_HASH])
    checkpoint_large = g.mset(checkpoint, 3, (1 << 64) - 1)
    assert len(encode(checkpoint)) == 82 and len(encode(checkpoint_large)) == 90
    name = "OwnerRecoveryCheckpoint"
    add(name, checkpoint, "generation-one")
    add(name, checkpoint_large, "max-generation")
    add(name, g.mset(checkpoint, 3, 0), "generation-zero")
    add(name, g.mset(checkpoint, 2, B(bytes(31))), "short-account", "value_range")
    add(name, g.mset(checkpoint, 3, "1"), "noninteger-generation", "wrong_type")
    add(name, g.mdel(checkpoint, 4), "missing-digest", "missing_field")

    target_hash = g.digest_of("OM-DEVICE-CREDENTIAL-HASH-v1", bytes.fromhex(n.CRED2["$bytes"]))
    req = g.mset(g.mset(g.REQ, 4, B(g.D3)), 5, target_hash)
    successor = M([0, n.CHILD], [1, n.POLICY2], [2, n.CRED2])
    proposal = g.top("RecoveryApprovalProposal", [2, n.sb("RecoveryRequest", req, ["recovery"])], [3, successor])
    name = "RecoveryApprovalProposal"
    add(name, proposal, "v1")
    add(name, g.mset(proposal, 1, 2), "unknown-version", "unsupported_version")
    add(name, g.mset(proposal, 0, 8), "crossed-type", "wrong_object_type")
    add(name, g.mdel(proposal, 3), "missing-successor", "missing_field")

    name = "RecoveryApprovalEvidence"
    cosigned = g.mset(successor, 0, n.sb("AccountControlState", n.ACS_CHILD, ["controller", "recovery"]))
    evidence = g.top(name, [2, n.BRIDGE], [3, [n.CRED1, n.CRED2]], [4, []], [5, []], [6, cosigned])
    add(name, evidence, "co-signed")
    add(name, g.mset(g.mdel(evidence, 6), 5, [n.VETO]), "veto-only")
    add(name, g.mset(evidence, 2, []), "empty-chain", "value_range")
    add(name, g.mset(evidence, 2, [n.BRIDGE[0]] * 66), "long-chain", "value_range")
    add(name, g.mset(evidence, 3, [n.CRED1] * 321), "excess-credentials", "value_range")
    add(name, g.mset(evidence, 5, [n.VETO] * 17), "excess-vetoes", "value_range")

    # A multipart shape fixture, independent of whether each credential is useful to the verifier.
    long_credential = B(encode(g.mset(g.DC1, 5, B(g.det("long-signing-key", 256)))))
    multiple = g.mset(evidence, 3, [long_credential] * 150)
    payload = encode(multiple)
    parts = [payload[i:i + 50000] for i in range(0, len(payload), 50000)]
    assert len(parts) == 2
    add(name, multiple, "multipart-shape")
    prefix = "OM-RECOVERY-APPROVAL-"
    evidence_digest = ds(prefix + "EVIDENCE-HASH-v1", payload)
    descriptor = M(
        [0, B(ds("OM-RECOVERY-REQUEST-HASH-v1", encode(req)))],
        [1, g.mget(g.mget(req, 6), 1)], [2, g.mget(g.mget(req, 5), 1)],
        [3, B(g.D1)], [4, B(ds("OM-ACS-HASH-v1", encode(n.ACS_CHILD)))],
        [5, B(evidence_digest)], [6, len(payload)], [7, len(parts)],
    )
    expiry = 1790208300
    secret = g.det("owner-recovery-offer-secret", 16)
    cap_a = g.det("owner-recovery-queue-a", 24)
    cap_b = g.det("owner-recovery-queue-b", 24)
    uri = b"https://home.example"
    kdf = prefix + "KDF-v1"
    context = encode(["recovery_approval", 1, expiry, B(uri), B(cap_a), B(cap_b)])
    transcript = ds(kdf, context)
    keys = {p: hkdf(transcript, secret, encode([kdf, p])) for p in ("req", "resp", "code", "bind")}
    proposal_bytes = encode(proposal)

    def tag(purpose, role, body):
        return hmac.digest(keys[purpose], encode([prefix + "MAC-v1", role, B(body)]), "sha256")

    def bind(desc):
        return tag("bind", 2, encode([B(transcript), B(sha(proposal_bytes)), B(encode(desc))]))

    binding = bind(descriptor)
    name = "RecoveryApprovalReply"
    reply = g.top(name, [2, descriptor], [3, B(binding)])
    add(name, reply, "manifest")
    add(name, g.mset(reply, 2, g.mset(descriptor, 7, 65)), "excess-chunk-count", "value_range")
    add(name, g.mset(reply, 2, g.mset(descriptor, 6, 3200001)), "excess-evidence", "value_range")

    name = "RecoveryApprovalChunk"
    for i, part in enumerate(parts):
        add(name, g.top(name, [2, B(evidence_digest)], [3, i], [4, B(part)]), f"chunk-{i}")
    add(name, g.top(name, [2, B(evidence_digest)], [3, 64], [4, B(b"x")]), "index-out-of-range", "value_range")
    add(name, g.top(name, [2, B(evidence_digest)], [3, 0], [4, B(b"")]), "empty", "value_range")

    host = b"home.example"
    sheet_body = b"\x02\x01" + bytes(32) + bytes([7]) * 32 + b"\x00\x00" + bytes([len(host)]) + host + encode(checkpoint)
    sheet = sheet_body + ds("OM-RECOVERY-SHEET-CHECK-v1", sheet_body)[:4]
    offer = bytes([1]) + expiry.to_bytes(4, "big") + secret + bytes([len(cap_a)]) + cap_a + bytes([len(cap_b)]) + cap_b + uri
    b64 = lambda body: base64.urlsafe_b64encode(body).decode().rstrip("=")
    code = int.from_bytes(tag("code", 1, proposal_bytes)[:8], "big") % 1000000
    return {
        "vectors": vectors,
        "sheet": {
            "binary": sheet.hex(), "compact": base64.b32encode(sheet).decode().rstrip("="),
            "qr": "om:recover/" + b64(sheet),
        },
        "ceremony": {
            "offer": "om:recover-approve/" + b64(offer), "context": context.hex(),
            "transcript": transcript.hex(), "keys": {p: key.hex() for p, key in keys.items()},
            "ad": [encode([prefix + "AEAD-AD-v1", B(transcript), role]).hex() for role in (1, 2)],
            "mailbox": ds(prefix + "MAILBOX-HASH-v1", cap_b)[:16].hex(),
            "code": f"{code:06}", "binding": binding.hex(), "descriptor": encode(descriptor).hex(),
            "evidence": payload.hex(),
            "refusals": [
                {"case": "crossed-purpose", "offer": "om:link/" + b64(offer), "expect": "unsupported"},
                {"case": "old-client", "offer": "om:recover-approve/" + b64(offer), "expect": "unsupported-by-enrollment"},
                {"case": "expired", "now": expiry, "expect": "expired"},
                {"case": "missing-chunk", "indexes": [0], "expect": "incomplete"},
                {"case": "substituted-target", "descriptor": encode(g.mset(descriptor, 2, B(bytes(32)))).hex(), "tag": binding.hex(), "expect": "bad-binding"},
                {"case": "stale-parent", "authenticated_head_generation": 2, "proposal_parent_generation": 1, "expect": "stale"},
            ],
        },
    }


if __name__ == "__main__":
    target = Path(sys.argv[1])
    payload = (json.dumps(make_vectors(), indent=1) + "\n").encode()
    target.write_bytes(gzip.compress(payload, compresslevel=9, mtime=0))
