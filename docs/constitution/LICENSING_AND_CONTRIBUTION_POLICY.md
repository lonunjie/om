# OM Licensing and Contribution Policy

- **Status:** Canonical licensing architecture; standard license texts are authoritative for their legal terms
- **Checkpoint:** 2026-09-22 23:02 PHT
- **Purpose:** Keep OM implementable, forkable and commercially usable without allowing the reference commons to be enclosed

## 1. Licensing is layered by role

One license is not optimal for every OM artifact. The project uses a deliberate licensing stack.

| Material | License | Reason |
|---|---|---|
| `om-core`, shared client/security/state libraries and reusable official client components | **MPL-2.0** | File-level copyleft: changes to covered core files distributed to others remain open, while independent clients can combine the core with differently licensed code. Includes contributor patent grants. |
| OM-operated/reference relay, discovery, provider, provisioning and other network-service implementations | **AGPL-3.0-or-later** | Modifications to the OM reference network-service code offered to users over a network remain source-available under the AGPL. Independent providers may still implement the public OM protocol from scratch under other licenses. |
| Normative protocol schemas, registries, test vectors, conformance harness interfaces and protocol-reference code intended for universal reuse | **Apache-2.0** | Maximizes independent implementation/reuse while carrying explicit copyright and contributor patent grants. These artifacts must not force adoption of the reference application code. |
| Architecture, explanatory specifications, governance prose, diagrams and project documentation | **CC-BY-SA-4.0** | Anyone may copy/adapt them, commercially or otherwise, but shared adaptations remain under the same terms and attribution is preserved. Embedded code explicitly marked Apache-2.0/MPL-2.0/AGPL retains its stated software license. |

`LICENSE.md`, SPDX headers, and the machine file registry determine the license class of each canonical file.

## 2. Why OM Core uses MPL rather than a permissive-only license

A permissive-only license would maximize reuse but would allow a distributor to take OM Core, improve the covered core itself, distribute the result, and keep those modifications private.

MPL-2.0 places copyleft at the file level. It protects the shared core while still allowing a larger proprietary or differently licensed application around it. That matches OM's goal: **the common protocol/security machinery stays improvable as a commons without forcing every OM-compatible application to use one business model.**

## 3. Why network reference providers use AGPL

Traditional distribution-triggered copyleft does not require publication merely because modified software is run as a hosted service. OM reference provider software exists specifically to be run over networks, so the reference implementations use AGPL-3.0-or-later.

This does not force every OM-compatible provider to be AGPL. The protocol is independently implementable from public Apache-licensed normative artifacts. It only prevents taking OM's own AGPL provider implementation, modifying it into a hosted derivative, and using the network boundary to avoid the source-sharing obligation defined by the AGPL.

## 4. Why the protocol itself is permissively implementable

OM's goal is not to force every compatible implementation to share one source license. It is to prevent anyone from becoming the gatekeeper of communication.

Normative wire artifacts and conformance material therefore use Apache-2.0. A university, hobbyist, phone vendor, accessibility project, business, alternate client, or future fork can implement the protocol without asking SeedLife for copyright permission.

Protocol freedom is protected by architecture/governance rules in addition to copyright licensing:

- no secret mandatory extension;
- no vendor-only endpoint;
- no trademark requirement for interoperability;
- public registries and test vectors;
- royalty-free patent requirement for canonical normative additions;
- public change records;
- ability to fork the specification and implementation.

## 5. Contribution policy: no ordinary copyright assignment

OM's normal contribution path is **DCO 1.1 sign-off plus inbound-equals-outbound licensing**.

Contributors keep their copyright. By submitting, they certify they have the right to contribute the material under the license declared for that file/component.

OM SHOULD NOT require a broad CLA that transfers copyright or gives SeedLife unilateral permission to relicense the entire community codebase under proprietary terms.

A narrowly scoped agreement may be considered later only when required for a specific legal purpose and only after public review showing that it does not create a capture path.

## 6. Protocol patent rule

Copyright licenses alone do not fully solve protocol-patent risk. Before external normative protocol contributions are accepted, OM will publish lawyer-reviewed contribution language that requires relevant patent disclosure and an irrevocable, worldwide, royalty-free patent license/non-assert commitment sufficient for conforming implementations of the accepted contribution.

Until that text is finalized:

- canonical OM MUST NOT knowingly adopt a royalty-bearing or permission-gated protocol dependency;
- suspected patent encumbrance is a blocking architecture/legal question;
- Apache-2.0's patent terms remain useful for covered contributions but are not treated as a substitute for a protocol-specific RF review.

## 7. Trademarks are not protocol locks

Project names/logos may later be protected to reduce impersonation and protect users. Trademark policy must permit truthful compatibility statements such as “OM-compatible” while distinguishing official/canonical distributions from modified ones.

No trademark license may be required to implement or interoperate with the protocol.

If stewardship of the trademark fails, open-source and protocol rights remain usable under another project name.

## 8. Relicensing rule

No future steward may represent that it can make already released OM versions closed source or revoke previously granted rights.

Where a future license change is proposed for new work:

- it requires a public ADR and compatibility/legal analysis;
- contributor rights must be respected;
- existing releases retain their existing licenses;
- the change must preserve `PROJECT_PHILOSOPHY_AND_ANTI_CAPTURE.md`.

## 9. Commercial use

Commercial use is explicitly compatible with OM's philosophy. Anti-capture is achieved through open rights, copyleft of the shared implementations where appropriate, provider replaceability, and public protocol governance—not by discriminating against businesses.

## 10. Pre-publication legal gate

Before accepting broad external contributions or making definitive project-wide legal claims, counsel familiar with open-source licensing/patents should review:

- the file-class mapping;
- DCO workflow;
- protocol RF patent contribution terms;
- trademark policy;
- compatibility between mixed-license components and packaging/distribution targets.

That review may refine legal mechanics but must not silently weaken the anti-capture intent.
