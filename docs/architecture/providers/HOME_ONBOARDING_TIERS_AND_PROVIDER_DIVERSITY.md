# Home Onboarding Tiers and Provider Diversity

- **Status:** Canonical architecture
- **Checkpoint:** 2026-09-24 02:29 PHT
- **Decision:** ADR-0102; amended by ADR-0130 (§2 same-operator roles, §3.3–3.5 T0 exposure, revocation overlap and client origin)
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

**Roles run by one operator collude by construction (ADR-0130 rule 4).** In the reference Home one deployment serves
the signed ContactDescriptor, runs the IntroGate and holds the mailbox. Whoever operates that deployment (the user's
cloud vendor, the T1 host or the T0 sponsor) can link the published contact identity (alias, intro identity) to
every deposit into and drain from that mailbox. OM does not claim otherwise. The MetadataExposureDescriptors, the
privacy UI and this document treat co-located roles as one observer, and every non-collusion assumption names roles
run by **different** operators. The reference Home still gives each role its own credentials, storage namespace and
request logs as defense in depth, so a leak of one role's logs or keys is not a leak of all three; that is hygiene,
not a privacy claim.

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
- `may_sponsor_further` is always `false` in the baseline (the frozen CDDL and schema make it a constant), so one Home cannot be turned into an unbounded chain. T1 Homes grant mailboxes under their own policy, not by chained sponsorship;
- per-sponsor cap on concurrent sponsored mailboxes, enforced at the sponsor's Home;
- sponsorship consumes the **sponsor's** budget under the sponsor's zero-paid-budget policy (R-016). Exhaustion fails closed at the sponsor's Home; it never bills.

### 3.3 Honest metadata exposure

The sponsor knows whom it invited. Its Home can observe the StarterMailbox's delivery timing, volume and sizes, but
not content. It can also see **contact edges among its co-resident users**: when a contact of the invitee also has a
mailbox on the same Home, the same client address or session that drains one mailbox deposits into the other, so the
edge is observable even though each deposit capability is opaque. Opaque capabilities hide the identities behind
deposits only from contacts whose mailboxes live elsewhere. The sponsor is also often exactly the proximate adversary
of the threat model (partner, parent, employer; T-084): it sees when messages arrive and it can close the mailbox,
bounded by §3.4.

The client shows this once, in plain language, and the wording depends on where the app itself came from (§3.5):

> You're using **Alice's OM home** for now. Her home can see when you get messages and who else on it you talk to,
> and Alice can close it after a 30-day notice. It can't read your messages. We'll add a second home for you so
> you're not relying on hers.

"Alice can't read your messages" is true only when the app is **not** served by Alice's Home or hosting account.
If the invitee explicitly chose Alice's app server (§3.5), the disclosure says instead that Alice's server supplies
the app and is trusted like the device itself.

The MetadataExposureDescriptor for the StarterMailbox role must state this (V-015 trace), including
`contact_graph_edge: sometimes` for co-resident contacts.

### 3.4 Graduation and expiry

- **Move to my own home** is always one tap away. It runs normal RelaySet migration with overlap.
- Expiry warnings start well before `expires_at`. Expiry closes the mailbox, never the identity. Contacts retain other routes where the user added them.
- **Sponsor revocation has an overlap window (R-266).** A sponsor's revocation stops new ticket issuance at once, but
  the Home keeps accepting deposits into the StarterMailbox and serving drains for **at least 30 days** (the same floor
  as T1 eviction, R-187), announces the closure through the mailbox and answers deposits with a typed "moved" response
  once the user has published a new route. The Home enforces this; conformance case PC-D06 tests it. During the
  window the sponsor may lower quotas to the grant's minimums but may not close the mailbox. Abuse by the invitee is
  bounded by those quotas, not by instant closure.
- **A second route that is not the sponsor's (R-267).** Soon after first run (by default within the first week, as
  soon as one is reachable) the client adds a fallback route on a Home the sponsor does not operate: a T1 Home from
  the distribution's Home list, a sponsorship offered by another contact, or the user's own Home. Contacts learn both
  routes through the normal RelaySet. The §3.3 disclosure says this will happen.
- When the overlap window ends, the client has already moved the user to another tier. Relationships survive through
  overlap routes or re-introduction; there is no global fallback capability (R-092).

### 3.5 Where a T0 user's client comes from (ADR-0130 rule 1)

A T0 user has no hosting account, so the client code they run must come from somewhere else, and that choice decides
who is part of their endpoint trust base (F-07). The rules:

1. **Never the sponsor by default.** A sponsor's Home, and any origin served from the sponsor's hosting account, never
   serves the invitee's ClientHost unless the invitee explicitly opts in ("Use Alice's app server"). If they opt in,
   the client treats the sponsor as part of the endpoint trust base and the §3.3 disclosure says so. This extends
   R-188 from T1 to T0.
2. **Always a choice with a native path.** An invite that carries a SponsorshipTicket offers, on the same landing
   page: a native app (store or direct download, where the platform allows) **and** at least two PWA origins that are
   shared ClientHosts ([`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) §9) run by independent operators. No single origin is
   the only way in, so no single hosting account is a network-wide endpoint compromise.
3. **A project-controlled origin is a declared service.** If the project or the reference distribution runs a PWA
   origin, it is a project-controlled service under PR-001, PR-005 and PR-012, serves only transparency-logged
   releases, is monitored by independent release-transparency monitors, and is offered next to the independent
   origins and the native option, never alone. It gains no protocol privilege (R-046, ADR-0043).
4. **The origin is named.** Onboarding names the origin that serves the app, and Settings → About shows it (CLIENTHOST
   §9 rule 4). Moving to another origin or a native client is new-device enrollment.

The comprehension study (V-018) tests this choice and the §3.3 disclosure.

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
3. The client records the release digest it runs and compares it with ≥2 independent release-transparency observations when online (log and monitor roles and failure behavior: SOFTWARE_SUPPLY_CHAIN §6; gate V-023). This detects non-targeted substitution; it does not defeat a fully compromised origin ([`CLIENTHOST_UPDATE_LIFECYCLE.md`](../client/CLIENTHOST_UPDATE_LIFECYCLE.md) §3).
4. Hardened/Maximum profiles recommend a native client or an independently hosted, transparency-monitored ClientHost over a same-account ClientHost.

## 6. Provider diversity and monoculture

Monoculture is a **declared 0M risk** (threat T-020). Controls:

1. **Second reference provider before public production (PR-013, V-020).** At least one reference Home implementation with no cloud-vendor-specific dependency must pass the same provider conformance suite. The recommended target is a single static binary with embedded SQLite, runnable on a VPS, home server or single-board computer, and it doubles as the T1 community-host software.
2. **Recommended RelaySet diversity.** Recommended profile's active+fallback prefers a fallback on a different provider family when one is known, for example the sponsor's Home or a T1 Home. Hardened already seeks evidenced diversity (Q-012).
3. **Vendor-wide block drill (V-020 pre-release).** Simulate the reference cloud vendor disabling every OM Home at once. Measure how many relationships retain a route, and prove that users can re-home through surviving routes or re-introduction without AccountID change.
4. **Second-family fallback by default (R-194).** The V-020 model ([`verification/bounded/home_tiers_model.py`](../../../verification/bounded/home_tiers_model.py),
   20 seeded 400-user invite graphs, Cloudflare-first mix) finds that a Cloudflare-wide block leaves **~2%** of
   relationships reachable when no one has a fallback route. With a fallback on another provider family for the 60%
   of users who can get one, **~43%** stay reachable. So the T1 host console and the Recommended profile add a
   second-family fallback route by default wherever one is reachable, such as a T3 community Home or a sponsor on
   another family. Growing that pool of non-Cloudflare Homes is a launch goal, not an afterthought.
5. **Distribution defaults.** The reference distribution must not make one vendor the only zero-signup path. T0 sponsorship is vendor-neutral because it lives wherever the sponsor's Home lives.

## 7. Onboarding UX (replaces `UI_UX_SYSTEM.md` §Onboarding step 3 when an invite is present)

```text
Opened from an invite with sponsorship:
  Invite page: Get the app (native) | Open in browser (choice of independent origins, §3.5)
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
6. A T1 host serves mail, not its members' client code, unless a member explicitly opts in (R-188). The same holds
   for a T0 sponsor (R-260).
7. Roles run by one operator are one observer; descriptors and UI never claim non-collusion between them (R-265).
8. A sponsor cannot cut an invitee off instantly: revocation keeps a 30-day overlap window (R-266).
