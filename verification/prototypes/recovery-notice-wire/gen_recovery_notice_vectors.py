#!/usr/bin/env python3
"""ADR-0190 recovery-notice wire vectors. Feasibility evidence, not product code.

Generates positive and negative vectors for RecoveryNotice (object type 34, W1) and, in a separate "structs" list, for the
non-top-level RecoveryDossierEntry (element of key 5 of the KIND_ACS_UPDATE payload). It reuses the V-005 generator as a
module (positive bytes from the unmodified verification/cbor/python_canonical.py encoder, negatives from the same raw
emitter) and its W0 account objects (RecoveryPolicy, RecoveryRequest, RecoveryVeto, AccountControlState, DeviceCredential).

Nested signed objects travel as a bstr holding the SignedEnvelope (registry type {signed: X}); credentials and policies
travel as the canonical body in a bstr (ADR-0190 appendix; the two prototype codecs have no cbor-in-bstr type, so the
bytes of a policy or credential are checked by length only, see the status report).

"Semantic" vectors are wire-valid notices whose refusal rule needs the receiver's own pinned state (ADR-0190 section 2
steps 1 to 5); they carry a semantic_rule field and are positives for the codecs.

Usage: gen_recovery_notice_vectors.py OUT.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "verification/prototypes/v005-encoding/gen"))
import gen_vectors as g  # noqa: E402
from gen_vectors import B, M, det, mdel, mset, py_enc, raw  # noqa: E402

for _t in json.loads((REPO / "spec/registries/om-object-types.json").read_text())["types"]:
    g.TYPES.setdefault(_t["name"], _t)
TYPES = g.TYPES
T0 = g.T0
mget = g.mget
NAME = "RecoveryNotice"
POLICY_MAX = 24576  # MAX_POLICY_BYTES
CRED_MAX = 32768  # DeviceCredential type limit


def senv(type_name, body_value, roles):
    env, _ = g.envelope(type_name, body_value, roles)
    return env


def sb(type_name, body_value, roles):
    """A nested signed object as the bstr the registry type {signed: X} expects."""
    return B(py_enc(senv(type_name, body_value, roles)))


# ------------------------------------------------------------------ ingredients
POLICY = B(py_enc(g.RP_TYPICAL))
POLICY2 = B(py_enc(g.RP_MIN))
REQ_ENV = senv("RecoveryRequest", g.REQ, ["recovery"])
REQ = B(py_enc(REQ_ENV))
ACS_BASE = g.ACS1
ACS_CHILD = mset(mset(g.ACS1, 3, 2), 4, g.ACS1_HASH)


def acs(gen_no, roles=("controller",)):
    return sb("AccountControlState", mset(ACS_BASE, 3, gen_no), list(roles))


BRIDGE = [sb("AccountControlState", g.ACS0, ["controller"]), sb("AccountControlState", g.ACS1, ["controller"])]
CRED1, CRED2 = B(py_enc(g.DC1)), B(py_enc(g.DC_MIN))
VETO = sb("RecoveryVeto", g.VETO, ["controller"])
CHILD = sb("AccountControlState", ACS_CHILD, ["recovery"])
SUCCESSOR = M([0, CHILD], [1, POLICY2], [2, CRED2])


def notice(*pairs):
    return g.top(NAME, *sorted(pairs, key=lambda p: p[0]))


MIN = notice([2, POLICY], [3, REQ])
TYPICAL = notice([2, POLICY], [3, REQ], [4, BRIDGE], [5, [CRED1, CRED2]], [6, SUCCESSOR], [7, [VETO]])
POSITIVES = {
    "typical": TYPICAL,
    "minimal": MIN,
    "bridge-only": notice([2, POLICY], [3, REQ], [4, BRIDGE], [5, [CRED1]]),
    "successor-only": notice([2, POLICY], [3, REQ], [6, SUCCESSOR]),
    "veto-only": notice([2, POLICY], [3, REQ], [7, [VETO]]),
    "bridge-8-states": notice([2, POLICY], [3, REQ], [4, [acs(n) for n in range(8)]]),
    "credentials-16": notice([2, POLICY], [3, REQ], [5, [B(py_enc(mset(g.DC_MIN, 2, B(det(f"d{i}", 16))))) for i in range(16)]]),
    "vetoes-16": notice([2, POLICY], [3, REQ], [7, [VETO] * 16]),
    "empty-optional-arrays": notice([2, POLICY], [3, REQ], [4, []], [5, []], [7, []]),
    "next-policy-minimal-successor": notice([2, POLICY2], [3, REQ], [6, M([0, CHILD], [1, POLICY], [2, CRED1])]),
}
# bounds that CDDL (`.cbor`) cannot see: the bytes are not a policy, only a legal length (cddl n/a)
LENGTH_ONLY = {
    "policy-24576-bytes": notice([2, B(b"\x00" * POLICY_MAX)], [3, REQ]),
    "policy-1-byte": notice([2, B(b"\x00")], [3, REQ]),
}

# ------------------------------------------------------------------ semantic fixtures (wire-valid)
OTHER_ACC = det("other-account", 32)


def req_variant(**kw):
    r = g.REQ
    for k, v in kw.items():
        r = mset(r, int(k[1:]), v)
    return r


OVERSIZE_REQ = mset(g.REQ, 23, M(["org.example.padding", M([0, False], [1, B(b"\x00" * 600)])]))
SEMANTIC = {
    "policy-hash-differs-from-commitment": (notice([2, POLICY2], [3, REQ], [4, BRIDGE]),
        "the policy body must hash (OM-RECOVERY-POLICY-HASH-v1) to the recovery_policy_commitment of the bridged parent_state_hash state; here it is another policy: refuse, no state change"),
    "request-signature-role-controller": (notice([2, POLICY], [3, sb("RecoveryRequest", g.REQ, ["controller"])]),
        "the request must open under the policy's recovery key with signature role recovery; a controller-role signature never counts: refuse"),
    "request-for-another-account": (notice([2, POLICY], [3, sb("RecoveryRequest", req_variant(k2=B(OTHER_ACC)), ["recovery"])]),
        "the request must name the contact's pinned account: refuse"),
    "request-for-another-epoch": (notice([2, POLICY], [3, sb("RecoveryRequest", req_variant(k3=9), ["recovery"])]),
        "the request's recovery epoch must equal the policy's epoch: refuse"),
    "request-for-another-parent": (notice([2, POLICY], [3, sb("RecoveryRequest", req_variant(k6=g.fake_digest("other-parent")), ["recovery"])]),
        "the request's parent_state_hash must be the last bridged state: refuse"),
    "request-target-device-already-enrolled": (notice([2, POLICY], [3, sb("RecoveryRequest", req_variant(k4=B(g.D1)), ["recovery"])]),
        "the target device is already in the pinned head (enrolled by another transition): the request is stale, drop"),
    "request-body-over-512-bytes": (notice([2, POLICY], [3, sb("RecoveryRequest", OVERSIZE_REQ, ["recovery"])]),
        "MAX_REQUEST_BYTES: the request body is bounded at 512 bytes before any parse; here it is over (the registered limit of the type is 16,384): refuse"),
    "bridge-out-of-order": (notice([2, POLICY], [3, REQ], [4, list(reversed(BRIDGE))]),
        "bridge states must chain from the pinned head's hash in ascending order (previous_state_hash of each equals the hash of the one before): refuse"),
    "bridge-of-another-account": (notice([2, POLICY], [3, REQ], [4, [sb("AccountControlState", mset(g.ACS1, 2, B(OTHER_ACC)), ["controller"])]]),
        "bridge states must be of the pinned account: refuse"),
    "bridge-contains-recovery-link": (notice([2, POLICY], [3, REQ], [4, [sb("AccountControlState", g.ACS1, ["recovery"])]]),
        "a recovery link inside the bridge is not followed (it needs its own evidence): the bridge stops before it, no state"),
    "successor-credential-not-the-requested-one": (notice([2, POLICY], [3, REQ], [6, M([0, CHILD], [1, POLICY2], [2, CRED1])]),
        "the credential body must hash (OM-DEVICE-CREDENTIAL-HASH-v1) to the request's new_device_credential: drop the successor, keep the request"),
    "successor-child-signed-by-controller": (notice([2, POLICY], [3, REQ], [6, M([0, sb("AccountControlState", ACS_CHILD, ["controller"]), ], [1, POLICY2], [2, CRED2])]),
        "a recovery-alone successor carries one recovery-role signature from the policy's key: refuse the successor"),
    "veto-for-another-request-digest": (notice([2, POLICY], [3, REQ], [7, [sb("RecoveryVeto", mset(g.VETO, 4, g.fake_digest("another-request")), ["controller"])]]),
        "a veto must name this request digest and epoch: ignore it"),
    "veto-signature-role-recovery": (notice([2, POLICY], [3, REQ], [7, [sb("RecoveryVeto", g.VETO, ["recovery"])]]),
        "a veto counts only under the controller role of a live controller of the pinned head: ignore it"),
}

# ------------------------------------------------------------------ RecoveryDossierEntry (struct vectors)
DOSSIER = M([0, g.fake_digest("recovery-link-child")], [1, POLICY], [2, POLICY2], [3, REQ])


def dossier_vectors():
    out = []

    def add(suffix, expect, error, value, cddl, note=""):
        b = raw(value) if not isinstance(value, bytes) else value
        out.append({"id": f"RecoveryDossierEntry/{suffix}", "struct": "RecoveryDossierEntry", "expect": expect,
                    "error": error, "cddl": cddl, "hex": b.hex(), "note": note})

    add("typical", "accept", None, DOSSIER, "valid")
    add("policies-equal", "accept", None, M([0, g.fake_digest("l")], [1, POLICY], [2, POLICY], [3, REQ]), "valid")
    add("neg/digest-missing", "reject", "missing_field", mdel(DOSSIER, 0), "invalid")
    add("neg/policy-missing", "reject", "missing_field", mdel(DOSSIER, 1), "invalid")
    add("neg/next-policy-missing", "reject", "missing_field", mdel(DOSSIER, 2), "invalid")
    add("neg/request-missing", "reject", "missing_field", mdel(DOSSIER, 3), "invalid")
    add("neg/unknown-key-4-seconds", "reject", "unknown_field", mset(DOSSIER, 4, 3600), "invalid",
        "a dossier never carries seconds or any time (ADR-0190 3.2)")
    add("neg/unknown-key-5-first-seen", "reject", "unknown_field", mset(DOSSIER, 5, T0), "invalid")
    add("neg/digest-alg-2", "reject", "value_range", mset(DOSSIER, 0, M([0, 2], [1, B(det("x", 32))])), "invalid")
    add("neg/digest-31-bytes", "reject", "value_range", mset(DOSSIER, 0, M([0, 1], [1, B(det("x", 31))])), "invalid")
    add("neg/policy-empty", "reject", "value_range", mset(DOSSIER, 1, B(b"")), "n/a")
    add("neg/policy-24577-bytes", "reject", "value_range", mset(DOSSIER, 1, B(b"\x00" * (POLICY_MAX + 1))), "n/a")
    add("neg/next-policy-24577-bytes", "reject", "value_range", mset(DOSSIER, 2, B(b"\x00" * (POLICY_MAX + 1))), "n/a")
    add("neg/request-inline-map", "reject", "wrong_type", mset(DOSSIER, 3, REQ_ENV), "invalid")
    add("neg/request-is-a-veto", "reject", "wrong_object_type", mset(DOSSIER, 3, VETO), "invalid")
    add("neg/policy-wrong-type", "reject", "wrong_type", mset(DOSSIER, 1, "policy"), "invalid")
    add("neg/null-request", "reject", "null_forbidden", mset(DOSSIER, 3, None), "invalid")
    add("neg/unsorted-keys", "reject", "unsorted_keys", {"$rawmap": [DOSSIER["$map"][1], DOSSIER["$map"][0]] + DOSSIER["$map"][2:]}, "n/a")
    add("neg/trailing-bytes", "reject", "trailing_bytes", raw(DOSSIER) + b"\x00", "n/a")
    return out


# ------------------------------------------------------------------ gen
def gen():
    g.VECTORS.clear()
    for k, v in POSITIVES.items():
        g.emit_positive(NAME, k, v, None, k == "typical")
    for k, v in LENGTH_ONLY.items():
        g.emit_positive(NAME, k, v, None, False)
        g.VECTORS[-1]["cddl"] = "n/a"
        g.VECTORS[-1]["note"] = "legal length only: the bytes are not a RecoveryPolicy, so the CDDL tools are not asked"
    ext = mset(MIN, 23, M(["org.example.nice-to-have", M([0, False], [1, M(["hint", "x"])])]))
    g.emit_positive(NAME, "noncritical-extension-preserved", ext, None, False)

    base = MIN
    for suffix, err, mut, cddl in g.body_mutations(NAME, base):
        if suffix == "nonminimal-int":
            mut = mset(base, 0, {"$nonmin": mget(base, 0), "w": 3})
        g.emit_negative(NAME, suffix, err, g.wrap_top(NAME, mut), cddl)
    g.emit_negative(NAME, "trailing-bytes", "trailing_bytes", raw(base) + b"\x00")
    g.emit_negative(NAME, "truncated", "truncated", raw(base)[:-1])
    # over the registered 131,068 bytes: two 65,000-byte paddings so the prototype codecs (64 KiB string limit) reach the
    # same object-size verdict as the product codec
    pad = lambda: M([0, False], [1, B(b"\x00" * 65000)])
    g.emit_negative(NAME, "over-size-limit", "size_limit",
                    raw(mset(base, 23, M(["org.example.padding-a", pad()], ["org.example.padding-b", pad()], ["org.example.padding-c", M([0, False], [1, B(b"\x00" * 1200)])]))))

    def neg(suffix, err, mutated, cddl="invalid"):
        g.emit_negative(NAME, suffix, err, raw(mutated), cddl)

    T = TYPICAL
    # fields that could carry time, and every other unassigned key (key 23 is the only extension point)
    for k, label in ((8, "received-at"), (9, "first-seen-ms"), (10, "expires-at"), (11, "elapsed-seconds"), (12, "activates-at"),
                     (22, "unassigned"), (24, "above-the-extension-key")):
        neg(f"unknown-key-{k}-{label}", "unknown_field", mset(MIN, k, T0 if k != 22 else 1))
    neg("time-field-in-successor-key-3", "unknown_field", mset(T, 6, mset(SUCCESSOR, 3, T0)))
    neg("seconds-field-in-successor-key-4", "unknown_field", mset(T, 6, mset(SUCCESSOR, 4, 259200)))
    neg("object-type-is-request", "wrong_object_type", mset(MIN, 0, 4))
    # policy (key 2)
    neg("policy-missing", "missing_field", mdel(MIN, 2))
    neg("policy-null", "null_forbidden", mset(MIN, 2, None))
    neg("policy-wrong-type-map", "wrong_type", mset(MIN, 2, g.RP_TYPICAL))
    neg("policy-empty", "value_range", mset(MIN, 2, B(b"")), "n/a")
    neg("policy-24577-bytes", "value_range", mset(MIN, 2, B(b"\x00" * (POLICY_MAX + 1))), "n/a")
    # request (key 3)
    neg("request-missing", "missing_field", mdel(MIN, 3))
    neg("request-null", "null_forbidden", mset(MIN, 3, None))
    neg("request-inline-envelope-map", "wrong_type", mset(MIN, 3, REQ_ENV))
    neg("request-not-a-bstr-text", "wrong_type", mset(MIN, 3, "request"))
    neg("request-is-a-veto-envelope", "wrong_object_type", mset(MIN, 3, VETO))
    neg("request-is-a-state-envelope", "wrong_object_type", mset(MIN, 3, BRIDGE[0]))
    renv = REQ_ENV
    neg("request-envelope-no-signatures", "value_range", mset(MIN, 3, B(raw(mset(renv, 2, [])))))
    neg("request-envelope-unknown-key", "unknown_field", mset(MIN, 3, B(raw(mset(renv, 3, 0)))))
    neg("request-envelope-signature-missing", "missing_field", mset(MIN, 3, B(raw(mdel(renv, 2)))))
    neg("request-envelope-body-trailing-bytes", "trailing_bytes", mset(MIN, 3, B(raw(g.envelope("RecoveryRequest", None, ["recovery"], body_bytes=raw(g.REQ) + b"\x00")[0]))), "n/a")
    sig = mget(renv, 2)[0]
    neg("request-signature-role-7", "value_range", mset(MIN, 3, B(raw(mset(renv, 2, [mset(sig, 2, 7)])))))
    neg("request-signature-profile-0", "value_range", mset(MIN, 3, B(raw(mset(renv, 2, [mset(sig, 1, 0)])))))
    neg("request-inner-body-missing-field", "missing_field", mset(MIN, 3, B(raw(g.envelope("RecoveryRequest", None, ["recovery"], body_bytes=raw(mdel(g.REQ, 6)))[0]))))
    neg("request-inner-body-unknown-field", "unknown_field", mset(MIN, 3, B(raw(g.envelope("RecoveryRequest", None, ["recovery"], body_bytes=raw(mset(g.REQ, 8, T0)))[0]))))
    # bridge (key 4): count 0..8
    neg("bridge-9-states", "value_range", mset(MIN, 4, [acs(n) for n in range(9)]))
    neg("bridge-16-states", "value_range", mset(MIN, 4, [acs(n) for n in range(16)]))
    neg("bridge-null", "null_forbidden", mset(MIN, 4, None))
    neg("bridge-not-an-array", "wrong_type", mset(MIN, 4, BRIDGE[0]))
    neg("bridge-element-inline-map", "wrong_type", mset(MIN, 4, [senv("AccountControlState", g.ACS1, ["controller"])]))
    neg("bridge-element-is-a-veto", "wrong_object_type", mset(MIN, 4, [VETO]))
    neg("bridge-state-envelope-no-signatures", "value_range", mset(MIN, 4, [B(raw(mset(senv("AccountControlState", g.ACS1, ["controller"]), 2, [])))]))
    # credentials (key 5): count 0..16, each 1..32768 bytes
    neg("credentials-17", "value_range", mset(MIN, 5, [CRED2] * 17))
    neg("credential-empty", "value_range", mset(MIN, 5, [B(b"")]), "n/a")
    neg("credential-32769-bytes", "value_range", mset(MIN, 5, [B(b"\x00" * 32769)]), "n/a")
    neg("credential-wrong-type-text", "wrong_type", mset(MIN, 5, ["credential"]))
    neg("credentials-null", "null_forbidden", mset(MIN, 5, None))
    # successor (key 6)
    neg("successor-not-a-map", "wrong_type", mset(T, 6, CHILD))
    neg("successor-null", "null_forbidden", mset(T, 6, None))
    neg("successor-child-missing", "missing_field", mset(T, 6, mdel(SUCCESSOR, 0)))
    neg("successor-next-policy-missing", "missing_field", mset(T, 6, mdel(SUCCESSOR, 1)))
    neg("successor-credential-missing", "missing_field", mset(T, 6, mdel(SUCCESSOR, 2)))
    neg("successor-child-is-a-request-envelope", "wrong_object_type", mset(T, 6, mset(SUCCESSOR, 0, REQ)))
    neg("successor-child-inline-map", "wrong_type", mset(T, 6, mset(SUCCESSOR, 0, senv("AccountControlState", ACS_CHILD, ["recovery"]))))
    neg("successor-next-policy-empty", "value_range", mset(T, 6, mset(SUCCESSOR, 1, B(b""))), "n/a")
    neg("successor-next-policy-24577-bytes", "value_range", mset(T, 6, mset(SUCCESSOR, 1, B(b"\x00" * (POLICY_MAX + 1)))), "n/a")
    neg("successor-credential-32769-bytes", "value_range", mset(T, 6, mset(SUCCESSOR, 2, B(b"\x00" * 32769))), "n/a")
    # vetoes (key 7): count 0..16
    neg("vetoes-17", "value_range", mset(MIN, 7, [VETO] * 17))
    neg("vetoes-null", "null_forbidden", mset(MIN, 7, None))
    neg("veto-element-is-a-request-envelope", "wrong_object_type", mset(MIN, 7, [REQ]))
    neg("veto-element-inline-map", "wrong_type", mset(MIN, 7, [senv("RecoveryVeto", g.VETO, ["controller"])]))
    # extensions
    neg("extensions-key-23-not-a-map", "wrong_type", mset(MIN, 23, [1]))

    for key, (obj, rule) in SEMANTIC.items():
        before = len(g.VECTORS)
        g.emit_positive(NAME, "semantic-" + key, obj, None, False)
        g.VECTORS[-1]["semantic_rule"] = rule
        assert len(g.VECTORS) == before + 1
    return g.VECTORS


def main(out):
    vs = [v for v in gen() if v is not None]
    ids = [v["id"] for v in vs]
    assert len(ids) == len(set(ids)), "duplicate vector ids"
    structs = dossier_vectors()
    Path(out).write_text(json.dumps({"vectors": vs, "structs": structs}, indent=1) + "\n")
    pos = sum(v["expect"] == "accept" for v in vs)
    print(f"wrote {len(vs)} vectors ({pos} positive, {len(vs) - pos} negative) and {len(structs)} struct vectors to {out}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1])
