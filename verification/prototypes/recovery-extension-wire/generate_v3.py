#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Emit offline ADR-0240–0242 goldens using synthetic keys and fixed signing randomness."""

import argparse
import json
from pathlib import Path

from generate_v2 import (
    ACCOUNT, ACS_SIG, FixtureCrypto, OLD_EPOCH, OLD_ROOT, ORIGIN, PLACE, POLICY_HASH,
    PREFLIGHT, PURPOSE, REQUEST, ROOT, SCOPE, SELECTED, TARGET, TRANSFER, auth_seed,
    b, controller_transcript, credential, digest, ds_hash, enc, entry, hkdf,
    mapping, numeric_map, policy, preimage, sha, signed,
)

ACS_HASH = "OM-ACS-HASH-v1"
CRED_HASH = "OM-DEVICE-CREDENTIAL-HASH-v1"
ORIGIN_OLD, SELECTED_KEY, TARGET_KEY, ORIGIN_NEW = (
    bytes([i]) * 64 for i in (0x81, 0x82, 0x83, 0x84)
)
HOST = ".".join(["a" * 63, "b" * 63, "c" * 63, "d" * 61])


def hex_fields(**fields):
    return {name: value.hex() for name, value in fields.items()}


def acs(generation, previous, commitment, controllers, refs, *, rotated=False, messaging=(), revoked=()):
    fields = {
        0: 1, 1: 1, 2: b(ACCOUNT), 3: generation, 5: [b(d) for d in controllers],
        9: mapping(k0=1), 11: 1,
        13: [mapping(k0=b(d), k1=digest(h)) for d, h in refs],
    }
    if commitment is not None:
        fields[10] = digest(commitment)
    if previous is not None:
        fields[4] = digest(previous)
    if messaging:
        fields[7] = [b(d) for d in messaging]
    if revoked:
        fields[8] = [b(d) for d in revoked]
    if rotated:
        fields[12] = [mapping(k0=b(ORIGIN), k1=1)]
    return enc(numeric_map(fields))


def vote(crypto, seed, device, body, randomness, role=1, label=ACS_SIG):
    return entry(device, role, crypto.sign(seed, preimage(label, role, device, body), randomness))


def controller(crypto, q, q_signed, p, p_signed, policies, creds, refs):
    q_hash, p_hash = (ds_hash(ACS_HASH, state) for state in (q, p))
    old_policy, new_policy = policies[:2]
    child = acs(3, p_hash, ds_hash(POLICY_HASH, new_policy), [ORIGIN, SELECTED], refs, rotated=True)
    child_hash = ds_hash(ACS_HASH, child)
    binding = enc([PURPOSE, 3, b(ACCOUNT), b(p_hash), b(ORIGIN), b(SELECTED), b(refs[1][1]), b(child_hash)])
    route = enc(["recovery_approval", 1, 1800000000, b(b"https://home.example"), b(b"\xa1" * 24), b(b"\xb2" * 24)])
    ceremony_id = ds_hash("OM-RECOVERY-APPROVAL-KDF-v1", enc([b(binding), b(route)]))
    historical = sorted(creds[:2], key=lambda body: ds_hash(CRED_HASH, body))
    union = [*historical, creds[2]]
    proposal = enc(mapping(
        k0=2, k1=b(p_signed), k2=b(child), k3=b(new_policy), k4=b(p_hash), k5=b(child_hash),
        k6=6, k7=7, k8=[[b(d), b(h)] for d, h in refs], k9=b(old_policy), k10=[b(c) for c in union],
    ))
    preflight_input = preimage(PREFLIGHT, 1, ORIGIN, enc([b(ceremony_id), b(sha(proposal))]))
    preflight = enc([3, b(proposal), b(crypto.sign(ORIGIN_NEW, preflight_input, 15))])
    selected_vote = vote(crypto, SELECTED_KEY, SELECTED, child, 16)
    partial = signed(child, selected_vote)
    request_input = preimage(REQUEST, 3, SELECTED, enc([b(ceremony_id), b(sha(preflight)), b(sha(partial))]))
    request = enc([1, b(crypto.sign(SELECTED_KEY, request_input, 17)), b(partial), None, b(sha(preflight))])
    origin_vote = vote(crypto, ORIGIN_NEW, ORIGIN, child, 18)
    head = signed(child, origin_vote, selected_vote)
    reply_body = enc([2, b(head), None])
    reply_input = enc([PURPOSE, "reply", b(ceremony_id), b(sha(request)), b(sha(reply_body))])
    offer = controller_transcript(3)["link"].rsplit("/", 1)[1]
    return {
        "link": f"om:controller-change/3/{ORIGIN.hex()}/{offer}/{child_hash.hex()}",
        **hex_fields(
            predecessor=q_signed, predecessor_hash=q_hash, parent=p_signed, parent_hash=p_hash,
            child=child, binding=binding, ceremony_id=ceremony_id, proposal=proposal,
            preflight_input=preflight_input, preflight=preflight, request_input=request_input,
            request=request, selected_vote=enc(selected_vote), origin_vote=enc(origin_vote),
            reply_input=reply_input, reply=enc([b(reply_body), b(crypto.sign(ORIGIN_NEW, reply_input, 19))]),
        ),
    }


def kit_vectors(crypto, anchor_hash, old_refs):
    read, write, record = (
        hkdf(b"", ROOT, label + SCOPE, 32) for label in (
            b"OM-RECOVERY-RECORD-READ-CAP-v1", b"OM-RECOVERY-RECORD-WRITE-CAP-v1", b"OM-RECOVERY-RECORD-KEY-v1",
        )
    )
    locator = ds_hash("OM-RECOVERY-RECORD-LOCATOR-v1", read)

    def make(host, epoch, port, snapshot, rnd, pin_hash):
        seed = auth_seed(ROOT, epoch)
        current = policy(crypto, seed, epoch)
        commitment = ds_hash(POLICY_HASH, current)
        place = {0: 1, 1: b(ACCOUNT), 2: epoch, 3: b(commitment), 4: b(SCOPE), 5: b(locator), 6: host}
        if port is not None:
            place[7] = port
        body = enc(numeric_map(place))
        cert_input = preimage(PLACE, 2, ACCOUNT, body)
        cert = signed(body, entry(ACCOUNT, 2, crypto.sign(seed, cert_input, rnd)))
        fields = {
            0: 38, 1: 1, 2: b(SCOPE), 3: host, 5: b(read), 6: b(write), 7: b(record),
            8: b(b"\x64" * 32), 10: epoch, 11: mapping(k0=b(ACCOUNT), k1=1, k2=b(pin_hash)), 12: b(cert),
        }
        if port is not None:
            fields[4] = port
        if snapshot:
            fields[9] = b(bytes(range(32)))
        fields_for_kit = fields
        kit = enc(numeric_map(fields_for_kit))
        kit_hash = ds_hash("OM-RECOVERY-KIT-TRANSFER-HASH-v1", enc([b(ACCOUNT), epoch, b(commitment), b(kit)]))
        fields = {0: 19, 1: 2, 2: b(TRANSFER), 3: b(ACCOUNT), 4: b(ORIGIN), 5: b(SELECTED),
                  6: epoch, 7: b(commitment), 8: b(kit_hash)}
        send = enc(numeric_map({**fields, 9: b(kit), 10: b(current)}))
        ack = enc(numeric_map({**fields, 0: 20, 4: b(SELECTED), 5: b(ORIGIN)}))
        ack_binding = enc([
            "OM-RECOVERY-KIT-ACK-BIND-v1", b(ACCOUNT), b(SELECTED), b(ORIGIN), epoch, b(kit_hash), b(TRANSFER),
        ])
        return fields_for_kit, {"kit_length": len(kit), **hex_fields(
            kit=kit, policy=current, place_input=cert_input, certificate=cert,
            kit_digest=kit_hash, send=send, ack=ack, ack_binding=ack_binding,
        )}

    # Reproduce the exact historical maximum-hostname input.
    legacy_fields, legacy = make(HOST, 7, None, False, 44, anchor_hash)
    assert legacy["kit_length"] == 4342

    # A separate verified promotion keeps the maximum-encoded policy epoch.
    epoch = (1 << 64) - 1
    commitment = ds_hash(POLICY_HASH, policy(crypto, auth_seed(ROOT, epoch), epoch))
    before = acs(1, None, commitment, [ORIGIN], old_refs, messaging=[SELECTED])
    pin_hash = ds_hash(ACS_HASH, before)
    promoted = acs(2, pin_hash, commitment, [ORIGIN, SELECTED], old_refs, messaging=[SELECTED])
    signed_promotion = signed(promoted, vote(crypto, ORIGIN_OLD, ORIGIN, promoted, 45))
    _, max_fields = make(HOST, epoch, 65535, True, 46, pin_hash)
    _, no_snapshot = make(HOST, epoch, 65535, False, 46, pin_hash)

    def with_private_data(size, critical):
        ext_id = "3af5254d-9ecb-46b4-867d-4226aab2edaa"
        if critical:
            ext_id = "om.critical.private." + ext_id + ".fixture"

        def encode_kit(data_length):
            return enc(numeric_map({**legacy_fields, 23: {ext_id: mapping(
                k0=critical, k1=b(bytes(data_length)),
            )}}))

        overhead = len(encode_kit(256)) - 256
        result = encode_kit(size - overhead)
        assert len(result) == size
        return result

    cap = with_private_data(8192, False)
    overflow = with_private_data(8193, False)
    critical = with_private_data(8192, True)
    return {
        "KITS3-01": {"max_hostname": HOST, "legacy": legacy,
                     "max_fields": max_fields, "max_port_epoch_without_snapshot": no_snapshot},
        "KITS3-02": hex_fields(
            parent=signed(before, vote(crypto, ORIGIN_OLD, ORIGIN, before, 47)),
            promoted=signed_promotion, promotion_reply_body=enc([2, b(signed_promotion), None]),
        ),
        "KITS3-03": {
            "cap": 8192, "oversize": 8193, "invalid_host": HOST + "d",
            **hex_fields(at_cap_sha256=sha(cap), over_cap_sha256=sha(overflow),
                         unknown_critical_at_cap_sha256=sha(critical)),
        },
        "KITS3-04": {"promotion_ciphertext_cap": 4112, "sealed_delivery_bucket": 16384},
    }


def record_vectors(crypto, checkpoint, checkpoint_signed, policies, creds, refs):
    def hash_state(body):
        return ds_hash(ACS_HASH, body)

    def run(body, current):
        return mapping(k0=b(hash_state(body)), k1=b(current))

    def evidence(head, earlier, current, runs, bodies, dossier=None):
        fields = {0: b(head), 1: [b(s) for s in earlier[-8:]], 2: [b(c) for c in bodies],
                  3: b(current), 4: [b(s) for s in earlier[:-8]], 6: runs}
        if dossier is not None:
            fields[5] = [dossier]
        return enc(numeric_map(fields))

    history = [checkpoint_signed]
    runs = [run(checkpoint, policies[0])]
    previous = checkpoint
    target_ref = (TARGET, ds_hash(CRED_HASH, creds[3]))
    for generation, index in enumerate((1, 1, 2, 2, 3), start=3):
        current = policies[index]
        linked = generation in (4, 5)
        body = acs(
            generation, hash_state(previous), ds_hash(POLICY_HASH, current), [ORIGIN, SELECTED],
            [*refs, target_ref] if linked else refs, rotated=True,
            messaging=[TARGET] if linked else [], revoked=[TARGET] if generation >= 6 else [],
        )
        votes = [vote(crypto, ORIGIN_NEW, ORIGIN, body, 20 + generation)]
        if generation % 2:
            votes.append(vote(crypto, SELECTED_KEY, SELECTED, body, 30 + generation))
            runs.append(run(body, current))
        head = signed(body, *votes)
        if generation == 3:
            first = evidence(head, history, current, runs, creds[1:3])
        if generation == 7:
            complete = evidence(head, history, current, runs, creds[1:])
        history.append(head)
        previous = body

    # A historical W0 recovery successor has no exact request commitment.
    recovery_id = b"\x78" * 16
    recovery_cred = credential(crypto, recovery_id, bytes([0x85]) * 64, 0x95)
    recovery_hash = ds_hash(CRED_HASH, recovery_cred)
    old_seed = auth_seed(ROOT, 9)
    next_policy = policy(crypto, auth_seed(ROOT, 10), 10)
    request = enc(mapping(
        k0=4, k1=1, k2=b(ACCOUNT), k3=9, k4=b(recovery_id),
        k5=digest(recovery_hash), k6=digest(hash_state(previous)), k7=b(b"\x79" * 16),
    ))
    signed_request = signed(request, vote(crypto, old_seed, ACCOUNT, request, 36, 2, "OM-RECOVERY-REQUEST-SIG-v1"))
    child = acs(8, hash_state(previous), ds_hash(POLICY_HASH, next_policy), [recovery_id],
                [(recovery_id, recovery_hash)], revoked=[ORIGIN, SELECTED, TARGET])
    child_signed = signed(child, vote(crypto, old_seed, ACCOUNT, child, 37, 2))
    dossier = mapping(k0=b(hash_state(child)), k1=b(current), k2=b(next_policy), k3=b(signed_request))
    legacy = evidence(child_signed, history, next_policy, [*runs, run(child, next_policy)], [*creds[1:], recovery_cred], dossier)
    return {
        "RK2B-01": hex_fields(checkpoint=enc(mapping(
            k0=39, k1=1, k2=b(ACCOUNT), k3=2, k4=digest(hash_state(checkpoint)),
        )), evidence=first),
        "RK2B-02": hex_fields(evidence=complete),
        "RK2B-03": {"expected": "needs_evidence", **hex_fields(evidence=legacy)},

    }


def maximal_record(crypto, origin, *, version=1):
    """A full signed window with a maximum-size, exact historical credential union."""
    pk = b(crypto.public(TARGET_KEY))
    ik = b(crypto.agreement_public(bytes([0x93]) * 32))
    devices = [(1000 + i).to_bytes(16, "big") for i in range(319)]
    credentials = [
        enc(mapping(k0=7, k1=1, k2=b(device), k3=b(ACCOUNT), k4=1, k5=pk, k6=ik, k7=1))
        for device in devices
    ]
    refs = [(d, ds_hash(CRED_HASH, c)) for d, c in zip(devices, credentials)]
    origin_ref = (ORIGIN, ds_hash(CRED_HASH, origin))
    states, runs = [], []
    previous = None
    for generation in range(1, 66):
        current = enc(mapping(
            k0=3, k1=1, k2=3, k3=[], k4=mapping(k0=1, k1=True), k5=[1, 3, 4],
            k6=mapping(k0=259200, k1=1, k3=True), k7=generation,
        ))
        if generation == 63:
            active = refs[:160]
        elif generation == 65:
            active = refs[160:]
        else:
            active = []
        revoked = devices[:160] if generation >= 64 else []
        body = acs(
            generation, previous, ds_hash(POLICY_HASH, current), [ORIGIN],
            sorted([origin_ref, *active]), messaging=[d for d, _ in active], revoked=revoked,
        )
        states.append(signed(body, vote(crypto, ORIGIN_OLD, ORIGIN, body, generation)))
        previous = ds_hash(ACS_HASH, body)
        runs.append(mapping(k0=b(previous), k1=b(current)))
    evidence = mapping(
        k0=b(states[-1]), k1=[b(state) for state in states[56:64]], k2=[b(c) for c in [origin, *credentials]],
        k3=b(current), k4=[b(state) for state in states[:56]], k6=runs,
    )
    plain = mapping(
        k0=36, k1=version, k2=65 << 20, k3=65, k4=mapping(k0=65, k1=digest(previous)), k5=evidence,
        k6=mapping(k0=[mapping(k0="home.example", k2=b(SCOPE))]), k7=mapping(k0=[]),
    )
    cap = 1048000
    unpadded = enc(plain)
    pad = cap - len(unpadded) - 6  # key 22 and five-byte bstr header
    assert pad > 65535
    padded = enc(numeric_map({**dict(plain["$map"]), 22: b(bytes(pad))}))
    assert len(padded) == cap
    return {"segment": 56, "tail": 8, "head": 1, "runs": 65, "credentials": 320,
            "credential_bytes": 700000, "plaintext": cap,
            "plaintext_sha256": sha(padded).hex()}


def build(crypto):
    creds = [
        credential(crypto, ORIGIN, ORIGIN_OLD, 0x91), credential(crypto, ORIGIN, ORIGIN_NEW, 0x94),
        credential(crypto, SELECTED, SELECTED_KEY, 0x92), credential(crypto, TARGET, TARGET_KEY, 0x93),
    ]
    old_refs = [(d, ds_hash(CRED_HASH, c)) for d, c in zip((ORIGIN, SELECTED), (creds[0], creds[2]))]
    refs = [(ORIGIN, ds_hash(CRED_HASH, creds[1])), old_refs[1]]
    policies = [policy(crypto, auth_seed(root, epoch), epoch) for epoch, root in
                [(OLD_EPOCH, OLD_ROOT), (7, ROOT), (8, ROOT), (9, ROOT)]]
    commitment = ds_hash(POLICY_HASH, policies[0])
    q = acs(1, None, commitment, [ORIGIN, SELECTED], old_refs)
    q_signed = signed(q, vote(crypto, ORIGIN_OLD, ORIGIN, q, 1), vote(crypto, SELECTED_KEY, SELECTED, q, 2))
    p = acs(2, ds_hash(ACS_HASH, q), commitment, [ORIGIN, SELECTED], refs, rotated=True)
    p_signed = signed(p, vote(crypto, ORIGIN_OLD, ORIGIN, p, 14))
    return {
        "CC3B-01": controller(crypto, q, q_signed, p, p_signed, policies, creds, refs),
        "CC3B-02": {"durable_parent": "preflight_verified", "untrusted_parent": "refused"},
        "CC3B-03": {"maximum_credentials": 18, "maximum_preflight_bytes": 100000},
        **kit_vectors(crypto, ds_hash(ACS_HASH, q), old_refs),
        **record_vectors(crypto, p, p_signed, policies, creds, refs),
        "RK2B-04": maximal_record(crypto, creds[0]),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture_crypto", type=Path)
    args = parser.parse_args()
    print(json.dumps(build(FixtureCrypto(args.fixture_crypto)), indent=2))
