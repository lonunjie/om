# ADR-0207 — Room rejoin after owner recovery

## Design sources read

ADR-0206, ADR-0173, ADR-0177, ADR-0187, ADR-0189, the group transport CDDL, the protocol evolution
contract and the operator's Q-REL-5 through Q-REL-7 rulings inform this decision. The implementation
also uses the device rules and transition schemas in ADR-0176, the group chat and convergence
architecture, the Room capability registry and the shared UX brief. The remaining mapped Core, client
and CDDL sources were not all read in full.

- **Status:** Accepted by delegation, 2026-11-18.
- **Scope:** D157, the Room portion of ADR-0206. Q-REL-5 through Q-REL-7 supersede the earlier
  Room re-add and KeyPackage-reference proposals. The new contact event is additive with safe
  old-client failure. Existing invite, accept and Welcome bytes and consent rules apply.

## Decision

1. After owner recovery, **Ask to rejoin** sends an advisory event through a usable, authenticated
   contact relationship with a Room member. It carries only the recovered AccountID and the Room
   name as a hint. The existing event header identifies the authenticated author, device and contact
   conversation. The name is compared exactly against current Rooms locally. Zero or multiple matches
   keep the request from producing a review. A name gives no authority over a Room.
2. The recipient checks the authenticated device and AccountID against the current validated account
   chain and its accepted recovery successor. It offers a review in the uniquely matched Room only
   to a current member device. It binds the review to that membership incarnation and successor.
   An identical request, including one sent under a new EventID, creates at most one review for the
   same Room, account, device and head. The member sees “‹name› wants to rejoin” using the verified
   contact's local display name. The member explicitly agrees or declines.
3. Core revalidates the evidence and policy at confirmation. It uses the journaled
   `device.remove_revoked` transition for the revoked device first, with the existing route rotation
   and send pause. Once the removal is accepted, the member sends an ordinary Room invitation
   through the existing relationship. If authorization, sequencing or admin approval is unavailable,
   the request stays pending. No recovery request authorizes a Room transition by itself.
4. The recovered person explicitly accepts the ordinary invitation, which releases a single-use
   KeyPackage and a fresh per-Room route for that invitation ID. The inviting member authors the
   journaled `device.add` under current Room policy, including admin approval where required. This
   adds a leaf for the existing membership incarnation and preserves the account's roles. Each Room
   uses its own invitation ID and release. A Welcome without a matching accepted invitation is refused.
5. The recovered profile shows quiet per-Room pending state once Core durably queues the request.
   Pending follows the chosen contact and hint until the ordinary invitation installs the Room. The
   validated Welcome determines the actual Room ID. Recovery provides no new Room directory or route.

## Wire shape

W1 kind `18`, kind version `1`, requires exactly `["om.group.rejoin.v1"]`. The W0 registry view and
earlier W1 decoders reject the new kind. It has no fallback interpretation as a message or invitation.

```cddl
event-group-rejoin-request = { event-common, 8: 18, 11: body-group-rejoin-request }
body-group-rejoin-request = { 0: account-id, 1: tstr .size (1..256) }
```

## Where did authority go?

The validated account chain determines device eligibility. The canonical Room state, authorized member
confirmation, ordinary invitation acceptance and journaled MLS transition determine admission. The
contact request and pending display convey no Room authority.

## Verification

The [wire prototype](../verification/prototypes/group-transport-wire/rejoin_request_vectors.py) checks
the additive kind against two codecs, including W0 and earlier W1 rejection. Core tests must cover
owner recovery over two identities, removal before invitation, explicit acceptance, sending and
receiving, duplicate and stale requests, refusal under Room policy, and unsolicited Welcome refusal.
