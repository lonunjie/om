# ADR-0196 — A person's own nickname for a contact: local, in Core's contact record, in the backup, never sent

- **Status:** Accepted
- **Date:** 2026-11-17
- **Decided by:** operator delegation 2026-11-17 ("easy by default, user in control"), to fill silence. This ADR fills silence only and amends no audited text.
- **Class:** local-state addition. No wire object, no registry entry and no `spec/` schema changes (the nickname is never encoded into any message); the one persisted-format change is the body of the `OMB-1` backup file of [ADR-0184](ADR-0184-chat-history-backup-file.md), handled as section 4 below. Under [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) this is not an extension of a frozen W0 object and adds no mandatory field to one.
- **Amends:** nothing. **Does not amend:** [`GROUPS_AND_SPACES_UX.md`](../docs/ux-design/specs/GROUPS_AND_SPACES_UX.md) section 10 and [`PRIVACY_SECURITY_INTERACTION_UX.md`](../docs/ux-design/specs/PRIVACY_SECURITY_INTERACTION_UX.md) section 14 (they name "user-local nickname" as a cue and define no model; this ADR gives it one without changing their text), [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md) (`PersonRecord` is local/private contact state), [`IDENTIFIER_SPOOFING_AND_DISPLAY.md`](../docs/architecture/identity/IDENTIFIER_SPOOFING_AND_DISPLAY.md), [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md) (including the ADR-0148 preference allowlist, which is not extended), the OMB-1 header, profile, key schedule, allow-list principle and "restoring grants nothing" rules of ADR-0184, and the design-gate rules.
- **Scope:** `core_kernel` (state, intent, view, backup body) and `reference_client_ui` (contact detail sheet, shown names). Evidence: relay status D126 to D129, [`machine/requirements.jsonl`](../machine/requirements.jsonl) R-436, threat T-180.

## Design sources read

Read in full: [`AGENTS.md`](../AGENTS.md) "Design gate"; [`IDENTIFIER_SPOOFING_AND_DISPLAY.md`](../docs/architecture/identity/IDENTIFIER_SPOOFING_AND_DISPLAY.md); [ADR-0184](ADR-0184-chat-history-backup-file.md); [`GROUPS_AND_SPACES_UX.md`](../docs/ux-design/specs/GROUPS_AND_SPACES_UX.md) section 10; [`PRIVACY_SECURITY_INTERACTION_UX.md`](../docs/ux-design/specs/PRIVACY_SECURITY_INTERACTION_UX.md) section 14.

Read in part: [`IDENTITY_PROTOCOL.md`](../docs/architecture/identity/IDENTITY_PROTOCOL.md) (`PersonRecord` and the AccountID sections, by search); [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md) (the account-private preference channel, ADR-0148 allowlist and the backup-restore paragraph); [`PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md`](../docs/process/PROTOCOL_EVOLUTION_AND_CHANGE_CONTROL.md) (classes and rules, by search); [`IMPORT_EXPORT_AND_USER_ESCAPE.md`](../docs/architecture/ecosystem/IMPORT_EXPORT_AND_USER_ESCAPE.md) (contact and alias mentions); the chat-backup report.

Not read (stated, not claimed): the other documents and ADRs that `python3 tools/design_gate.py needs crates/om-core/src/backup.rs crates/om-core/src/state.rs packages/om-ui/src` prints for `client_endpoint_distribution`, `crypto_1to1`, `discovery_first_contact`, `identity_control_recovery` (beyond the two above), `metadata_privacy`, `relay_reachability`, `time_ordering_events` and `ux_accessibility_personalization`. The change touches no wire, key, route, delivery or time semantics; a later reader should not take the list above as a full gate reading.

## Context

Duplicate and lookalike display names are the main impersonation risk in groups. GROUPS_AND_SPACES_UX section 10 and PRIVACY_SECURITY_INTERACTION_UX section 14 list a "user-local nickname" among the cues and define no model. D114 shipped the other cues (pattern, "1 of 2", one calm line) and left the nickname open, because it needs a stored field and therefore a backup-format decision. A display name is a claim the other person chose for themselves; a nickname is what the user chooses to call them, so it is the one cue that a copied name cannot imitate.

## Decision

1. **A nickname is one optional text field of the contact record in Core.** `Relationship.nickname` (and `RestoredChat.nickname` for history restored from a file), absent by default. It is local contact state in the sense of `PersonRecord` ("how the user remembers a human"): it is not an alias claim, not an identity and not evidence of continuity.
2. **One intent, `SetContactNickname { conversationId, nickname? }`.** Present and non-empty sets it; absent, empty or whitespace-only clears it. It is not destructive (no confirmation token), is idempotent, and is device-local. Core trims, requires 1 to 64 characters after trimming, and refuses control characters and the bidirectional controls (U+061C, U+200E, U+200F, U+202A to U+202E, U+2066 to U+2069) as `Rejected(invalid)`, so a nickname cannot rewrite the surrounding security chrome (IS-03). Text is kept exactly as typed otherwise (no case folding, no script ban; IS-01). An unknown conversation is `Rejected(not_found)`.
3. **It is never sent.** No wire object, introduction, invite, group message, group profile, notice or relay call carries it, and `SetContactNickname` itself produces no network call or outbox entry. A test replays the traffic of a full conversation and asserts that the number of deposits does not change when a nickname is set, that no deposited byte string contains it, and that the contact's own state does not contain it.
4. **It is in the backup, by a body version bump, and old files still restore.** The OMB-1 header (magic, version byte 1, profile, key schedule) is unchanged. The body of ADR-0184 gains an optional per-conversation key 6, `nickname`. The body version is 2 when at least one chat has a nickname and stays 1 otherwise, so a backup of a profile with no nicknames is byte for byte what it was (the committed vector still holds) and an older build keeps reading it. The decoder accepts body versions 1 and 2; a version 1 body means "no nicknames". A build that predates this ADR refuses a version 2 body as `invalid_content` instead of restoring it with the nicknames silently dropped. The allow-list of ADR-0184 section 5 gains this one field and nothing else. Restoring grants nothing (ADR-0184 section 6) and a conversation already live on the device is left alone, as before, so a restore never overwrites a nickname the person set here.
5. **Per device, not synced.** [`MULTI_DEVICE_SYNC.md`](../docs/architecture/crypto/MULTI_DEVICE_SYNC.md) describes an account-private preference channel with an ADR-0148 allowlist (mute, labels, pins, appearance). Core has no such channel today, and the nickname is not on the allowlist, so it is per device. When the channel exists, adding the nickname to it is its own ADR (a typed deterministic merge, never authority). Until then the backup file is how a nickname travels between a person's devices, and the Devices story says nothing about it.
6. **Nickname wins where a name is shown; the original stays secondary.** Everywhere a contact's name is shown to the person (list, conversation header, people sheet, group sender lines, add and pick lists, invitations, search results, Omnibar labels), the shown name is the nickname when there is one, else the display name. The contact detail sheet shows the name the contact chose for itself as "Their name", always. The Core views carry the nickname beside `displayName` and never replace it, so a client can always show the original. The name in a group is the name this device's contact list gives the account (`group_contact_name`), so it follows the nickname too. A nickname is never shown as something the contact said.
7. **Same-name cues use the shown name.** The D114 rule (NFKC, case fold, trim, exact equality) is applied to the shown name, so two contacts both called "Alex" stop colliding once one has a nickname, and a nickname equal to another contact's shown name collides like any two names do.
8. **A contact detail sheet in the reference client.** Reachable from the conversation header: the name as shown, "Their name" when a nickname hides it, the pattern avatar (UI only, as D114), and the nickname field with Save and Clear. It has no safety-number, AccountID or verification row, because no screen, form or encoding for them is defined in the design (see "Not done" and D129); it names no person as verified.

## Where did authority go?

Nowhere. The nickname grants nothing, is read by no one but its owner and is never part of identity, sorting for the network, routing or any decision of Core other than display. It cannot be used to claim that two accounts are one person: IS-04 (no silent rebinding) is untouched because the key of the record stays the conversation and the account.

## Not done, and why

- No AccountID display form, safety number or verification row in the sheet: the design defines none (IS-02 requires them to be "independently reachable" but no document fixes a form). Recorded as D129; nothing is invented.
- No UTS #39 skeleton or mixed-script matching: still the open item of D114.
- No sync to the person's own other devices (section 5).
- No search by nickname beyond what the shown name gives the local index.

## Consequences

- A person can make two "Alex" apart in their own words in two taps, and the choice survives a backup and restore.
- Restoring into a profile that already has the conversation live does not carry the nickname over; the person sets it again or keeps the one on that device.
- A backup with a nickname cannot be read by an older build; a backup without one can be read by every build.

## Verification

Rust: body version 1 and 2 vectors, decode of a version 1 body and of version 2 with and without the key, round trip through a real export and import, bounds and bidi refusal, a refusal of version 3, state persistence across restart, and the wire-capture test of section 3. TypeScript: harness parity, shown-name helper, sheet and list tests with axe. Chromium e2e: set a nickname, see it in the list, reload, back up and restore into a fresh profile.
