# ADR-0216 — Approving a linked device as a controller

- **Status:** Accepted by delegation. The controller exclusion ruling amends no audited text and weakens no security floor. Additive pointer: ADR-0241 (gated) keeps the `kit-transfer` slot at its 4,112-byte limit, sends `null` and carries delivery debt when a certified RecoveryKit does not fit, and delivers it by the in-session kind-19 version-2 send of ADR-0237.
- **Date:** 2026-11-18
- **Class:** A new optional ceremony carrier is an `additive_noncritical_extension` under ADR-0165. The carrier has a private versioned namespace inside the existing ceremony envelope.

## Design sources read

Read in full for this decision: [Account Continuity and Control](../docs/architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md), [Recovery and Backup Policy](../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md), [Multi-Device Sync](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md), ADR-0028–0030, ADR-0127, ADR-0192, ADR-0202 and ADR-0205. Consulted the applicable passages in ADR-0190, ADR-0199, ADR-0201, ADR-0203, the [Home API](../spec/protocol/home-api.md), and the [dangerous algorithms supplement](../docs/architecture/IMPLEMENTATION_ALGORITHMS_AND_FAILURE_PATHS.md). The wider mapped Core, provider and UI design sources were not all read in full. This is a decision record. The product-code design gate applies before implementation.

## Decision

In Settings > Devices, the person opens the linked device's details and chooses **Make this a main device**. Reuse the short-lived QR or private-link exchange and matching-code confirmation from ADR-0192. The confirmation explains that a main device can approve changes to the account and help get the account back. Core supplies the eligibility and outcome. The device is shown as a main device only once its verified AccountControlState lists it as a controller.

Promotion is a separate operation on the already enrolled DeviceID. A promotion offer authenticates its purpose and version and binds the account, reconciled parent hash, selected DeviceID and parent-committed credential digest. The linked device proves possession of that credential over the fresh ceremony transcript. The comparison code covers the exact proposed change and its ceremony context. An enrollment or recovery approval is invalid in this exchange. A mismatched code, expiry, cancellation or a changed parent ends the exchange with no authority change.

The promotion child adds the selected device to the controller set and preserves messaging eligibility. The normal ACS verifier decides authorization. Signatures from the linked device on its ceremony request establish possession and contribute zero votes toward its promotion. With two eligible parent controllers, a device-only constitutional change requires both votes. When collecting a controller signature, show the specific transition for confirmation and bind the approval to the exact parent and child body. Persist the prepared body and approval context before sending. Check the verified parent again before signing and before adopting the child. Repeated delivery of an approval can complete only its original operation.

The verified promotion reply carries the existing RecoveryKit, when available, and the policy that the promoted state commits to, as contemplated by ADR-0205. Release the kit only to the verified target after the promotion commits. The receiver checks the policy commitment, the kit epoch and its own controller eligibility against the adopted head before storing the kit in DeviceVault. An account without a kit continues through the existing key replacement path. Later kit changes use the controller distribution in ADR-0199 section 5. The offline recovery key and the keys for conversation history remain separate.

Either eligible controller can host the existing owner recovery approval exchange under ADR-0202 section 3. The recovering device still supplies its recovery signature. A controller signature contributes one vote toward the committed parent quorum. For a parent with two controllers those two distinct votes meet the majority. The approval checks the existing veto, freshness and parent-reconciliation rules before completion.

## Private carrier registration

[The private controller-change schema](../spec/cddl/controller-change.cddl) registers the purpose `om.controller_change.v1`, request kind 1 and reply kind 2. The carrier uses the existing Home queues and ceremony envelope. A private link starts with `om:controller-change/`, followed by the operation digit (0 promotion, 1 removal), the originating DeviceID in lowercase hex and the ADR-0203 offer token, separated by slashes. The token uses version 1 and retains its 200-character bound. Enrollment and recovery approval decoders reject this prefix. Unknown controller-change versions end the exchange.

The key context is the canonical CBOR pair of byte strings containing the controller binding and the ADR-0203 route context. Use it with the ADR-0203 KDF, direction-specific AEAD, comparison code and local response mailbox derivation. The new context has a disjoint encoding from the ADR-0203 recovery context. Links live for 300 seconds. Receivers allow at most 60 seconds of clock skew at join. Plaintext stays within the existing 100,000-byte ceremony bound.

The binding names the account, parent, origin, selected device, selected credential and exact candidate ACS body hash. The selected device signs the request proof with its committed hybrid credential. The request has a single controller vote for removal or an ephemeral hybrid HPKE public key for promotion. The comparison code covers the versioned proof transcript. The selected device's signature additionally covers its ephemeral key. The originating controller signs the final response binding, including the hashes of the received request and response body. The kit uses the existing hybrid HPKE suite, with the kit binding as info and the committed policy bytes as associated data. The HPKE private seed stays with the selected device, even if someone else obtains the private link. The receiving device first verifies the origin signature and the ACS successor before opening and accepting any kit.

The reference flow opens the first promotion from one eligible controller. It also lets both eligible controllers approve a permanent removal in a two-controller account. The receiving device confirms the exact change before producing a vote. The originating controller confirms after the vote arrives. Core enforces the parent quorum and single-use confirmation. The person compares the codes. Before either device authorizes, it can cancel locally. After a vote is sent, the sender waits for the result or expiry. Newer parents end the original exchange. The current client has no ceremony for collecting additional votes for a further promotion or an ADR-0205 policy change with multiple controllers.

## Recovery record after controller exclusion

When a verified account state excludes a controller that held the current RecoveryKit, writes under that kit pause, including author writes, steward refreshes and queued retries. They resume only when the controller is eligible again or when confirmation through ADR-0205 activates a new recovery key. Permanent revocation is terminal for the DeviceID. An old record remains readable by the existing safety code during the pause. Its contents remain subject to the existing head and freshness checks. Devices with an unresolved fork cannot write recovery records.

Settings > Recovery offers **Make a new safety code** and explains that the old code still opens the saved recovery information and that OM has paused updates. Kit sharing stays disabled until the exclusion guard is implemented and tested. Writers treat every eligible controller in the current key lineage as a potential kit holder, including when delivery has an unknown outcome. Ordinary recovery advances the policy epoch while retaining the same root-derived record key, so the pause carries across that transition. A surviving controller can supply a verified descendant of the older record's pinned head through the existing recovery approval exchange. The older request ends. The person can explicitly ask again against the newer parent if it commits to the same policy.

## Enablement conditions

- A controller must durably detect a verified competing ACS branch, surface the existing conflict state and enforce the audited fork rules. Receiving or restarting on a competing branch must not make a stale parent eligible for signing.
- A request must still reach a second live controller when the first controller reads it first. The account-scoped Home inbox acknowledges one shared item. When the account has multiple controllers, authenticated requests stay available until the Home expires them. Junk can still be acknowledged, and vetoes remain on the separate board.
- A revoked controller cannot decrypt records written under the replacement key. Exercise the paused writer and the replacement lineage in a loopback using the removed controller’s old kit.
- The independent Python and Core codecs agree on the [controller transcript vector](../verification/vectors/controller-change-v1.json). Core exercises prefix and version rejection. Loopbacks cover promotion, kit transfer, rejected single and replayed votes, a co-signed transition, revocation, recovery approval from either controller, and restart.

## Current integration limits

The existing Home inbox registration binds the account to its first inbox capability. A replaced kit can use the new record locator immediately. An already registered inbox has no capability-rotation path in Home API v1. Owner-mediated approval uses its own ceremony queues. Verified sibling forks pause account changes pending the audited bridge. The bridge ceremony remains open.

## Where did authority go?

The parent ACS and its committed policy decide every vote. The exchange authenticates the two devices and their specific request. The Home carries opaque bytes. A RecoveryKit gives access to the recovery record and inbox.
