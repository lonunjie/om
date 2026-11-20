#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Reproduce offline ADR-0244–0248 evidence with synthetic fixture keys."""

import argparse
import base64
import json
from pathlib import Path

from generate_v2 import (
    ACCOUNT, OLD_EPOCH, OLD_ROOT, ORIGIN, POLICY_HASH, PREFLIGHT, ROOT, SCOPE,
    SELECTED, TARGET, FixtureCrypto, auth_seed, b, credential, digest, ds_hash,
    enc, hkdf, mapping, numeric_map, policy, preimage, sha, signed,
)
from generate_v3 import (
    ACS_HASH, CRED_HASH, HOST, ORIGIN_NEW, ORIGIN_OLD, SELECTED_KEY, TARGET_KEY,
    acs, controller, hex_fields, kit_vectors, maximal_record, vote,
)

from siv import seal

CERTIFIED_TYPE = 45  # Provisional, ADR-0246.
ENVELOPE_PIN = "OM-ACS-SIGNED-ENVELOPE-PIN-v1"  # Provisional, ADR-0247.
RECORD_AD = "OM-RECOVERY-RECORD-AEAD-AD-v2"  # Provisional, ADR-0248.
REQUEST_SIG = "OM-RECOVERY-REQUEST-SIG-v1"
REQUEST_HASH = "OM-RECOVERY-REQUEST-HASH-v1"
FIXTURE_HOST = "home.example"


def changed(value, **fields):
    entries = dict(value["$map"])
    entries.update({int(key[1:]): item for key, item in fields.items()})
    return numeric_map(entries)


def off_policy(epoch):
    return enc(mapping(
        k0=3, k1=1, k2=3, k3=[], k4=mapping(k0=1, k1=True), k5=[1, 3, 4],
        k6=mapping(k0=259200, k1=1, k3=True), k7=epoch,
    ))


def ref(device_id, body):
    return device_id, ds_hash(CRED_HASH, body)


def commitment(body):
    return ds_hash(POLICY_HASH, body) if body is not None else None


def state_hash(body):
    return ds_hash(ACS_HASH, body)


def case_bytes(wire, expected):
    return {"size": len(wire), "sha256": sha(wire).hex(), "expected": expected}


def op3_vectors(crypto, q, q_signed, p, p_signed, policies, creds, refs, old_refs):
    wire = controller(crypto, q, q_signed, p, p_signed, policies, creds, refs)
    historical, current, selected, extra = creds
    wrong_old = credential(crypto, ORIGIN, TARGET_KEY, 0x93)
    old_hash, current_hash = [ds_hash(CRED_HASH, x) for x in (historical, current)]
    ceremony_id = bytes.fromhex(wire["ceremony_id"])
    raw = lambda key: bytes.fromhex(wire[key])
    proposal = mapping(
        k0=2, k1=b(p_signed), k2=b(raw("child")), k3=b(policies[1]), k4=b(state_hash(p)),
        k5=b(state_hash(raw("child"))), k6=6, k7=7, k8=[[b(d), b(h)] for d, h in refs],
        k9=b(policies[0]), k10=[],
    )

    def sorted_bodies(pairs):
        return [b(body) for _, body in sorted(pairs, key=lambda pair: ref(*pair))]

    valid = [(ORIGIN, historical), (ORIGIN, current), (SELECTED, selected)]

    def preflight(changed_proposal, key=ORIGIN_NEW):
        body = enc(changed_proposal)
        proof = preimage(PREFLIGHT, 1, ORIGIN, enc([b(ceremony_id), b(sha(body))]))
        return enc([3, b(body), b(crypto.sign(key, proof, 15))])

    def mutate(bodies, key=ORIGIN_NEW):
        return preflight(changed(proposal, k10=sorted_bodies(bodies)), key)

    assert mutate(valid) == raw("preflight")
    wrong_parent = signed(p, vote(crypto, ORIGIN_NEW, ORIGIN, p, 14))
    cases = {
        "wrong_old": mutate([(ORIGIN, wrong_old), *valid[1:]]),
        "old_current_swap": preflight(changed(proposal, k1=b(wrong_parent), k10=sorted_bodies(valid)), ORIGIN_OLD),
        "omit_old": mutate(valid[1:]),
        "collapse_keep_old": mutate([valid[0], valid[2]]),
        "extra": mutate([*valid, (TARGET, extra)]),
    }
    extra_devices = [(1000 + i).to_bytes(16, "big") for i in range(16)]
    nineteen = valid + [
        (d, credential(crypto, d, TARGET_KEY, 0x93)) for d in extra_devices
    ]
    ordered = sorted_bodies(valid)
    oversized = preflight(changed(proposal, k10=[*ordered[:2], b(bytes(100_000))]))
    child = raw("child")
    bad_proof = bytearray(raw("request"))
    bad_proof[5] ^= 1
    return {
        "CC3C-01": {
            "transcript": "recovery-extension-v3.json#CC3B-01",
            "accepted_parent": "vote_permitted_historical_bodies_unverified",
            "untrusted_predecessor": "refused",
            "preflight_sha256": sha(raw("preflight")).hex(),
            "variants_without_historical_authority": {"wrong_old": "vote_permitted", "omit_old": "vote_permitted"},
        },
        "CC3C-02": {
            **hex_fields(old_digest=old_hash, current_digest=current_hash),
            "mutations": {name: case_bytes(data, "refused") for name, data in cases.items()},
        },
        "CC3C-03": {
            **hex_fields(parent_hash=state_hash(p), predecessor_hash=state_hash(q), signer_digest=old_refs[0][1]),
            "verified_receipt": "historical_membership_verified",
            "invalid_receipts": ["forged", "transplanted", "stale", "created_from_preflight"],
            "fallback": "requires_independently_accepted_parent",
        },
        "CC3C-04": {
            "maximum_credentials": 18, "maximum_preflight_bytes": 100_000,
            "mutations": {
                "unsorted": case_bytes(preflight(changed(proposal, k10=ordered[::-1])), "refused"),
                "duplicate": case_bytes(preflight(changed(proposal, k10=[ordered[0], *ordered])), "refused"),
                "malformed_body": case_bytes(preflight(changed(proposal, k10=[b(b"\xa0"), *ordered[1:]])), "refused"),
                "nineteen": case_bytes(mutate(nineteen), "refused"),
                "preflight_over_cap": case_bytes(oversized, "refused"),
                "old_proof_key": case_bytes(mutate(valid, ORIGIN_OLD), "refused"),
                "wrong_child_proof": case_bytes(bytes(bad_proof), "refused"),
            },
            "contexts": ["stale_parent", "forked_parent"],
            "fixed_link_and_code": base_link(),
            "child_hash": state_hash(child).hex(),
        },
    }, changed(proposal, k10=ordered), ceremony_id


def base_link():
    path = Path(__file__).resolve().parents[2] / "vectors/recovery-extension-v2.json"
    return json.loads(path.read_text())["CC3A-01"]


def recovery_parent(crypto, q, q_signed, policies, target, proposal, ceremony_id):
    old_seed = auth_seed(OLD_ROOT, OLD_EPOCH)
    request_body = mapping(
        k0=4, k1=1, k2=b(ACCOUNT), k3=OLD_EPOCH, k4=b(TARGET),
        k5=digest(ds_hash(CRED_HASH, target)), k6=digest(state_hash(q)), k7=b(b"\x79" * 16),
    )

    def request(body, random_byte, seed=old_seed):
        raw = enc(body)
        return signed(raw, vote(crypto, seed, ACCOUNT, raw, random_byte, 2, REQUEST_SIG))

    original = request(request_body, 61)
    new_nonce = request(changed(request_body, k7=b(b"\x7a" * 16)), 62)
    bad_target = request(changed(request_body, k4=b(b"\x78" * 16)), 63)
    bad_epoch = request(changed(request_body, k3=7), 64)
    wrong_method = request(request_body, 65, auth_seed(ROOT, 7))
    request_digest = ds_hash(REQUEST_HASH, enc(request_body))
    veto_body = enc(mapping(k0=5, k1=1, k2=b(ACCOUNT), k3=OLD_EPOCH, k4=digest(request_digest)))
    veto = signed(veto_body, vote(crypto, ORIGIN_OLD, ORIGIN, veto_body, 65, label="OM-RECOVERY-VETO-SIG-v1"))
    p = acs(2, state_hash(q), commitment(policies[1]), [TARGET], [ref(TARGET, target)],
            revoked=[ORIGIN, SELECTED])
    recovery_vote = vote(crypto, old_seed, ACCOUNT, p, 66, 2)
    cosigner_vote = vote(crypto, ORIGIN_OLD, ORIGIN, p, 67)
    p_signed = signed(p, cosigner_vote, recovery_vote)
    p_solo = signed(p, recovery_vote)
    wrong_parent_method = signed(p, cosigner_vote, vote(crypto, auth_seed(ROOT, 7), ACCOUNT, p, 66, 2))
    carrier = mapping(k0=b(q_signed), k1=b(policies[0]), k2=b(original), k3=b(target))
    borrowed = changed(carrier, k2=b(new_nonce))
    over = changed(carrier, k2=b(bytes(100_000)))
    over_body = enc(changed(proposal, k1=b(p_signed), k11=over))
    proof = preimage(PREFLIGHT, 1, ORIGIN, enc([b(ceremony_id), b(sha(over_body))]))
    over_wire = enc([3, b(over_body), b(crypto.sign(ORIGIN_NEW, proof, 15))])
    fake_credential = credential(crypto, ACCOUNT, TARGET_KEY, 0x93)
    return {
        "CC3D-01": {"expected": "anchor_exact_vote_blocked_ineligible_parties", **hex_fields(parent=p_signed)},
        "CC3D-02": {
            "expected": "historical_recovery_authorized",
            **hex_fields(predecessor=q_signed, carrier=enc(carrier), request=original,
                         request_digest=request_digest, recovery_only_parent=p_solo),
        },
        "CC3D-03": {
            "expected": "refused",
            **hex_fields(nonce_substitution=new_nonce, wrong_target=bad_target, wrong_epoch=bad_epoch,
                         wrong_method=wrong_method, changed_carrier=enc(borrowed), veto=veto,
                         borrowed_wait_carrier=enc(changed(carrier, k4=259200)),
                         fake_account_credential=fake_credential, wrong_parent_method=wrong_parent_method),
            "contexts": ["missing_independent_q", "missing_original_request_identity", "borrowed_wait", "fork"],
        },
        "CC3D-04": {
            "optional_key": 11,
            "oversize_preflight": case_bytes(over_wire, "refused_before_decode"),
            "fixed_link_and_code": base_link(),
        },
    }


def certified_kit(crypto, anchor_hash, refs):
    legacy = kit_vectors(crypto, anchor_hash, refs)["KITS3-01"]["legacy"]
    # Rebuild the fixed 4,342-byte specimen's kit and carrier without changing the certificate.
    keys = [hkdf(b"", ROOT, label + SCOPE, 32) for label in (
        b"OM-RECOVERY-RECORD-READ-CAP-v1", b"OM-RECOVERY-RECORD-WRITE-CAP-v1",
        b"OM-RECOVERY-RECORD-KEY-v1",
    )]
    fields = {
        0: CERTIFIED_TYPE, 1: 1, 2: b(SCOPE), 3: HOST, 5: b(keys[0]), 6: b(keys[1]), 7: b(keys[2]),
        8: b(b"\x64" * 32), 10: 7, 11: mapping(k0=b(ACCOUNT), k1=1, k2=b(anchor_hash)),
        12: b(bytes.fromhex(legacy["certificate"])),
    }
    raw_policy = bytes.fromhex(legacy["policy"])
    base = {
        0: 19, 1: 2, 2: b(b"\x55" * 16), 3: b(ACCOUNT), 4: b(ORIGIN), 5: b(SELECTED),
        6: 7, 7: b(commitment(raw_policy)),
    }

    def delivery(kit):
        raw_kit = enc(kit)
        kit_digest = ds_hash("OM-RECOVERY-KIT-TRANSFER-HASH-v1", enc([
            b(ACCOUNT), 7, base[7], b(raw_kit),
        ]))
        send = enc(numeric_map({**base, 8: b(kit_digest), 9: b(raw_kit), 10: b(raw_policy)}))
        ack = enc(numeric_map({**base, 0: 20, 4: b(SELECTED), 5: b(ORIGIN), 8: b(kit_digest)}))
        binding = enc(["OM-RECOVERY-KIT-ACK-BIND-v1", b(ACCOUNT), b(SELECTED), b(ORIGIN),
                       7, b(kit_digest), base[2]])
        return hex_fields(kit=raw_kit, kit_digest=kit_digest, send=send, ack=ack, ack_binding=binding)

    def boundary(size, critical=False):
        ext = "3af5254d-9ecb-46b4-867d-4226aab2edaa"
        if critical:
            ext = "om.critical.private." + ext + ".fixture"

        def with_extension(length):
            return numeric_map({**fields, 23: {ext: mapping(k0=critical, k1=b(bytes(length)))}})

        overhead = len(enc(with_extension(256))) - 256
        kit = with_extension(size - overhead)
        assert len(enc(kit)) == size
        return kit

    positive = delivery(numeric_map(fields))
    assert enc(numeric_map({**fields, 0: 38})).hex() == legacy["kit"]
    at_cap = boundary(8192)
    cert = bytearray.fromhex(legacy["certificate"])
    cert[-1] ^= 1
    invalid = {
        "hostname": {3: HOST + "d"},
        "signature": {12: b(cert)},
        "version": {1: 2},
    }
    return {
        "KITS4-01": {"type38": legacy["kit"], "type45": positive, "hostname": HOST,
                     "size": 4342, "type38_expected": "refused_before_decode", "type45_expected": "admitted"},
        "KITS4-02": {
            "at_cap": delivery(at_cap),
            "over_cap": case_bytes(enc(boundary(8193)), "refused_before_decode"),
            "unknown_critical": case_bytes(enc(boundary(8192, True)), "refused"),
            "invalid_hostname": HOST + "d",
            "invalid": {
                name: case_bytes(enc(numeric_map({**fields, **replacement})), "refused")
                for name, replacement in invalid.items()
            },
        },
        "KITS4-03": {"promotion_ciphertext_cap": 4112, "promotion_kit_slot": None,
                     "sealed_kit_minimum": 4358, "vault_before_ack": True},
        "KITS4-04": {"legacy_cap": 4096, "certified_cap": 8192, "sealed_delivery_bucket": 16384},
    }


def checkpoint(body, generation):
    return enc(mapping(k0=39, k1=1, k2=b(ACCOUNT), k3=generation, k4=digest(state_hash(body))))


def sheets(crypto, p, p_signed, p_child, creds):
    pin = ds_hash(ENVELOPE_PIN, p_signed)
    ck = checkpoint(p, 2)

    def sheet(version, kind, secret):
        data = bytes([version, kind]) + secret + SCOPE + b"\0\0" + bytes([len(FIXTURE_HOST)])
        data += FIXTURE_HOST.encode() + ck
        if version == 3:
            data += pin
        return data + ds_hash("OM-RECOVERY-SHEET-CHECK-v1", data)[:4]

    def forms(data):
        return {
            "binary": data.hex(),
            "base32": base64.b32encode(data).decode().rstrip("="),
            "qr": "om:recover/" + base64.urlsafe_b64encode(data).decode().rstrip("="),
        }

    original = vote(crypto, ORIGIN_OLD, ORIGIN, p, 14)
    extra = vote(crypto, SELECTED_KEY, SELECTED, p, 68)
    tampered = vote(crypto, ORIGIN_OLD, ORIGIN, p, 14)
    signature = bytes.fromhex(dict(tampered["$map"])[3]["$bytes"])
    tampered = changed(tampered, k3=b(bytes([signature[0] ^ 1]) + signature[1:]))
    variants = {
        "signature_byte": signed(p, tampered),
        "signature_added": signed(p, original, extra),
        "signature_reordered": signed(p, extra, original),
        "signature_removed": p_signed,
        "signature_duplicate": signed(p, original, original),
        "signed_again": signed(p, vote(crypto, ORIGIN_OLD, ORIGIN, p, 69)),
    }
    child = signed(p_child, vote(crypto, ORIGIN_NEW, ORIGIN, p_child, 70))
    return {
        "RK2D-01": {
            **hex_fields(checkpoint=ck, signed_envelope=p_signed, body_hash=state_hash(p), envelope_pin=pin),
            "root": forms(sheet(3, 1, ROOT)), "relationship": forms(sheet(3, 3, bytes(range(32)))),
        },
        "RK2D-02": {name: ds_hash(ENVELOPE_PIN, wire).hex() for name, wire in variants.items()},
        "RK2D-03": {"legacy_root": forms(sheet(2, 1, ROOT)),
                     "legacy_relationship": forms(sheet(2, 3, bytes(range(32)))),
                     "v3_v2_pair": "refused"},
        "RK2D-04": {"after_restart": "exact_anchor", **hex_fields(signed_child=child, rotated_credential=creds[1])},
    }


def records(crypto, credentials, refs, target, current):
    def make_pair(start, end, epoch):
        q = acs(1, None, commitment(start), [ORIGIN, SELECTED], refs)
        add_target = start == end
        p = acs(2, state_hash(q), commitment(end), [ORIGIN, SELECTED],
                [*refs, ref(TARGET, target)] if add_target else refs,
                messaging=[TARGET] if add_target else [])
        sign = lambda body, r: signed(body, vote(crypto, ORIGIN_OLD, ORIGIN, body, r),
                                      vote(crypto, SELECTED_KEY, SELECTED, body, r + 1))
        runs = []
        if start is not None:
            runs.append(mapping(k0=b(state_hash(q)), k1=b(start)))
        if end is not None and end != start:
            runs.append(mapping(k0=b(state_hash(p)), k1=b(end)))
        bodies = [*credentials, target] if add_target else credentials
        fields = {0: b(sign(p, 73)), 1: [b(sign(q, 71))], 2: list(map(b, bodies)),
                  3: b(end) if end else None}
        if runs:
            fields[6] = runs
        evidence = numeric_map(fields)
        plain = mapping(
            k0=36, k1=2, k2=2 << 20, k3=epoch, k4=mapping(k0=2, k1=digest(state_hash(p))),
            k5=evidence, k6=mapping(k0=[mapping(k0=FIXTURE_HOST, k2=b(SCOPE))]), k7=mapping(k0=[]),
        )
        return checkpoint(q, 1), evidence, plain

    def envelope(plain, nonce, version):
        size = 65_536
        empty_padding = enc(changed(plain, k22=b(bytes(256))))
        padded = enc(changed(plain, k22=b(bytes(size - len(empty_padding) + 256))))
        assert len(padded) == size
        key = hkdf(b"", ROOT, b"OM-RECOVERY-RECORD-KEY-v1" + SCOPE, 32)
        read = hkdf(b"", ROOT, b"OM-RECOVERY-RECORD-READ-CAP-v1" + SCOPE, 32)
        locator = ds_hash("OM-RECOVERY-RECORD-LOCATOR-v1", read)
        label = RECORD_AD if version == 2 else "OM-RECOVERY-RECORD-AEAD-AD-v1"
        ad = enc([label, b(SCOPE), b(locator), version])
        ct = seal(key, nonce, ad, padded)
        return mapping(k0=35, k1=2, k2=1, k3=b(nonce), k4=b(ct)), ad, sha(padded)

    pin, evidence, plain = make_pair(current, None, OLD_EPOCH)
    absent = make_pair(None, None, 0)
    off = make_pair(None, off_policy(7), 7)
    outer, ad, padded_hash = envelope(plain, b"\xa4" * 12, 2)
    wrong_ad, _, _ = envelope(plain, b"\xa5" * 12, 1)

    def mutate(plain, evidence, key, *replacement):
        fields = dict(evidence["$map"])
        if replacement:
            fields[key], = replacement
        else:
            del fields[key]
        return enc(changed(plain, k5=numeric_map(fields)))

    rk2e_03 = {
        "missing_key3": (mutate(plain, evidence, 3), "needs_evidence"),
        "null_with_commitment": (mutate(off[2], off[1], 3, None), "integrity_failure"),
        "body_without_commitment": (mutate(plain, evidence, 3, b(current)), "integrity_failure"),
        "fabricated_run": (mutate(absent[2], absent[1], 6, dict(evidence["$map"])[6]), "integrity_failure"),
        "missing_historical_run": (mutate(plain, evidence, 6), "needs_evidence"),
        "bad_nonce": (enc(changed(outer, k3=b(b"\xa6" * 12))), "authentication_failure"),
        "wrong_ad": (enc(wrong_ad), "authentication_failure"),
        "wrong_envelope_version": (enc(changed(outer, k1=1)), "refused"),
        "wrong_plaintext_version": (enc(changed(plain, k1=1)), "refused"),
    }
    return {
        "RK2E-01": {**hex_fields(checkpoint=pin, plaintext=enc(plain), ad=ad, envelope=enc(outer),
                                padded_sha256=padded_hash), "expected": "verified_read_only"},
        "RK2E-02": {
            name: hex_fields(checkpoint=ck, plaintext=enc(value))
            for name, (ck, _, value) in [("no_policy", absent), ("off_explicit", off)]
        },
        "RK2E-03": {name: case_bytes(wire, outcome) for name, (wire, outcome) in rk2e_03.items()},
        "RK2E-04": {**maximal_record(crypto, credentials[0], version=2),
                     "policy_free_writer": "ineligible", "old_reader": "refused"},
    }


def build(crypto):
    old = credential(crypto, ORIGIN, ORIGIN_OLD, 0x91)
    new = credential(crypto, ORIGIN, ORIGIN_NEW, 0x94)
    selected = credential(crypto, SELECTED, SELECTED_KEY, 0x92)
    target = credential(crypto, TARGET, TARGET_KEY, 0x93)
    creds = [old, new, selected, target]
    old_refs = [ref(ORIGIN, old), ref(SELECTED, selected)]
    refs = [ref(ORIGIN, new), old_refs[1]]
    policies = [policy(crypto, auth_seed(root, epoch), epoch) for root, epoch in [(OLD_ROOT, 6), (ROOT, 7)]]
    q = acs(1, None, commitment(policies[0]), [ORIGIN, SELECTED], old_refs)
    q_signed = signed(q, vote(crypto, ORIGIN_OLD, ORIGIN, q, 1), vote(crypto, SELECTED_KEY, SELECTED, q, 2))
    p = acs(2, state_hash(q), commitment(policies[0]), [ORIGIN, SELECTED], refs, rotated=True)
    p_signed = signed(p, vote(crypto, ORIGIN_OLD, ORIGIN, p, 14))
    child = acs(3, state_hash(p), commitment(policies[0]), [ORIGIN, SELECTED],
                [*refs, ref(TARGET, target)], rotated=True, messaging=[TARGET])
    op3, proposal, ceremony_id = op3_vectors(crypto, q, q_signed, p, p_signed, policies, creds, refs, old_refs)
    return {
        "status": "provisional offline verification evidence",
        **op3,
        **recovery_parent(crypto, q, q_signed, policies, target, proposal, ceremony_id),
        **certified_kit(crypto, state_hash(q), old_refs),
        **sheets(crypto, p, p_signed, child, creds),
        **records(crypto, [old, selected], old_refs, target, policies[0]),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture_crypto", type=Path)
    args = parser.parse_args()
    print(json.dumps(build(FixtureCrypto(args.fixture_crypto)), indent=2))
