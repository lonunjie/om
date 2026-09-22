# OM Master Planning & Design Snapshot

Generated: **2026-09-22 23:02 PHT (UTC+08:00)**

This snapshot supersedes the 2026-09-22 14:34 PHT engineering-transition checkpoint, preserved under `history/20260922-1434/`.

## Phase

**Architecture hardening / pre-engineering. Product engineering is not authorized.**

The 14:34 decision to begin engineering is intentionally reversed after a deeper cross-boundary feasibility audit found several expensive-to-retrofit protocol/state-machine gaps. No fatal blocker was found; the correct response is to close them before implementation creates inertia.

## Constitutional direction

OM is a gift to humanity: an open, independently implementable secure-messaging protocol whose users are not made dependent on the founder, SeedLife/lonunjie, a company, a cloud, an app store, or an official client. Businesses may participate and compete, but receive no protocol privilege.

See `PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`.

## Major hardening additions

- layered anti-capture licensing: MPL-2.0 core, AGPL-3.0-or-later reference network services, Apache-2.0 normative protocol/conformance artifacts, CC-BY-SA-4.0 documentation;
- DCO/inbound-equals-outbound contribution model; no ordinary broad copyright-assignment/relicensing CLA;
- SeedLife/lonunjie initial stewardship with explicit founder-only S0 custody and future succession rails;
- asynchronous key-material publication as a first-class provider boundary;
- independent one-to-one multi-device session-management state machine;
- MLS same-epoch Commit ordering and OM GroupControlState↔MLS transition binding;
- catastrophic relay reachability repair and immutable BlobID/mutable location repair;
- unified time/expiry/clock semantics;
- self-hosted ClientHost update/deployment lifecycle;
- machine-enforced repository contract, architecture map, file registry and stewardship registry;
- canonical cleanup: random algorithm-independent AccountID, one AccountControlState term, one current ProviderCapabilities schema.

## Blocking architecture gates before engineering

1. freeze CryptoProfile v1 and its asynchronous key-material semantics;
2. freeze the one-to-one multi-device session state machine;
3. select/prove MLS Commit ordering and atomic group-policy binding;
4. freeze catastrophic reachability/blob-repair semantics;
5. classify all canonical time/expiry fields and skew behavior;
6. freeze ClientHost update/recovery architecture;
7. complete protocol RF-patent contribution language and mixed-license legal review before broad public contributions;
8. run the repository contract cleanly with no unclassified current files or canonical-definition drift;
9. close every other question classified as architecture-blocking, or prove its interface is safely bounded.

## Machine truth

Run:

```sh
python3 tools/validate_architecture.py
```

A later accepted ADR must explicitly set `engineering_authorized=true` before `ENGINEER_START_HERE.md` becomes active.
