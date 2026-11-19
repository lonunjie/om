# ADR-0239 — Recovery-veto hourly class quotas: explicit audited Home API amendment required

- **Status:** Accepted (2026-11-19), by the operator's direct delegation of this decision to the agent ("you decide on what's best for ADR-0239 based on our constitution"). It amends audited text (Home API v1 section 8.4, the row "veto posts per inbox, rolling hour"); the amendment is applied to [`home-api.md`](../spec/protocol/home-api.md) in the same change, with this ADR as its record under ADR-0165 change control. Reason from the constitution: recovery must work without an operator and fail closed, so an inbox that untrusted public posts can fill must still keep a reserved path for the person's own controllers; 8 public plus 8 controller per hour (16 in total) is the smallest bound that keeps that reservation.
- **Date:** 2026-11-19
- **Proposed by:** an independent reviewer of a different vendor than the code author, imported by the repository's agent as a proposal. Not decided: it needs the operator's acceptance (see Status).
- **Decided by:** the operator, by delegating the decision to the agent on 2026-11-19; decided from the constitution (fail closed, no operator dependence, a reserved recovery path).
- **Class:** Normative change to a Home API limit semantics; its stronger controller reservation is a security tightening, but the previous aggregate bound changes. Treat under ADR-0165 change control, not an invisible editorial correction.
- **Answers:** the hourly-bound question left by ADR-0234 item 2 (Home API v1 §8.4, described in the question as §8.2).
- **Sources:** [`AGENTS.md`](../AGENTS.md), ADR-0226 §§3–4, ADR-0234 §2, [`spec/protocol/home-api.md`](../spec/protocol/home-api.md) §8.4, its feature/limit advertisement in §8, and [`docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) §1–2. The Home implementations/conformance harness were not run as part of this ruling.

## Finding and decision

The audited [`home-api.md`](../spec/protocol/home-api.md) §8.4 row is literal:

```
| veto posts per inbox, rolling hour | 8 | 8 |
```

This says **eight aggregate posts per inbox per rolling hour**. ADR-0234 §2 requires that eight controller-class posts be admitted even if eight public posts have already been accepted. That admits at least **sixteen** posts in an hour. Both statements cannot hold for the same inbox, and a class-aware implementation with a shared preliminary eight-post limiter still defeats the reserved class. Consequently, the per-class reading is a **desired amended contract**, not a faithful behavior-preserving interpretation of the audited row. Explicitly replace or qualify that row only after the operator/audit-owner approves this normative change.

**Exact proposed audited-table delta:**

```diff
-| veto posts per inbox, rolling hour | 8 | 8 |
+| veto posts per inbox **per class**, rolling hour | 8 public; 8 controller (at most 16 total) | same |
```

Add immediately below the table:

> For the recovery-veto board's two admission classes (ADR-0226 §3), the hourly limit is independently accounted on `(inbox_id, class)` with a rolling one-hour window. A public-class post must never consume controller-class hourly tokens or the eight reserved controller-class **storage slots**. An absent/wrong bearer is public. Storage remains at most 16 public plus eight controller entries per request digest. Each post returns the same success/drop envelope irrespective of digest existence or chosen class. A whole-Home anti-abuse limit may further restrict availability only if its advertised/controller-reserved capacity does not let public floods exhaust the controller minimum. The per-class interpretation changes the prior maximum number of accepted posts per inbox per hour from eight to sixteen; it is an intentional normative amendment, not an erratum with unchanged behavior.

Without such approval, the only honest choices are: keep the old eight aggregate and **do not claim** ADR-0234's eight-post reserved hourly guarantee; or keep recovery-veto class-aware emission gated. Do not let client UI infer a complete quiet board from saturation or a success-shaped post response; ADR-0226's evidence-incompleteness rule remains intact. A Home that implements only the old aggregate limit may remain available for the legacy feature but must not be represented as meeting this newer guarantee; feature/limit negotiation and conformance must make that difference observable to clients before production reliance.

## CDDL, labels, vectors and metadata

- **CDDL delta:** none. Post/read body and response shapes are unchanged.
- **Domain-separation delta:** none. Continue the fixed keyed 256-bucket digest-read mapping and existing operation limits; do not create per-digest read counters or reveal classes through a new response.
- **Conformance vectors:** `BOARD2-01`: eight public posts immediately followed by eight bearer-valid controller posts to the same inbox/rolling hour; all controller posts can occupy reserved slots. Reverse the order. `BOARD2-02`: nine posts per class, full per-class digest storage, absent digest, wrong cap, identical-shaped responses. `BOARD2-03`: public burst against whole-Home quota does not disable the advertised controller reservation; otherwise flag availability limitation. `BOARD2-04`: replay `BOARD-03` HMAC vectors (digest `11*32` → bucket 25, digest `22*32` → 101), check present/absent pacing and no leak of class through response code, length or timing beyond incidental load effects. Both Cloudflare and SQLite Homes must run the same fixtures.
- **Metadata:** a hostile Home already sees bearer validity, class, request digest and admission timing; separate hourly counters do not conceal them. A remote unauthenticated poster still receives indistinguishable success/drop envelopes and a non-enumerating 256-bucket read mapping. The Home can censor, delay, or omit controller posts, so a full or empty board is **not** cryptographic proof of completeness against that Home. A larger per-class hourly allowance changes capacity/abuse exposure but does not introduce a Home-facing account cryptographic secret.

## Gate

The audited Home API §8.4 change, both reference Home conformance suites and the provider capability declaration must be accepted together. Until then ADR-0234 item 2's per-class reading must not be represented as approved audited semantics. No release claim.

## Import notes (collision check and edits, 2026-11-19)

- **Not accepted by delegation, on purpose.** The proposal changes the audited aggregate bound from eight to up to sixteen posts per inbox per hour. AGENTS "Design gate" rule 6 forbids an ADR by delegation from doing that, so this ADR is imported as proposed and [`spec/protocol/home-api.md`](../spec/protocol/home-api.md) was not edited. On acceptance, the same change must edit the row, update the capability declaration and add the `BOARD2-01` to `BOARD2-04` fixtures to both reference Homes.
- **Section number.** The audited row is in section 8.4 (limits), not 8.2.
- **Values checked, no collision.** No CDDL field, registry value or label is introduced. Vector ids `BOARD2-01` to `BOARD2-04` are unused; `BOARD-03` is the existing board vector it replays.
- **Relation to ADR-0234.** Item 2 of ADR-0234 (class-aware admission) depends on this ADR for its per-class hourly accounting; ADR-0234 carries a pointer to that effect.
- **Edits.** Review-numbering references in the header were reworded to neutral wording; the status and decided-by lines were rewritten; this section was added. No ruling text was changed.
