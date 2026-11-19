#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Generate offline Python reference fixtures for the gated recovery extensions."""

import hashlib
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "device-link-ceremony"))
sys.path.insert(0, str(HERE.parent / "recovery-record-wire"))
sys.path.insert(0, str(HERE.parent / "recovery-sheet-wire"))
from gen_controller_change_vector import b, build as controller_transcript, enc
from gen_recovery_sheet_vectors import b32, qr, sheet_form, whole_check
from siv import open_, seal

SCOPE = bytes(range(0xA0, 0xC0))
RELATIONSHIP_KEY = bytes(range(32))
ROOT = bytes(range(0x40, 0x60))


def ds_hash(label, body):
    return hashlib.sha256(enc([label, b(body)])).digest()


def numeric_map(fields):
    return {"$map": list(fields.items())}


def kit_transfer():
    account, source, target = b"\x11" * 32, b"\x22" * 16, b"\x33" * 16
    policy, transfer = b"\x44" * 32, b"\x55" * 16
    epoch = 7
    kit = enc(numeric_map({
        0: 38, 1: 1, 2: b(b"\x60" * 32), 3: "home.example",
        5: b(b"\x61" * 32), 6: b(b"\x62" * 32), 7: b(b"\x63" * 32),
        8: b(b"\x64" * 32), 9: b(b"\x65" * 32), 10: epoch,
        11: numeric_map({0: b(account), 1: 4, 2: b(b"\x66" * 32)}),
    }))
    digest = ds_hash(
        "OM-RECOVERY-KIT-TRANSFER-HASH-v1", enc([b(account), epoch, b(policy), b(kit)])
    )
    fields = {
        0: 19, 1: 1, 2: b(transfer), 3: b(account), 4: b(source), 5: b(target),
        6: epoch, 7: b(policy), 8: b(digest), 9: b(kit),
    }
    send = enc(numeric_map(fields))
    del fields[9]
    fields.update({0: 20, 4: b(target), 5: b(source)})
    ack = enc(numeric_map(fields))
    binding = enc([
        "OM-RECOVERY-KIT-ACK-BIND-v1", b(account), b(target), b(source),
        epoch, b(digest), b(transfer),
    ])
    return {
        "kit": kit.hex(), "kit_digest": digest.hex(), "send": send.hex(),
        "ack": ack.hex(), "ack_binding": binding.hex(),
    }


def relationship_sheet():
    host = b"home.example"
    checkpoint = enc(numeric_map({
        0: 39, 1: 1, 2: b(b"\x11" * 32), 3: 7,
        4: numeric_map({0: 1, 1: b(b"\x22" * 32)}),
    }))
    place = SCOPE + b"\x00\x00" + bytes([len(host)]) + host + checkpoint
    body = b"\x02\x03" + RELATIONSHIP_KEY + place
    binary = body + whole_check(body)
    assert len(binary) == 85 + len(checkpoint)
    compact = b32(binary)
    return {
        "binary": binary.hex(), "compact": compact, "sheet": sheet_form(compact),
        "qr": qr(binary), "checkpoint": checkpoint.hex(), "root": ROOT.hex(),
    }


def snapshot_layer():
    generation = 0x1234
    nonce = bytes(range(12))
    plaintext = enc(numeric_map({0: []}))
    label = "OM-RECOVERY-RELATIONSHIP-SNAPSHOT-AEAD-AD-v1"
    aad = enc([label, b(SCOPE), generation, 1])
    ciphertext = seal(RELATIONSHIP_KEY, nonce, aad, plaintext)
    assert open_(RELATIONSHIP_KEY, nonce, aad, ciphertext) == plaintext
    for wrong_key, wrong_aad in (
        (ROOT, aad),
        (RELATIONSHIP_KEY, enc([label, b(SCOPE), generation + 1, 1])),
        (RELATIONSHIP_KEY, enc([label, b(bytes(32)), generation, 1])),
    ):
        try:
            open_(wrong_key, nonce, wrong_aad, ciphertext)
        except ValueError:
            pass
        else:
            raise AssertionError("opened snapshot with the wrong key or context")
    return {
        "plaintext": plaintext.hex(), "aad": aad.hex(), "nonce": nonce.hex(),
        "ciphertext_and_tag": ciphertext.hex(),
    }


def build():
    return {
        "CC3-01": controller_transcript(3),
        "KITS-01": kit_transfer(),
        "RK2-01": relationship_sheet(),
        "RK2-03": snapshot_layer(),
    }


if __name__ == "__main__":
    print(json.dumps(build(), indent=2))
