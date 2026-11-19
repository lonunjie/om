# ADR-0234 — Objection-board privacy boundary, class-aware admission and honest-versus-hostile Home claims

- **Status:** Accepted by delegation (2026-11-19) **only** for items 1, 2, 3 and 6 (class-aware admission and the honest statement of what a hostile Home and an outside reader can see) and for the reading in item 5 that 8 or more entries is incomplete. **Items 4 and 5 (second part) are not adopted and need the operator or a specialist**, because they would amend the audited board text or require a new versioned response. Item 2's per-class reading of the hourly bound depends on ADR-0239, which is proposed and needs operator acceptance because it amends the audited Home API v1 section 8.4 row; until then the per-class hourly guarantee is not approved audited semantics.
- **Date:** 2026-11-19
- **Decided by:** operator delegation 2026-11-19 (standing direction: answer design questions from the constitution by ADR, additive only; never amend audited text, never weaken a cryptographic or metadata floor). Authored by an independent reviewer of a different vendor than the code author; imported by the repository's agent, with the changes listed under "Import notes".
- **Class:** Item 2 is internal Home behavior with no wire change; the rest is a statement of claims. No new wire is authorized.
- **Answers:** OM4-43 and OM4-44 follow-up (veto board classes and pacing) and the metadata question left open by ADR-0226 sections 3 and 4.
- **Amends:** nothing. **Does not amend:** ADR-0201 to ADR-0203, ADR-0226, [`METADATA_THREAT_MODEL.md`](../docs/threat-model/METADATA_THREAT_MODEL.md), [`RECOVERY_AND_BACKUP_POLICY.md`](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), [`home-api.md`](../spec/protocol/home-api.md).
- **Scope:** `core_kernel` and `provider_reference`, for recovery only.
- **Controlling sources:** ADR-0201 to ADR-0203, ADR-0226 sections 3 and 4; the metadata threat model, the recovery policy and Home API v1.

## Design sources read, and not read

Read in full for this import: the six proposals; ADR-0204, ADR-0225, ADR-0227, ADR-0228, ADR-0229 and the passages of ADR-0199, ADR-0203 and ADR-0226 cited here; [`call.cddl`](../spec/cddl/call.cddl), [`controller-change.cddl`](../spec/cddl/controller-change.cddl), [`lan.cddl`](../spec/cddl/lan.cddl), the recovery-record CDDL, [`recovery-sheet.md`](../spec/protocol/recovery-sheet.md), the object-type, domain-separation and numeric-field registries and Home API v1 section 8.2. Not read in full: ADR-0216, the constitution documents, and the domain documents the design gate lists for these paths. The proposals were checked against the sources above only; if an unread text fixes a point differently, that text wins and this ADR is corrected by a follow-up.

## Ruling

1. **There are two different adversaries.** A *remote unauthenticated poster or reader* can be kept from learning whether a request digest exists through equal-shaped success/drop and keyed fixed-size pacing. A **hostile Home** cannot be kept from knowing class, validity or timing of traffic delivered to itself: the Home receives AccountID/epoch, a candidate bearer inbox capability, and the digest in `postVeto`, computes the bearer hash and class, sees the request table and stores/serves the ciphertexts. Home can censor, order, copy or suppress board entries. Do **not** claim the reservation, 256-bucket mapping or empty-board answer cryptographically hides class or proves completeness *against a hostile Home*. A hostile Home can also record a presented capability; this is bearer admission, not a cryptographic client signature of class.
2. **Keep reserved storage and prevent public starvation.** Storage may retain 16 public + 8 controller entries per digest, with no public post ever consuming controller **capacity**. However, all *admission bottlenecks* also have to be separated: at least eight authenticated-class posts per rolling hour remain possible independently of public-class posts. Do not put a global eight-per-hour inbox count **ahead of** class checks. For example, maintain `(inbox, class, hour-window)` counters, each with its published bound, inside the same transaction. The public class has its own eight-per-hour anti-flood bound, and controller class eight; an unknown cap is public. Apply existing whole-Home burst as an **availability** throttle with a reserved controller quota or admit that a flood can block all classes. An untrusted public poster cannot learn whether an apparent controller post consumed reserved capacity from responses.
3. **Response privacy.** POST outcome must remain identical in status, response body size, error category, and ideally timing for present/absent digest, valid/invalid bearer, duplicate, class full and quota full. Do not return counts, class labels or the inbox existence flag in a public reply. No public API may reveal the Home's HMAC secret, bucket number or cap hash. Reads pace all digests (present or absent) with the same keyed mapping `HMAC-SHA256(home_secret, request_digest)[0]`, 256 durable buckets, burst 10/refill 1 per 30 seconds; reuse the **existing label-free HMAC construction**, not a global unkeyed hash. Do not move to per-digest buckets that create existence state. Two digests sharing one bucket do have coupled throttling; acknowledge this as metadata and availability leakage to an active probing reader.
4. **Class ordering leaks to readers.** ADR-0226 requires **controller entries first**; a reader able to inject public entries and compare ordered list snapshots can infer, at minimum, that an earlier slot changed when a controller item arrived. A Home cannot promise to hide that bit while returning an unlabelled ordered concatenation. Either (a) explicitly accept this external-reader side channel in the audited threat model, while keeping no *explicit* class labels, or (b) redesign the audited board response and saturation attestation to hide class (requires a versioned new wire and specialist approval). **Do not** silently reorder controller items after public or use a random shuffle: that departs from ADR-0226 §3 and breaks deterministic fullness handling.
5. **Reader completeness must remain fail-closed.** The current public `encodeVetoList` includes no class marker or count. Given only `N` entries, a reader cannot distinguish `8 controller + 0 public` (controller full) from `0 controller + 8 public` (neither full). With this wire, the **only sound generic threshold is `N >= 8` = incomplete**; Core takes that conservative route (ADR-0226 section 3 item 3). An answer of fewer than eight is complete only as to *capacity under this specified honest Home*, never a cryptographic absence proof from a malicious Home. Do not increase threshold to 24 just because advertised capacity totals 24. If the product requires safe progress at 8..23 entries, define an independently authenticated, class-aware completeness proof **outside the current wire** or accept a new class-leaking response format via audited review.
6. **No Home is a recovery authority.** A saturated/unanswered board never means “no objection”; preserve valid already-verified vetoes, in-session controller approval and contact notice routes. A malicious Home may falsely answer empty; strict-timer safety relies on independently verified head/clock/veto evidence and explicitly assumes Home response availability/honesty only to the degree documented, not a magically unforgeable directory. The client must never log or persist request digests, bearer caps or controller class for debugging; secrecy of a Home-controlled HMAC key is only against remote clients.

## Format and compatibility

**No new wire is authorized by this ruling.** Keep the existing `VetoPost`, `GET /v1/recovery/requests/{digest}/vetoes`, `encodeVetoList` and current Home API v1. Admission is an internal class-aware transactional counter, not a wire property. Logical persistent row schema:

```cddl
; Logical storage record, NOT an on-wire object nor a new type number.
board-admission-counter = {
  0: bstr .size 32,             ; recovery inbox id, already internal
  1: 0 / 1,                     ; public or controller admission class
  2: uint,                      ; coarse hour/window index
  3: 0..8,                      ; accepted posts in that class/window
}
board-read-bucket = {
  0: 0..255,                    ; HMAC(home_secret, digest)[0]
  1: 0..10,                     ; tokens
  2: uint,                      ; last accounting time (internal)
}
```

No extra domain-separation label is needed to correct internal rate counters; preserve Home-local unexposed 256-bucket HMAC key, never reuse it for account signing, snapshot AEAD or locator derivation. If board responses gain authenticated per-class summaries, register **a new wire version**, class-commitment label and threat-model change through the specialist before coding; do not smuggle a class discriminator into the existing veto object's extension field.

## Metadata threat matrix

| Observer | Existing evidence | Additional risk / permitted claim |
|---|---|---|
| Unauthenticated remote poster | Can send request digest and shape-only bytes, see equal status | Should not infer inbox existence, which class was admitted or capacity via response; can cause availability saturation |
| Public board reader with known digest | Sees ordered list, payload sizes, throttling and changes over time | Can infer arrivals and possibly class through controller-first ordering; 256-bucket collision leaks a coarse shared pacing fate, not the HMAC key |
| Same-person controller | Has inbox capability, account head and signed veto content | Intentionally can object, may correlate its own class/objection; cap loss compromises controller reservation at that Home |
| Honest-but-curious Home | Sees AccountID/epoch, digest, cap on request, class, timing, size | **Class secrecy from Home is impossible** with this API; it must not see recovery root/snapshot plaintext |
| Hostile Home | Can record cap, lie about board results, censor, reorder, throttle | Cannot confer ACS authority; **cannot be trusted to attest empty board**; requires independent controller/contact evidence where safety demands |

## Golden/conformance vectors and exploitation reproducer

- `BOARD-01`: Create inbox, hold one valid request digest, send eight distinct shape-valid *public* posts within 3600 seconds (cap absent), then one well-formed authenticated controller-cap post. Expected: the controller post is stored in the controller class and appears before public posts. Both Worker Home and SQLite Home must run the same fixture. Confirm public posts never consume controller quota.
- `BOARD-02`: Post to absent digest, unknown inbox, wrong cap, valid cap, duplicate, both class capacities, public 8/hour and controller 8/hour: identical successful post envelope; no `rate_limited` solely due to digest existence/class.
- `BOARD-03`: Fixed 32-byte Home HMAC key `00..1f`, digests `11*32` and `22*32`; independent TS/Python implementations compute canonical HMAC-SHA256 and first-byte bucket, refill schedule, present/absent symmetry, collision throttling. Expected HMAC outputs: digest `11*32` -> `19c89035532577d8d991258817b7ff62e37dc37e42237d66034ad0ca5602fe0c`, **bucket 25**; digest `22*32` -> `652cf36446a9606492097f64ea1bb8d03c93192ceb8082c2f5c1c29da6f22ab4`, **bucket 101**. Freeze these as cross-runtime fixtures. Restart persists same secret and 256 counters; no per-digest growth.
- `BOARD-04`: GET synthetic boards containing exactly 7, 8, 16, 23 and 24 entries, with hidden class partition; only `<8` can be treated complete with current unclassified response. Any `>=8` blocks strict-time adoption absent verified surviving-controller approval. A malicious Home returning empty is explicitly a separate unverifiable case, not labeled “secured by board”.

## What this ADR adopts and what it does not

| Item | Disposition |
|---|---|
| 1 Two adversaries; no claim that the board hides class or proves completeness against a hostile Home | **Adopted** (an honest statement of what each observer can see; it removes no protection and the matrix below is the permitted claim) |
| 2 Class-aware admission: a hourly bound per class, public posts never consuming controller capacity or controller hourly admission | **Adopted** (repair of the reserved class of ADR-0226 section 3; internal Home state, no wire change). The Home API v1 table row "veto posts per inbox, rolling hour: 8" of [`home-api.md`](../spec/protocol/home-api.md) section 8.2 is read as applying within each class; the table text itself is audited and is not edited by this ADR, so the specialist should confirm the reading and add a clarifying note with the Home change |
| 3 Equal response shape; 256-bucket keyed read pacing | **Adopted** (it restates ADR-0226 section 4 and adds the coupled-bucket acknowledgement) |
| 4 Controller-first ordering lets an active reader infer an arrival: either accept it in the audited threat model, or redesign the board response to hide class | **Not adopted. Needs operator or specialist.** Option (a) edits the audited threat model and option (b) is a new versioned response; neither is within delegation. Until decided, ADR-0226's controller-first order stands unchanged; no reordering or shuffling is permitted |
| 5 With the current unlabelled list, 8 or more entries is incomplete | **Adopted as the reading of ADR-0226 section 3** (it fixes the safe side). The suggestion to define a class-aware completeness proof outside the current wire, or to accept a class-leaking response, is **not adopted. Needs operator or specialist** |
| 6 No Home is a recovery authority; no logging of digests, capabilities or class | **Adopted** (restates the fail-closed rules already in force) |

## Gate

Item 2 is internal Home behavior: both reference Homes must pass the same `BOARD-01` and `BOARD-02` fixtures. `BOARD-03` fixes the cross-runtime pacing fixtures (verified for this import: HMAC-SHA-256 with key bytes `00` to `1f` over thirty-two `11` bytes gives `19c8...fe0c`, bucket 25, and over thirty-two `22` bytes gives `652c...2ab4`, bucket 101). A specialist metadata review is still required before the board's privacy claims are made in product text.

## Import notes (collision check and edits, 2026-11-19)

- **Values checked, no collision, no new value.** No number, CDDL field, registry row or domain label is introduced: the storage rows are logical and internal, and the pacing key is the existing per-Home secret of ADR-0226 section 4.
- **Edits.** The reviewer's section describing a specific unfixed source defect was removed, as were file-and-line references and the "current code fails" clause of `BOARD-01`; the adoption table and gate text were added; provisional-number sentence removed.
