# Home Onboarding Tiers and Provider Diversity

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decision:** ADR-0102
- **Gates:** V-020 (pre-engineering + pre-release), PR-013
- **Supersedes in part:** the "v1 premise: user-owned Cloudflare deployment" framing in [`PROVISIONING_AND_BOOTSTRAP.md`](PROVISIONING_AND_BOOTSTRAP.md) §1. Cloudflare remains the first reference *own-cloud* tier. It is no longer the only way in.

## 1. Why the old premise fails OM's own tests

The 09:20 onboarding made every new user create a hosting account and deploy a Worker before sending a first message. That fails three constitutional tests:

1. **The Wi-Fi test** (AGENTS.md, R-010). A user who cannot configure Wi-Fi cannot be expected to create and secure a cloud account. Some deploy-button flows also require a Git-forge account; verify the current flow before relying on it.
2. **0M / R-145.** If nearly every Home runs the same template on one vendor, that vendor's terms-of-service decision, abuse filter or outage can remove most of the network at once. No protocol rule is broken; the choke point is practical. It is still an indispensable universal permission boundary.
3. **Endpoint trust (F-07).** For PWA users, the hosting account that serves the ClientHost origin can ship arbitrary JavaScript to that user. A phished cloud password becomes an E2EE break.

The protocol already supports every provider shape needed. What was missing was the **onboarding and diversity architecture**.

## 2. Home tiers

A **Home** is the set of provider roles (relay/mailbox, IntroGate, discovery publisher, optional blob/push) serving one IdentityProfile. All tiers use the same public provider contracts. No tier is privileged in protocol.

| Tier | Who runs it | Signup needed | Typical user |
|---|---|---|---|
| **T0 Sponsored starter mailbox** | The inviter's Home (or any Home that offers sponsorship) | none | Everyone who joins through an invite |
| **T1 Hosted Home** | A household member, community, organization or commercial/managed provider | the host's own policy | Families, clubs, companies, people who prefer a service |
| **T2 Own cloud** | The user, in their own cloud account (Cloudflare reference first) | cloud account | Enthusiasts, sovereignty-minded users |
| **T3 Self-hosted** | The user, on a VPS, home server or single-board computer | none beyond the machine | Technical users, communities running T1 for others |
| **T4 Local only** | No WAN Home; LAN/QR peers | none | Offline/specialty (existing [`LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md`](../network/LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md)) |

A user may hold several tiers at once (active + fallback RelaySet). Moving between tiers is RelaySet migration and never changes AccountID (R-001, R-052).

## 3. T0 — Sponsored starter mailbox

### 3.1 Flow

```text
Alice (has a Home) creates an invite
  -> invite payload includes an optional single-use SponsorshipTicket
Bob opens invite, taps "Join"
  -> Bob's client generates AccountID/keys locally (unchanged)
  -> Bob redeems SponsorshipTicket at Alice's Home
  -> Alice's Home creates a bounded StarterMailbox and returns
     opaque deposit/drain capabilities (normal RelaySet entries)
  -> Bob and Alice are connected; Bob can chat immediately
```

The ticket is an ordinary IntroGate-style bearer capability. It is scoped to "create one StarterMailbox", short-lived, single-use and revocable by Alice. It carries no AccountID.

### 3.2 Bounds (SponsoredMailboxGrant)

Schema: [`spec/protocol/sponsored-mailbox-grant.logical.schema.json`](../../../spec/protocol/sponsored-mailbox-grant.logical.schema.json).

- hard quotas: stored bytes, envelopes per hour, max envelope size, max blob spool, retention;
- `expires_at` with renewal by the sponsor (clock class: provider wall-clock, non-authoritative; see [`TIME_AND_EXPIRY_SEMANTICS.md`](../events/TIME_AND_EXPIRY_SEMANTICS.md));
- `may_sponsor_further = false` by default, so one Home cannot be turned into an unbounded chain;
- per-sponsor cap on concurrent sponsored mailboxes, enforced at the sponsor's Home;
- sponsorship consumes the **sponsor's** budget under the sponsor's zero-paid-budget policy (R-016). Exhaustion fails closed at the sponsor's Home; it never bills.

### 3.3 Honest metadata exposure

The sponsor knows whom it invited. Its Home can observe the StarterMailbox's delivery timing, volume and sizes, but not content and not the identities behind other contacts' opaque deposit capabilities. The client shows this once, in plain language:

> You're using **Alice's OM home** for now. Alice can't read your messages, but her home can see when you get them. You can move to your own home anytime.

The MetadataExposureDescriptor for the StarterMailbox role must state this (V-015 trace).

### 3.4 Graduation and expiry

- **Move to my own home** is always one tap away. It runs normal RelaySet migration with overlap.
- Expiry warnings start well before `expires_at`. Expiry closes the mailbox, never the identity. Contacts retain other routes where the user added them.
- If the sponsor revokes the mailbox, the client guides the user to another tier. Relationships survive through overlap routes or re-introduction; there is no global fallback capability (R-092).

## 4. T1 — Hosted Homes and the Home list

- A Hosted Home grants mailboxes under its own policy, terms and jurisdiction. It is responsible for the service it offers ([`OPERATORLESS_CORE_AND_ECOSYSTEM.md`](../../constitution/OPERATORLESS_CORE_AND_ECOSYSTEM.md)).
- A distribution may ship a **replaceable Home list** (like a directory list). The list must be user-editable. It must not be protocol state, and it must not rank any operator, including SeedLife, as protocol-privileged (R-048, R-107).
- If SeedLife ever runs a Hosted Home, it is one entry among others, and it must pass the same PR-001 role review as any project-controlled service.

### 4.1 Cloudflare is the first T1 reference (ADR-0112)

Growth runs through **hosted Homes**, not only personal clouds. One household member, club organizer or small
organization runs a Home, and everyone they invite gets a mailbox there. This is the adoption pattern that worked
for independent and federated messengers, and it pairs with T0 sponsorship as the invite loop:
*Alice runs a Home; Bob joins by invite (T0); Bob's family joins Alice's Home (T1).*

The **first T1 reference implementation runs on Cloudflare**. It is the same Worker and SQLite Durable Object code
base as the T2 own-cloud reference ([`CLOUDFLARE_REFERENCE_DEPLOYMENT.md`](CLOUDFLARE_REFERENCE_DEPLOYMENT.md)),
made **multi-tenant from W0**:

- one Durable Object per hosted mailbox. Per-member quotas and the host's zero-paid-budget policy are enforced at the
  Home, so exhaustion fails closed and never bills (R-016);
- a host console to invite members, set quotas, suspend or evict, and hand members off when the Home closes, with at
  least 30 days of overlap routes (R-187);
- the host's MetadataExposureDescriptor states what a T1 host can observe: member delivery timing, volume and
  sizes. It cannot observe content or contacts behind opaque capabilities (V-015).

The Cloudflare-first choice is about **delivery order**, not privilege. The second reference Home (single static
binary + SQLite, V-020/PR-013) is still required before public-production claims, and it is the natural T1 software
for communities that prefer not to use a cloud vendor.

### 4.2 A T1 host must not serve its members' client code by default (R-188)

A T1 host that also served the PWA ClientHost for its members would turn the host's hosting-account security into
**every member's endpoint security** (F-07, T-021, T-042). One phished host account could ship malicious JavaScript
to a whole community. Therefore:

1. Members of a T1 Home use an **independent client distribution** by default: a native app, or a PWA origin
   that the host does not control and that is monitored by release transparency.
2. A host may offer a same-origin ClientHost only as an explicit, labeled choice ("Use my community's app
   server"). The client then treats the host as part of the endpoint trust base and says so.
3. The reference host console never deploys a ClientHost into the mailbox Home's account.

## 5. T2 — Own cloud, with account security as endpoint security

For PWA users, the hosting account that serves the ClientHost origin is part of the endpoint trust base (F-07, T-021). Requirements (R-164):

1. Onboarding requires or strongly steers to phishing-resistant auth (passkey/security key) on the hosting account where the provider supports it, and explains why in one sentence.
2. ClientHost deployment and relay/Home deployment use **separate least-privilege credentials** and, where the provider allows, separate scripts/origins (already required: separate public and app origins).
3. The client records the release digest it runs and compares it with ≥2 independent release-transparency observations when online. This detects non-targeted substitution; it does not defeat a fully compromised origin ([`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) §3).
4. Hardened/Maximum profiles recommend a native client or an independently hosted, transparency-monitored ClientHost over a same-account ClientHost.

## 6. Provider diversity and monoculture

Monoculture is a **declared 0M risk** (threat T-020). Controls:

1. **Second reference provider before public production (PR-013, V-020).** At least one reference Home implementation with no cloud-vendor-specific dependency must pass the same provider conformance suite. The recommended target is a single static binary with embedded SQLite, runnable on a VPS, home server or single-board computer, and it doubles as the T1 community-host software.
2. **Recommended RelaySet diversity.** Recommended profile's active+fallback prefers a fallback on a different provider family when one is known, for example the sponsor's Home or a T1 Home. Hardened already seeks evidenced diversity (Q-012).
3. **Vendor-wide block drill (V-020 pre-release).** Simulate the reference cloud vendor disabling every OM Home at once. Measure how many relationships retain a route, and prove that users can re-home through surviving routes or re-introduction without AccountID change.
4. **Distribution defaults.** The reference distribution must not make one vendor the only zero-signup path. T0 sponsorship is vendor-neutral because it lives wherever the sponsor's Home lives.

## 7. Onboarding UX (replaces `UI_UX_SYSTEM.md` §Onboarding step 3 when an invite is present)

```text
Opened from an invite with sponsorship:
  Welcome -> Profile -> "You're in. Using Alice's home for now." -> Chat with Alice
Opened without an invite:
  Welcome -> Profile -> Where should your messages live?
     • Use a community home   (T1 list)
     • Set up my own home     (T2/T3 automated)
     • I already have a home  (link/QR)
```

No vendor names or infrastructure vocabulary appear outside Advanced. The continuity check (ADR-0106) appears after the first conversation, not before it.

## 8. Invariants

1. No tier is required by the protocol. Every tier uses public provider contracts.
2. Tier changes never change AccountID or canonical conversation meaning.
3. Sponsorship grants mailbox capacity only: no identity, control, discovery or content authority.
4. Sponsored capacity is bounded, revocable and paid for only by the sponsor's own policy.
5. A distribution's Home list is user-replaceable data, never protocol truth.
6. A T1 host serves mail, not its members' client code, unless a member explicitly opts in (R-188).
