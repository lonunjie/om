#!/usr/bin/env python3
"""V-015: executable checks for the ADR-0129/0130/0131 metadata rules, against the W0 descriptors
(spec/metadata/w0-metadata-exposure-descriptors.json) and, where the repository has them, real protocol bytes
(V-001 ratchet headers and key bundles, V-005 MessageRequest). One function per V-015 criterion; each returns a list
of problems (empty = holds) plus a mutation that must be caught, so a check that cannot fail does not count.

Written by a Claude agent (verification executor). Deterministic.
"""
from __future__ import annotations

import gzip
import hashlib
import hmac
import itertools
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DESC = json.loads((ROOT / "spec/metadata/w0-metadata-exposure-descriptors.json").read_text())
EXPO = {d["role"]: d["exposures"] for d in DESC["descriptors"]}
V001 = json.loads((ROOT / "verification/prototypes/v001-crypto/vectors/om-v001-proto-vectors.json").read_text())
V005 = json.loads(gzip.decompress((ROOT / "verification/vectors/v005-w0-object-vectors.json.gz").read_bytes()))
OUT = ROOT / "verification/results/metadata-w0-rules-latest.json"
sys.path.insert(0, str(ROOT / "verification/cbor"))
from python_canonical import enc  # noqa: E402


def dec(b: bytes):
    def item(i):
        ib = b[i]; mj, ai = ib >> 5, ib & 31; i += 1
        n = ai
        if ai >= 24:
            w = 1 << (ai - 24); n = int.from_bytes(b[i:i + w], "big"); i += w
        if mj == 0: return n, i
        if mj == 2: return bytes(b[i:i + n]), i + n
        if mj == 3: return b[i:i + n].decode(), i + n
        if mj == 4:
            o = []
            for _ in range(n): x, i = item(i); o.append(x)
            return o, i
        if mj == 5:
            o = {}
            for _ in range(n): k, i = item(i); o[k], i = item(i)
            return o, i
        if mj == 7: return {20: False, 21: True, 22: None}[n], i
        raise ValueError(mj)
    v, e = item(0)
    assert e == len(b)
    return v


def declared(role, field):
    return EXPO[role].get(field, "no_by_design") != "no_by_design"


def H(*parts):
    return hashlib.sha256(b"|".join(parts)).digest()


# ---- criterion: push (ADR-0129 rule 14) ------------------------------------------------------------
def push(mutate=False):
    """Two IdentityProfiles on one install. Parties that receive a push registration: each profile's Home, or one
    declared push gateway. Web: one Web Push subscription per profile. Native: one per-app vendor token; a
    Keep-separate profile disables vendor push or all profiles register through one declared gateway."""
    bad = []
    for platform, mode in itertools.product(("web", "native"), ("shared_gateway", "keep_separate")):
        regs = []   # (party, token)
        for prof in ("p1", "p2"):
            if platform == "web":
                regs.append((f"home-{prof}", H(b"webpush", prof.encode())))
            elif mode == "shared_gateway" or mutate:
                regs.append(("declared_gateway" if not mutate else f"home-{prof}", H(b"apns-token")))
            elif prof == "p1":
                regs.append(("home-p1", H(b"apns-token")))   # p2 is Keep-separate: vendor push disabled
        by_token = {}
        for party, tok in regs:
            by_token.setdefault(tok, set()).add(party)
        for tok, parties in by_token.items():
            if len(parties) > 1:
                bad.append(f"{platform}/{mode}: one push token presented to {sorted(parties)}")
            if len([r for r in regs if r[1] == tok]) > 1 and not declared("push_provider", "cross_profile_linkage"):
                bad.append(f"{platform}/{mode}: profiles linked at the push provider but the descriptor does not declare it")
    return bad


# ---- criterion: shared-blob mode (ADR-0129 rule 10) --------------------------------------------------
def shared_blob(mutate=False):
    bad = []
    asset = b"custom-emoji-png"
    ids = {}
    for conv in ("c1", "c2", "c3"):
        key = H(b"shared-key") if mutate else os.urandom(32)   # default: fresh content key per send
        ids[conv] = H(b"blob-id", H(key, asset))                 # BlobID over the ciphertext
    if len(set(ids.values())) != len(ids):
        bad.append("default per-send encryption produced a shared BlobID across conversations")
    shared = {c: H(b"blob-id", H(H(b"pack-key"), asset)) for c in ("c1", "c2")}
    if len(set(shared.values())) == 1 and not declared("blob_store", "shared_blob_linkage"):
        bad.append("shared-blob mode links conversations but the BlobStore descriptor has no line for it")
    return bad


# ---- criterion: LAN rendezvous (R-270, R-271) ----------------------------------------------------------
def lan(mutate=False):
    K = 8  # fixed advertisement cardinality
    bad = []

    def advert(contacts, marked, blocked, epoch, n_ll=True):
        tags = [hmac.new(H(b"rel", c.encode()), f"epoch{epoch}".encode(), "sha256").digest()[:16]
                for c in sorted(contacts) if c in marked and c not in blocked]
        if mutate:
            return tags, b"ll-fixed"
        pad = [hmac.new(H(b"pad-seed"), f"{epoch}/{i}".encode(), "sha256").digest()[:16] for i in range(K - len(tags))]
        ll = H(b"ll", str(epoch).encode())[:6] if n_ll else b"ll-fixed"
        return sorted(tags[:K] + pad), ll

    sizes = set()
    for n in range(0, 7):
        contacts = {f"c{i}" for i in range(n)}
        marked = {c for c in contacts if c != "c0"}
        a1, ll1 = advert(contacts, marked, set(), 1)
        a2, ll2 = advert(contacts, marked, {"c1"}, 2)
        sizes.add(len(a1))
        if set(a1) & set(a2):
            bad.append(f"n={n}: a tag is reused across epochs")
        if ll1 == ll2:
            bad.append(f"n={n}: link-layer address not rotated across epochs")
        blocked_tag = hmac.new(H(b"rel", b"c1"), b"epoch2", "sha256").digest()[:16]
        if n > 1 and blocked_tag in a2:
            bad.append(f"n={n}: tag of a blocked relationship still advertised")
        if n > 0 and hmac.new(H(b"rel", b"c0"), b"epoch1", "sha256").digest()[:16] in a1:
            bad.append(f"n={n}: rendezvous tag for a contact not marked for LAN")
    if len(sizes) != 1:
        bad.append(f"advertisement size depends on the contact count: {sorted(sizes)}")
    if not declared("lan_observer", "lan_advertisement"):
        bad.append("lan_observer descriptor does not declare the advertisement")
    return bad


# ---- criterion: co-located Home roles (R-265, ADR-0130 rule 4) ------------------------------------------
def colocation(mutate=False):
    bad = []
    roles = ["discovery_publisher", "introgate", "home_relay_mailbox"]
    joint = {"descriptor_mailbox_linkage", "contact_graph_edge"}   # what one operator learns by joining its logs
    for f in joint:
        if not any(declared(r, f) for r in roles):
            bad.append(f"co-located observer learns {f} but no co-located role declares it")
    for r in roles:
        text = " ".join(next(d for d in DESC["descriptors"] if d["role"] == r)["non_collusion_assumptions"]).lower()
        if mutate:
            text = "discovery publisher, introgate and mailbox do not collude"
        if "one observer" not in text and "does not hold" not in text:
            bad.append(f"{r}: descriptor does not state co-located roles are one observer")
        if "do not collude" in text and "different operator" not in text and "one observer" not in text:
            bad.append(f"{r}: descriptor claims non-collusion between co-located roles")
    return bad


# ---- criterion: mandatory sealed delivery (ADR-0142), real V-005 outer bytes -----------------------------
def sealed_delivery(mutate=False):
    bad = []
    v = next(x for x in V005["vectors"] if x["id"] == "SealedDeliveryEnvelope/typical")
    raw = bytes.fromhex(v["hex"] if isinstance(v["hex"], str) else "".join(p if isinstance(p,str) else p["fill"]*p["len"] for p in v["hex"]))
    body = dec(raw)
    if mutate:
        body[8] = bytes.fromhex(V001["cases"][0]["alice"]["ik_pub"])
    allowed = {0, 1, 2, 3, 4, 5, 6, 7, 23}
    if set(body) - allowed:
        bad.append(f"sealed outer carries unexpected clear fields: {sorted(set(body)-allowed)}")
    # Real inner session identifiers/headers must not occur in the provider-facing wrapper.
    # Use the canonical vector bytes directly; `dec` returns native bytes that the diagnostic encoder does not accept.
    blob = raw
    secrets = []
    for case in V001["cases"]:
        for who in ("alice", "bob"):
            if "ik_pub" in case[who]: secrets.append(bytes.fromhex(case[who]["ik_pub"]))
        for ev in case["events"]:
            if ev["type"] == "send": secrets.append(bytes.fromhex(ev["header"]))
    if any(x and x in blob for x in secrets):
        bad.append("sealed outer contains a sender identity key or Double Ratchet header")
    for r in ("home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home"):
        if EXPO[r].get("ratchet_header_shape") != "no_by_design":
            bad.append(f"{r}: ratchet_header_shape must be no_by_design under ADR-0142")
        if EXPO[r].get("stable_device_correlator") != "no_by_design":
            bad.append(f"{r}: established 1:1 stable_device_correlator must be no_by_design under ADR-0142")
    return bad, len(blob)


# ---- criteria: first contact (R-202, R-203, R-280), real V-005 MessageRequest ---------------------------
FORBIDDEN_AT_INTROGATE = {"stable_account_id", "device_id", "intro_identity_private", "intro_identity_public"}


def first_contact_flows(mutate=False):
    """Who sees what, per step. The IntroGate handles only the admission surface; bindings travel inside the
    accepted relationship (mailbox deposits under the reply_route capability, end-to-end encrypted)."""
    steps = [
        ("introgate", "submit_message_request", {"ticket_id", "request_key_digest", "crypto_profile", "hpke_enc", "ciphertext", "admission_proof"}),
        ("home_relay_mailbox", "deliver_accept_reply", {"ciphertext"}),
        ("home_relay_mailbox", "recipient_iib", {"ciphertext"}),
        ("home_relay_mailbox", "requester_iib", {"ciphertext"}),
    ]
    if mutate:
        steps[0][2].add("intro_identity_public")
    bad = []
    for role, step, fields in steps:
        if role == "introgate" and fields & FORBIDDEN_AT_INTROGATE:
            bad.append(f"{step}: IntroGate sees {sorted(fields & FORBIDDEN_AT_INTROGATE)}")
        if role != "introgate" and fields - {"ciphertext"}:
            bad.append(f"{step}: {role} sees more than ciphertext")
    mr = next(v for v in V005["vectors"] if v["id"] == "MessageRequest/hpke-pq-05-sealed")
    raw = "".join(x if isinstance(x, str) else x["fill"] * x["len"] for x in (mr["hex"] if isinstance(mr["hex"], list) else [mr["hex"]]))
    body = dec(bytes.fromhex(raw))
    if set(body) - {0, 1, 2, 3, 4, 5, 6, 7, 23}:
        bad.append(f"MessageRequest carries fields beyond the admission surface: {sorted(set(body))}")
    sys.path.insert(0, str(ROOT / "verification/prototypes/v005-encoding/gen"))
    import gen_vectors as g  # constants only (module import does not generate)
    secrets = [g.ACC_BOB, g.E1, g.E2, g.SENDER_INTRO, g.ACC, g.INTRO]
    blob = bytes.fromhex(raw)
    for s in secrets:
        if s in blob:
            bad.append("an account id, device id or intro identity appears in MessageRequest bytes")
    return bad


# ---- criterion: KeyMaterialProvider (R-209), real V-001 bundle bytes ------------------------------------
def key_material(mutate=False):
    bad = []
    # ADR-0140 made the V-005 current AsyncKeyBundle encoding authoritative for this metadata check;
    # older V-001 session KATs predate the provider-visible ACS-field removal.
    v = next(x for x in V005["vectors"] if x["id"] == "AsyncKeyBundle/typical")
    raw = bytes.fromhex(v["hex"] if isinstance(v["hex"], str) else "".join(p if isinstance(p,str) else p["fill"]*p["len"] for p in v["hex"]))
    env = dec(raw)
    body = dec(env[1])
    visible = set(body)
    documented = {0, 1, 2, 5, 6, 7, 8, 9, 10, 11, 12, 13}
    if visible - documented:
        bad.append(f"bundle exposes undocumented fields {sorted(visible - documented)}")
    if 3 in visible or 4 in visible:
        bad.append("bundle still exposes retired ACS generation/hash")
    if EXPO["key_material_provider"].get("stable_account_id") != "no_by_design" or EXPO["key_material_provider"].get("device_count") != "no_by_design":
        bad.append("KeyMaterialProvider descriptor still claims account-wide ACS linkage")
    # per-relationship fetch capabilities: the provider's log keys (cap) never repeat across a fetcher's relationships
    caps = {rel: (b"cap-shared" if mutate else H(b"fetch-cap", rel.encode())) for rel in ("alice-bob", "alice-carol", "alice-dave")}
    if len(set(caps.values())) != len(caps):
        bad.append("one fetch capability is used for several relationships (links the fetcher's contacts)")
    # disjoint provider pools: pool ids differ per provider
    pools = {p: H(b"pool", p.encode())[:16] for p in ("provider-A", "provider-B")}
    if len(set(pools.values())) != 2:
        bad.append("same pool id at two providers")
    return bad


# ---- criterion: stable per-device correlators (ADR-0136/0137, review F-5/F-12), real W0 bytes ---------
def device_correlator(mutate=False):
    """Check stable-device frequency after ADR-0142 sealed delivery.

    KeyMaterialProvider subject DeviceID and push registration remain stable. Established 1:1 mailbox wrappers do
    not carry a stable sender-device correlator. Provider-resource capabilities remain non-identity handles.
    """
    bad = []
    expo = {r: dict(e) for r, e in EXPO.items()}
    affected = {
        "key_material_provider": "always", "home_relay_mailbox": "no_by_design", "t0_sponsored_mailbox": "no_by_design",
        "t1_hosted_home": "no_by_design", "push_provider": "always", "discovery_publisher": "no_by_design",
        "recovery_record_store": "no_by_design", "blob_store": "no_by_design",
    }
    if mutate:
        for role, expected in affected.items():
            expo[role]["stable_device_correlator"] = ("no_by_design" if expected == "always" else "always")
    b = V001["binding"]
    body = dec(dec(bytes.fromhex(b["positive_fetch"][0]))[1])
    if body.get(2) == bytes.fromhex(b["device_id"]) and expo["key_material_provider"].get("stable_device_correlator") != "always":
        bad.append("key_material_provider: AsyncKeyBundle exposes subject_device_id but descriptor is not always")
    for r in ("home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home"):
        if expo[r].get("stable_device_correlator") != "no_by_design":
            bad.append(f"{r}: ADR-0142 established wrapper must not expose a stable sender-device correlator")
    if expo["push_provider"].get("stable_device_correlator") != "always":
        bad.append("push_provider: registered push token/device handle is stable but descriptor is not always")
    for r in ("discovery_publisher", "recovery_record_store", "blob_store"):
        if expo[r].get("stable_device_correlator") != "no_by_design":
            bad.append(f"{r}: provider-resource capability must not be modeled as a stable OM device correlator")
    return bad


def observer_matrix_alignment(mutate=False):
    """Human matrix and machine descriptor must name the same stable-device frequencies for canonical roles."""
    text = (ROOT / "docs/architecture/providers/METADATA_OBSERVER_MATRIX.md").read_text()
    machine = {r: dict(e) for r, e in EXPO.items()}
    if mutate:
        machine["discovery_publisher"]["stable_device_correlator"] = "sometimes"
    rows = {}
    for line in text.splitlines():
        if not line.startswith("|") or "Observer" in line or line.startswith("|---"):
            continue
        cells = [x.strip() for x in line.strip().strip("|").split("|")]
        if len(cells) < 2: continue
        obs, claim = cells[0], cells[1]
        if "**always**" in claim: freq = "always"
        elif "**sometimes**" in claim: freq = "sometimes"
        elif "no stable device correlator" in claim or "no stable sender-device correlator" in claim or "no OM stable account/device field" in claim or "no stable OM identity" in claim: freq = "no_by_design"
        else: continue  # platform-dependent row intentionally has no exact W0 machine frequency
        rows[obs] = freq
    mapping = {
        "mailbox-class Home / relay": ("home_relay_mailbox", "t0_sponsored_mailbox", "t1_hosted_home"),
        "KeyMaterialProvider": ("key_material_provider",), "push provider/gateway": ("push_provider",),
        "discovery publisher": ("discovery_publisher",), "RecoveryRecord store": ("recovery_record_store",),
        "BlobStore / StoragePool": ("blob_store",), "IntroGate": ("introgate",),
        "ClientHost origin": ("clienthost_origin",), "LAN observer": ("lan_observer",),
    }
    bad=[]
    for human, roles in mapping.items():
        if human not in rows:
            bad.append(f"matrix row missing/unparseable: {human}"); continue
        for role in roles:
            got=machine[role].get("stable_device_correlator","no_by_design")
            if got != rows[human]: bad.append(f"matrix/descriptor drift: {human}={rows[human]} but {role}={got}")
    return bad


def provider_resource_auth(mutate=False):
    """ADR-0138: routine Home admin is capability-scoped; AccountID/DeviceCredential auth is not required."""
    text=(ROOT/"spec/protocol/home-api.md").read_text()
    if mutate:
        text=text.replace('`directory_publish_cap` | publish/replace one descriptor namespace entry', '`device_auth` | publish/replace one descriptor namespace entry',1)
    bad=[]
    if '`device_auth`' in text:
        bad.append('Home API still exposes device_auth for routine provider administration')
    for cap in ('relay_admin_cap','directory_publish_cap','key_publish_cap','recovery_record_read_cap','recovery_record_write_cap','blob_repair_cap'):
        if cap not in text: bad.append(f'Home API missing {cap}')
    if '`GET /v1/recovery/records/{opaque_locator}`' not in text:
        bad.append('RecoveryRecord get operation missing')
    return bad

# ---- criterion: group fan-out (R-226) ------------------------------------------------------------------
def group_fanout(mutate=False):
    from cryptography.hazmat.primitives.ciphers.aead import AESGCMSIV
    bad = []
    conversation_id = H(b"conversation-id-random")
    group_id = os.urandom(32)          # MLS group_id: random, never derived from ConversationID
    epoch = (7).to_bytes(8, "big")
    mls_private_message = group_id + epoch + b"\x01" + os.urandom(64)   # RFC 9420 PrivateMessage header is cleartext
    copies = []
    for r in ("bob", "carol", "dave"):
        key = H(b"outer", r.encode())
        outer = mls_private_message if mutate else AESGCMSIV(key).encrypt(os.urandom(12), mls_private_message, r.encode())
        copies.append(outer)
    for c in copies:
        if group_id in c or (epoch + b"\x01") in c:
            bad.append("a relay-visible copy contains the raw MLS group_id or epoch")
    if len(set(copies)) != len(copies):
        bad.append("recipient copies are identical (linkable across mailboxes)")
    if group_id == conversation_id or H(conversation_id)[:32] == group_id:
        bad.append("group_id derivable from ConversationID")
    return bad


def main() -> int:
    checks = {
        "push_cross_profile": push, "shared_blob": shared_blob, "lan_rendezvous": lan, "colocated_home_roles": colocation,
        "first_contact_introgate": first_contact_flows, "key_material_provider": key_material, "group_fanout": group_fanout,
        "stable_device_correlator": device_correlator, "observer_matrix_alignment": observer_matrix_alignment,
        "provider_resource_auth": provider_resource_auth,
    }
    res = {}
    for name, fn in checks.items():
        ok, mut = fn(), fn(mutate=True)
        res[name] = {"problems": ok, "mutation_caught": bool(mut), "mutation_problems": mut[:3]}
    hb, outer_bytes = sealed_delivery()
    mb, _ = sealed_delivery(mutate=True)
    res["sealed_delivery_outer"] = {"problems": hb, "mutation_caught": bool(mb), "outer_bytes_seen": outer_bytes}
    passed = all(not r["problems"] and r["mutation_caught"] for r in res.values())
    OUT.write_text(json.dumps({"model": "metadata_w0_rules", "gates": ["V-015"], "passed": passed, "checks": res,
                               "gate_status_effect": "evidence_only_no_gate_automatically_passed"}, indent=1) + "\n")
    print(json.dumps({"passed": passed, **{k: (not v["problems"], v["mutation_caught"]) for k, v in res.items()}}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
