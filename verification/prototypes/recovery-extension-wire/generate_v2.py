#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Emit gated fixture bytes. Pass the built fixture-crypto binary as the first argument.

The binary supplies only public keys and raw hybrid signatures from synthetic seeds.
Python constructs all CBOR, hashes, KDF inputs and signed preimages separately.
"""

import argparse
import hashlib
import hmac
import json
from pathlib import Path
import subprocess

from generate import ROOT, b, b32, controller_transcript, ds_hash, enc
from generate import numeric_map, qr, relationship_sheet, sheet_form, whole_check
from gen_recovery_sheet_vectors import valid_host

ACCOUNT = b"\x11" * 32
ORIGIN = b"\x22" * 16
SELECTED = b"\x33" * 16
TARGET = b"\x77" * 16
SCOPE = b"\x60" * 32
TRANSFER = b"\x55" * 16
HOST = "home.example"
OLD_ROOT = bytes([0x80]) * 32
OLD_EPOCH, EPOCH = 6, 7
PURPOSE = "om.controller_change.v1"
PREFLIGHT = "OM-CONTROLLER-OP3-PREFLIGHT-SIG-v1"
REQUEST = "OM-CONTROLLER-OP3-REQUEST-PROOF-SIG-v1"
PLACE = "OM-RECOVERY-KIT-PLACE-BIND-SIG-v1"
ACS_SIG = "OM-ACS-SIG-v1"
POLICY_HASH = "OM-RECOVERY-POLICY-HASH-v1"


def mapping(**fields):
    return numeric_map({int(key[1:]): value for key, value in fields.items()})


def sha(data):
    return hashlib.sha256(data).digest()


def hkdf(salt, ikm, info, length):
    prk = hmac.digest(salt, ikm, "sha256")
    result, previous = b"", b""
    for index in range(1, (length + 31) // 32 + 1):
        previous = hmac.digest(prk, previous + info + bytes([index]), "sha256")
        result += previous
    return result[:length]


def auth_seed(root, epoch):
    return hkdf(b"", root, b"OM-RECOVERY-AUTH-KEY-SEED-v1" + epoch.to_bytes(8, "big"), 64)


class FixtureCrypto:
    def __init__(self, binary):
        self.binary = str(binary.resolve())

    def public(self, seed):
        return bytes.fromhex(subprocess.check_output([self.binary, seed.hex()], text=True))

    def agreement_public(self, seed):
        return bytes.fromhex(subprocess.check_output([self.binary, "--x25519", seed.hex()], text=True))

    def sign(self, seed, preimage, random_byte):
        return bytes.fromhex(subprocess.check_output(
            [self.binary, seed.hex(), (bytes([random_byte]) * 32).hex()], input=preimage
        ).decode())


def preimage(label, role, signer, body):
    return enc([label, 1, role, b(signer), b(body)])


def entry(signer, role, signature):
    return mapping(k0=b(signer), k1=1, k2=role, k3=b(signature))


def signed(body, *entries):
    return enc(mapping(k1=b(body), k2=list(entries)))


def digest(value):
    return mapping(k0=1, k1=b(value))


def policy(crypto, seed, epoch):
    method = mapping(k0=2, k1=b(crypto.public(seed)))
    return enc(mapping(
        k0=3, k1=1, k2=1, k3=[method], k4=mapping(k0=1, k1=True), k5=[1, 3, 4],
        k6=mapping(k0=259200, k1=1, k3=True), k7=epoch,
    ))


def credential(crypto, device, seed, ik_byte):
    return enc(mapping(
        k0=7, k1=1, k2=b(device), k3=b(ACCOUNT), k4=1, k5=b(crypto.public(seed)),
        k6=b(crypto.agreement_public(bytes([ik_byte]) * 32)), k7=1,
    ))


def acs(generation, controllers, refs, policy_hash, previous=None, messaging=(), revoked=()):
    fields = {
        0: 1, 1: 1, 2: b(ACCOUNT), 3: generation, 5: [b(d) for d in controllers],
        9: mapping(k0=1), 10: digest(policy_hash), 11: 1,
        13: [mapping(k0=b(d), k1=digest(h)) for d, h in refs],
    }
    if previous is not None:
        fields[4] = digest(previous)
    if messaging:
        fields[7] = [b(d) for d in messaging]
    if revoked:
        fields[8] = [b(d) for d in revoked]
    return enc(numeric_map(fields))


def build(crypto, adjunct=None):
    origin_seed, selected_seed, target_seed = (bytes([i]) * 64 for i in (0x81, 0x82, 0x83))
    old_seed = auth_seed(OLD_ROOT, OLD_EPOCH)
    current_seed = auth_seed(ROOT, EPOCH)
    old_policy, current_policy = policy(crypto, old_seed, OLD_EPOCH), policy(crypto, current_seed, EPOCH)
    old_hash, current_hash = (ds_hash(POLICY_HASH, p) for p in (old_policy, current_policy))
    origin_cred = credential(crypto, ORIGIN, origin_seed, 0x91)
    selected_cred = credential(crypto, SELECTED, selected_seed, 0x92)
    target_cred = credential(crypto, TARGET, target_seed, 0x93)
    origin_ref, selected_ref, target_ref = (
        (d, ds_hash("OM-DEVICE-CREDENTIAL-HASH-v1", c))
        for d, c in ((ORIGIN, origin_cred), (SELECTED, selected_cred), (TARGET, target_cred))
    )
    refs = [origin_ref, selected_ref]
    controllers = [ORIGIN, SELECTED]
    parent = acs(1, controllers, refs, old_hash)
    parent_hash = ds_hash("OM-ACS-HASH-v1", parent)
    child = acs(2, controllers, refs, current_hash, parent_hash)
    child_hash = ds_hash("OM-ACS-HASH-v1", child)

    def vote(seed, device, body, random_byte):
        return entry(device, 1, crypto.sign(seed, preimage(ACS_SIG, 1, device, body), random_byte))

    signed_parent = signed(parent, vote(origin_seed, ORIGIN, parent, 1), vote(selected_seed, SELECTED, parent, 2))

    legacy = controller_transcript(3)
    fixed_link = {**legacy, "link": legacy["link"] + "/" + "64" * 32}
    offer = legacy["link"].rsplit("/", 1)[1]
    binding = enc([PURPOSE, 3, b(ACCOUNT), b(parent_hash), b(ORIGIN), b(SELECTED), b(selected_ref[1]), b(child_hash)])
    route = enc(["recovery_approval", 1, 1800000000, b(b"https://home.example"), b(b"\xa1" * 24), b(b"\xb2" * 24)])
    ceremony_id = ds_hash("OM-RECOVERY-APPROVAL-KDF-v1", enc([b(binding), b(route)]))
    proposal = enc(numeric_map({
        0: 2, 1: b(signed_parent), 2: b(child), 3: b(current_policy), 4: b(parent_hash),
        5: b(child_hash), 6: OLD_EPOCH, 7: EPOCH, 8: [[b(d), b(h)] for d, h in refs],
        9: b(old_policy), 10: [b(origin_cred), b(selected_cred)],
    }))
    preflight_input = preimage(PREFLIGHT, 1, ORIGIN, enc([b(ceremony_id), b(sha(proposal))]))
    preflight_sig = crypto.sign(origin_seed, preflight_input, 3)
    preflight = enc([3, b(proposal), b(preflight_sig)])
    wrong_origin_sig = crypto.sign(selected_seed, preflight_input, 32)
    wrong_origin_preflight = enc([3, b(proposal), b(wrong_origin_sig)])
    selected_vote = vote(selected_seed, SELECTED, child, 4)
    partial = signed(child, selected_vote)
    request_input = preimage(REQUEST, 3, SELECTED, enc([b(ceremony_id), b(sha(preflight)), b(sha(partial))]))
    request_sig = crypto.sign(selected_seed, request_input, 5)
    request = enc([1, b(request_sig), b(partial), None, b(sha(preflight))])
    origin_vote = vote(origin_seed, ORIGIN, child, 6)
    head = signed(child, origin_vote, selected_vote)
    reply_body = enc([2, b(head), None])
    reply_input = enc([PURPOSE, "reply", b(ceremony_id), b(sha(request)), b(sha(reply_body))])
    reply_sig = crypto.sign(origin_seed, reply_input, 7)
    reply = enc([b(reply_body), b(reply_sig)])

    read_cap, write_cap, record_key = (
        hkdf(b"", ROOT, label.encode() + SCOPE, 32) for label in (
            "OM-RECOVERY-RECORD-READ-CAP-v1", "OM-RECOVERY-RECORD-WRITE-CAP-v1", "OM-RECOVERY-RECORD-KEY-v1",
        )
    )
    locator = ds_hash("OM-RECOVERY-RECORD-LOCATOR-v1", read_cap)

    def place_body(commitment, host=HOST):
        return enc(mapping(
            k0=1, k1=b(ACCOUNT), k2=EPOCH, k3=b(commitment), k4=b(SCOPE), k5=b(locator), k6=host,
        ))

    body = place_body(current_hash)
    place_input = preimage(PLACE, 2, ACCOUNT, body)
    place_sig = crypto.sign(current_seed, place_input, 8)
    certificate = signed(body, entry(ACCOUNT, 2, place_sig))
    kit_fields = {
        0: 38, 1: 1, 2: b(SCOPE), 3: HOST, 5: b(read_cap), 6: b(write_cap),
        7: b(record_key), 8: b(b"\x64" * 32), 10: EPOCH,
        11: mapping(k0=b(ACCOUNT), k1=1, k2=b(parent_hash)), 12: b(certificate),
    }
    kit = enc(numeric_map(kit_fields))
    long_host = ".".join(["a" * 63, "b" * 63, "c" * 63, "d" * 61])
    assert len(long_host) == 253 and valid_host(long_host)
    long_place = place_body(current_hash, long_host)
    long_input = preimage(PLACE, 2, ACCOUNT, long_place)
    long_cert = signed(long_place, entry(ACCOUNT, 2, crypto.sign(current_seed, long_input, 44)))
    long_kit = enc(numeric_map({**kit_fields, 3: long_host, 12: b(long_cert)}))
    kit_digest = ds_hash("OM-RECOVERY-KIT-TRANSFER-HASH-v1", enc([b(ACCOUNT), EPOCH, b(current_hash), b(kit)]))
    transfer = {0: 19, 1: 2, 2: b(TRANSFER), 3: b(ACCOUNT), 4: b(ORIGIN), 5: b(SELECTED),
                6: EPOCH, 7: b(current_hash), 8: b(kit_digest)}
    send = enc(numeric_map({**transfer, 9: b(kit), 10: b(current_policy)}))
    stale_digest = ds_hash("OM-RECOVERY-KIT-TRANSFER-HASH-v1", enc([b(ACCOUNT), OLD_EPOCH, b(current_hash), b(kit)]))
    stale_send = enc(numeric_map({**transfer, 6: OLD_EPOCH, 8: b(stale_digest), 9: b(kit), 10: b(current_policy)}))
    ack = enc(numeric_map({**transfer, 0: 20, 4: b(SELECTED), 5: b(ORIGIN)}))
    ack_binding = enc(["OM-RECOVERY-KIT-ACK-BIND-v1", b(ACCOUNT), b(SELECTED), b(ORIGIN), EPOCH, b(kit_digest), b(TRANSFER)])
    placeholder = bytes([0x44]) * 32
    placeholder_body = place_body(placeholder)
    placeholder_input = preimage(PLACE, 2, ACCOUNT, placeholder_body)
    placeholder_cert = signed(placeholder_body, entry(ACCOUNT, 2, crypto.sign(current_seed, placeholder_input, 9)))
    assert placeholder != current_hash

    # A normal controller transition in the same lineage reaches the head with no dossier.
    ordinary = acs(2, controllers, [*refs, target_ref], old_hash, parent_hash, messaging=[TARGET])
    signed_ordinary = signed(ordinary, vote(origin_seed, ORIGIN, ordinary, 10))
    checkpoint = enc(mapping(k0=39, k1=1, k2=b(ACCOUNT), k3=1, k4=digest(parent_hash)))

    def evidence(head_bytes, credentials, head_policy, dossier=None):
        fields = {0: b(head_bytes), 1: [], 2: [b(c) for c in credentials], 3: b(head_policy), 4: [b(signed_parent)]}
        if dossier is not None:
            fields[5] = [dossier]
        return enc(numeric_map(fields))

    ordinary_evidence = evidence(signed_ordinary, [origin_cred, selected_cred, target_cred], old_policy)
    recovery_request = enc(mapping(
        k0=4, k1=1, k2=b(ACCOUNT), k3=OLD_EPOCH, k4=b(TARGET),
        k5=digest(target_ref[1]), k6=digest(parent_hash), k7=b(b"\x78" * 16),
    ))
    recover_input = preimage("OM-RECOVERY-REQUEST-SIG-v1", 2, ACCOUNT, recovery_request)
    recovery_request_entry = entry(ACCOUNT, 2, crypto.sign(old_seed, recover_input, 11))
    signed_request = signed(recovery_request, recovery_request_entry)
    # Owner recovery advances the epoch on the same root and record lineage.
    recovered_policy = policy(crypto, auth_seed(OLD_ROOT, EPOCH), EPOCH)
    recovered_hash = ds_hash(POLICY_HASH, recovered_policy)
    recovery_child = acs(2, [TARGET], [target_ref], recovered_hash, parent_hash, revoked=controllers)
    recovery_sig = crypto.sign(old_seed, preimage(ACS_SIG, 2, ACCOUNT, recovery_child), 12)
    signed_recovery = signed(recovery_child, entry(ACCOUNT, 2, recovery_sig))
    dossier = mapping(
        k0=b(ds_hash("OM-ACS-HASH-v1", recovery_child)), k1=b(old_policy), k2=b(recovered_policy), k3=b(signed_request),
    )
    recovery_evidence = evidence(signed_recovery, [origin_cred, selected_cred, target_cred], recovered_policy, dossier)

    damaged_request = signed(
        recovery_request[:-1] + bytes([recovery_request[-1] ^ 1]),
        recovery_request_entry,
    )
    damaged_dossier = mapping(
        k0=b(ds_hash("OM-ACS-HASH-v1", recovery_child)), k1=b(old_policy),
        k2=b(recovered_policy), k3=b(damaged_request),
    )

    def hex_fields(**fields):
        return {name: value.hex() for name, value in fields.items()}

    result = {
        "blocked": {
            "CC3A-02-parent-key-rollover": "Parent-state signer may require a predecessor-committed credential.",
            "KITS2-04-max-host": "Certified kit can exceed the existing 4096-byte kit bound.",
            "RK2A-01/RK2A-04-non-genesis-anchor": "Checkpoint signature authority can precede the carried window.",
            "RK2A-01-policy-history": "Intermediate ordinary policy-body carriage is unspecified.",
            "RK2A-02/RK2A-03-exact-request": "W0 child has no commitment to the exact request or nonce.",
            "BOARD2": "ADR-0239 awaits acceptance of the audited Home API limit amendment.",
        },
        "CC3A-01": fixed_link,
        "operation-1-transcript": controller_transcript(1),
        "CC3A-02": {
            "link": f"om:controller-change/3/{ORIGIN.hex()}/{offer}/{child_hash.hex()}",
            **hex_fields(
                binding=binding, ceremony_id=ceremony_id, parent_body=parent, child_body=child, proposal=proposal,
                preflight_input=preflight_input, preflight_signature=preflight_sig, preflight=preflight,
                request_input=request_input, request_signature=request_sig, request=request,
                selected_vote=enc(selected_vote), origin_vote=enc(origin_vote),
                reply_input=reply_input, reply_signature=reply_sig, reply=reply,
            ),
        },
        "CC3A-03": {
            "wrong_origin_preflight": wrong_origin_preflight.hex(),
            "vote_with_response_role": 2,
        },
        "KITS2-01": hex_fields(
            policy=current_policy, policy_digest=current_hash, signed_head=head, read_cap=read_cap,
            write_cap=write_cap, record_key=record_key, locator=locator, place_body=body,
            place_input=place_input, place_signature=place_sig, certificate=certificate,
            kit=kit, kit_digest=kit_digest, send=send, ack=ack, ack_binding=ack_binding,
        ),
        "KITS2-02-placeholder": hex_fields(
            policy_digest=placeholder, place_body=placeholder_body,
            place_input=placeholder_input, certificate=placeholder_cert,
        ),
        "KITS2-02": {
            "stale_sender_epoch": stale_send.hex(),
        },
        "KITS2-04": {
            "max_hostname": long_host,
            "certified_kit_length": len(long_kit),
        },
        "RK2A-01": hex_fields(checkpoint=checkpoint, evidence=ordinary_evidence),
        "RK2A-02": hex_fields(
            checkpoint=checkpoint, signed_request=signed_request, signed_child=signed_recovery,
            dossier=enc(dossier), evidence=recovery_evidence,
        ),
        "RK2A-03": hex_fields(tampered_nonce_dossier=enc(damaged_dossier)),
        "RK2A-05": {
            "root_only_compact": b32(OLD_ROOT + sha(b"OM-RECOVERY-KEY-DISPLAY-CHECK" + OLD_ROOT)[:3]),
        },
        "RK2A-06": hostname_vectors(),
    }
    if adjunct is not None:
        for field in ("read_cap", "write_cap", "record_key", "locator"):
            assert result["KITS2-01"][field] == adjunct[f"KITS2-scalar-{field}"], field
        assert placeholder_body.hex() == adjunct["KITS2-scalar-place_body"]
        assert placeholder_input.hex() == adjunct["KITS2-scalar-DS-SIG-preimage"]
        for field in ("link", "ceremony_id", "code"):
            key = f"CC3A-01-{'unchanged-' if field != 'link' else ''}{field}"
            assert fixed_link[field] == adjunct[key], field
    return result


def hostname_vectors():
    fixture = relationship_sheet()
    historical = bytes.fromhex(fixture["binary"])
    checkpoint = bytes.fromhex(fixture["checkpoint"])
    result = {}
    for host in ("xn--bcher-kva.example", "xn--.example", "-home.example", "Home.example", "home.example."):
        prefix = historical[:68] + bytes([len(host)]) + host.encode("ascii") + checkpoint
        binary = prefix + whole_check(prefix)
        compact = b32(binary)
        result[host] = {
            "valid": valid_host(host), "binary": binary.hex(), "compact": compact,
            "sheet": sheet_form(compact), "qr": qr(binary),
        }
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture_crypto", type=Path)
    parser.add_argument("--encoding-adjunct", type=Path)
    args = parser.parse_args()
    adjunct = json.loads(args.encoding_adjunct.read_text()) if args.encoding_adjunct else None
    print(json.dumps(build(FixtureCrypto(args.fixture_crypto), adjunct), indent=2))
