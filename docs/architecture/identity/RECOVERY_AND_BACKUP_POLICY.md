# Recovery and Backup Policy

- **Status:** Canonical architecture
- **Decision:** ADR-0111 (identity recovery on by default), superseding ADR-0026; ADR-0027, ADR-0039 unchanged
- **Gates:** V-002 (formal model), V-006 (passkey/PRF feasibility), V-018 (comprehension)

## 1. Product rule: identity recovery is on by default

Every new identity is created with a working RecoveryPolicy. The user does nothing extra to get it, can see
it in Settings, and can change it. They can also **explicitly** turn it off: disposable, unrecoverable identities
remain first-class, but they are an informed choice, not the default.

Why (ADR-0111): most people have one phone. With recovery off, one lost phone meant a lost identity. With one
phone plus one laptop, one lost device locked account changes forever under the majority rule. A messenger whose
common case is "lose your phone, lose everyone" fails the Wi-Fi test. It also pushes people back to operator-run
messengers that quietly hold their keys. The architecture below adds default recovery **without** creating a
recovery operator, a master key, or a silent takeover path.

```text
Default RecoveryPolicy (Recommended profile)
  methods:         [RecoveryRoot held by the best available custodian (§10.2)]
  recovers:        [identity, provider_metadata, relationships]   // history: separate, §8
  vote_weight:     1 vote in the controller majority (§10.4)
  activation:      alone -> 72 h delay with veto by any live controller
                   with >= 1 current controller co-signing -> immediate
  after recovery:  all earlier controllers revoked, recovery epoch rotated,
                   contacts see a continuity notice
```

## 2. Separate recovery targets

OM MUST separate:

- **Identity recovery** — authority to create a legitimate successor AccountControlState;
- **History recovery** — ability to decrypt an encrypted archive/backup;
- **Provider recovery** — credentials/configuration to find or replace relay/blob/provider resources.

No single recovery secret should automatically do all three.

> **Backups may restore history. Backups may never restore authority.**

## 3. RecoveryPolicy as a plug-in interface

The wire shape is frozen in [`account.cddl`](../../../spec/cddl/account.cddl) `RecoveryPolicy` (ADR-0119):

```text
RecoveryPolicy {
  state: on | pending | off_explicit          // "no recovery" is state = off_explicit, not a method
  methods[]                                   // kind + public commitment for this epoch
  authorization_rule: vote_weight = 1, immediate_requires_controller_cosign = true
  recovers: [identity | history | provider_metadata | relationships]
  activation: delay_seconds (>= 86400), veto_by = any_live_controller
  rotation_epoch?
}
```

The policy carries no commitment to itself. Its commitment is `Hash(OM-RECOVERY-POLICY-HASH-v1, body)`, stored in
AccountControlState `recovery_policy_commitment` (ADR-0119 ruling 4).

Method kinds (`recovery_method_kind`) can coexist:

- `synced_passkey_prf` (the default custodian, §10.2)
- `recovery_key` (the OfflineRecoveryRoot, §4)
- `guardians_shamir`
- `spare_controller`
- `hardware_authenticator`
- `future_threshold`

Enterprise or recovery-provider adapters must map onto one of these kinds and meet OM trust boundaries.

The protocol must not assume one vendor or one cryptographic construction forever.

## 4. Offline RecoveryRoot

Generate a **uniformly random 256-bit** recovery root. Human words are an encoding of random entropy, not a user-selected password.

Recommended representations:

- QR code / printable recovery sheet;
- checksummed text encoding;
- mnemonic encoding designed or selected after review.

A 256-bit mnemonic can be secure if it faithfully encodes machine-generated entropy and includes error detection. Do not let users choose the words.

BIP-39 should not be imported merely because it is familiar; its wallet-specific semantics are not automatically OM's semantics. SLIP-0039 is useful prior art for Shamir mnemonic shares and specifies 33-word shares for 256-bit share values, but it remains a separate specification that OM should adopt only after implementation/security review.

The RecoveryRoot authorizes recovery state; it is **not** the long-term message-history encryption key.

## 5. Guardians

A user can choose `N-of-M` guardians or offline locations. Initial implementation can use reviewed Shamir Secret Sharing over a random RecoveryRoot.

Example:

```text
3 of 5 required
share 1 -> trusted person
share 2 -> trusted person
share 3 -> printed safe
share 4 -> spare offline device
share 5 -> lawyer/family vault
```

Risks must be explained:

- N colluding guardians can reconstruct the root;
- lost shares can make recovery impossible;
- social relationships change;
- a share should reveal as little account metadata as possible.

Shares should be opaque and include enough checksummed metadata to prevent accidental cross-account/share-set mixing without exposing a public social graph.

## 6. Future threshold authorization

Longer term, threshold signatures/MPC can let guardians authorize recovery without reconstructing one master secret in one place. NIST began a formal multi-party threshold-scheme call in 2026, so OM should reserve this interface rather than invent a bespoke PQ threshold signature now.

Migration from Shamir-style recovery to a standardized threshold scheme should be possible by issuing a new RecoveryPolicy epoch under valid current authority.

## 7. Recovery activation defenses

Whenever recovery is on, every identity recovery **must** include (ADR-0111, ADR-0127):

- an activation delay of at least 24 h (profile defaults in §10.6);
- notification to every live control device, with a **veto** that any live controller can issue during the delay;
- immediate completion only when at least one current, non-quarantined controller co-signs (§10.3-10.4).

No profile can turn these off. The same rules apply to every other transition a RecoveryPolicy takes part in
(permanent revocation, retirement, promotion, recovery-policy change): there it is one vote, and alone it acts only
after the delay with no veto ([`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) §3, §4.1). It never resolves a freeze or fork.

Methods may add defenses on top: a guardian quorum, recovery-code proof, or optional out-of-band confirmation.

Disposable identities have none of this only because they have no recovery at all (`state = off_explicit`).

## 8. History backup

History uses an independent `BackupRecoveryPolicy` and encryption key hierarchy.

Backups may contain:

- encrypted message/event history;
- encrypted attachments according to retention policy;
- local contact labels/preferences where configured;
- historical control-state records for audit/context.

Backups MUST NOT make old control state authoritative. They also MUST NOT restore live MessageRequest bootstrap transport state: request-scoped reply private keys, `OPEN`/`RESERVED` request/candidate state and `COMPLETE_REPLAY_ONLY` tombstones from a historical backup are inert. A still-needed first-contact attempt is re-issued with fresh request/reply key material and route context (ADR-0147).

Restore sequence:

```text
restore encrypted history
        -> mark security/control records historical
        -> establish/fetch freshest authenticated AccountControlState
        -> reconcile AccountControlHead/witness/contact observations
        -> apply current revocations and CryptoProfile floor
        -> establish fresh sessions/rejoin-rekey groups as needed
        -> become fully operational
```

If freshness cannot be established, the client enters a restricted recovery state rather than silently trusting old authority.

**Async prekey freshness (ADR-0140).** A backup may carry the recipient's observed `(peer AccountID, peer DeviceID, CryptoProfile) -> (highest_generation, generation_core_hash)` records as encrypted local security state, but an old backup is never allowed to lower a fresher value learned from another surviving device/private-sync copy. Merge is monotonic: higher generation wins; equal generation with a different core hash is a security conflict. After restoring an old snapshot, the client treats prekey freshness as stale until it has merged every surviving local/private-sync copy and compared reachable provider pools. Provider `revokeGeneration()` is not a substitute for this state. If every copy of a newer observation was lost, OM cannot cryptographically reconstruct that observation; the client uses current relationship-pinned ACS authorization, expiry and the highest valid generation obtainable across reachable providers, and the residual first-observation limitation is surfaced rather than hidden.

## 9. Recovery UX

Normal users see goals, not cryptographic terms. The first option is the default (§10):

- **On: saved with your passkeys** (or **On: recovery key saved**): if you lose your phone, you can get your account back
- **Also save a recovery key** (print it or keep it in your password manager)
- **Trusted people**: any N of M can help you get back in
- **Spare device/security key**
- **Off: if I lose all my devices, this identity is gone** (explicit choice, with a one-time confirmation)

Advanced details remain inspectable.

## 2026-09-23 recovery resolution (default superseded 2026-09-24 by ADR-0111)

~~Recovery remains off by default.~~ Recovery is **on** by default (§1, §10). When enabled, the v1 root is random 256-bit OfflineRecoveryRoot material with domain-separated derivation; optional guardians use reviewed Shamir N-of-M sharing bound to the account/recovery generation. Recovery of account authority and recovery of history remain separate. Threshold/MPC signing is reserved rather than required.

## 10. Default recovery design (ADR-0111)

### 10.1 What is recovered, and what is not

| Target | Default | Secret | Notes |
|---|---|---|---|
| **Identity** (authority to install a new controller set) | on | RecoveryRoot, epoch-bound | always subject to §10.3 activation rules |
| **Provider metadata** (Home locator, RelaySet, provider credentials) | on | key derived from RecoveryRoot under a separate label | confers no authority |
| **Relationships** (contacts, their descriptors, delivery capabilities) | on | separate RelationshipSnapshot key, wrapped by the same custodian | small; lets a recovered user reach people again |
| **Message history** | off on T0, offered on T1+ | independent BackupRecoveryPolicy key (§8) | storage-heavy; never confers authority |

§2's rule becomes: **separate secrets, one convenient custodian.** Each target has its own domain-separated key.
The default custodian may wrap several of them. Compromising history or provider-metadata keys never confers
authority, and identity recovery always goes through §10.3.

### 10.2 Custodians (chosen automatically, visible in Settings)

The client picks the first available custodian and says, in one sentence, where recovery lives:

1. **Synced passkey (platform or password-manager vault)** with WebAuthn PRF. OM creates a passkey whose
   `user.id` is a random 32-byte **RecoveryHandle**, not the AccountID. This avoids directly encoding AccountID in
   `user.id`; it does **not** make the credential anonymous to the credential manager. The RP ID is visible,
   `user.name` remains the user's human recovery locator (their handle or Home address) for the existing recovery UX,
   and the platform may correlate those fields with its own telemetry. `user.name` is updated with
   `signalCurrentUserDetails` when the recovery locator changes. `RecoveryRoot = HKDF(PRF(passkey,
   "om/recovery-root/v1" ‖ epoch), "om recovery root" ‖ RecoveryHandle)`. No wrapped identity secret needs to be
   stored anywhere: the passkey *is* the custody.
   The Home stores only an encrypted **RecoveryRecord** under the opaque RecoveryStore locator defined below. It
   holds provider metadata and the RelationshipSnapshot (§10.1) but is not indexed by AccountID or a raw
   RecoveryHandle.
   *User sees:* "Recovery is on. It's saved with your passkeys (Google Password Manager)."
2. **Recovery key** (printable sheet or password-manager text: 256-bit root, checksummed, plus locator). This is the
   default when no PRF-capable synced passkey exists, and the default custodian under Maximum Privacy (§10.6).
   Everyone is also offered it as a second custodian after their first conversation.
3. **Trusted contacts** (guardians, Shamir M-of-N of a *second* recovery root). Offered once the user has at
   least three contacts. This is an additional method, never required.
4. **Spare device or hardware security key.**

If the automatic custodian cannot be established, as with a PRF-less browser and no saved key, the identity is
created with a **pending** recovery policy. The continuity check (§10.5) then stays visible until a custodian exists.

### 10.2.1 RecoveryRecord locator and capabilities (ADR-0138)

Each RecoveryStore advertises a random public `recovery_scope_id`. From `RecoveryRoot`, the client derives two
independent 32-byte capabilities with the selected profile's HKDF:

```text
read_cap  = KDF(RecoveryRoot, "OM-RECOVERY-RECORD-READ-CAP-v1"  || recovery_scope_id)
write_cap = KDF(RecoveryRoot, "OM-RECOVERY-RECORD-WRITE-CAP-v1" || recovery_scope_id)
locator   = DS-HASH("OM-RECOVERY-RECORD-LOCATOR-v1", read_cap)
```

The RecoveryStore indexes encrypted bytes by `locator`. `GET` requires `read_cap`; `PUT` requires the distinct
`write_cap`. These are provider/resource capabilities only: neither is an AccountControlState vote, RecoveryPolicy
vote or message-history key. The provider can correlate operations on the same record, but cannot derive AccountID
from the protocol values.

A restricted recovering device that reconstructs RecoveryRoot and knows the RecoveryStore endpoint/scope can
therefore fetch the encrypted record **before** account recovery completes, exactly as §10.3 requires. Loss of the
store may lose this convenience metadata but not identity authority.

#### RecoveryRecord v1 freshness and advisory semantics (ADR-0141)

The decrypted RecoveryRecord v1 is authenticated and carries `record_generation`, `recovery_epoch` and
`last_known_acs_head` as defined by [`recovery-record.logical.schema.json`](../../../spec/protocol/recovery-record.logical.schema.json).
For each RecoveryStore scope/record lineage, clients durably retain the highest accepted `record_generation` and merge
that value monotonically across private sync/backup. A lower generation is stale when a fresher observation survives;
the same generation with different authenticated plaintext is fail-closed equivocation. Restoring an old local backup
MUST NOT lower this high-water mark.

The whole record is **advisory and potentially stale**. `last_known_acs_head` is a remembered authenticated observation,
not a second authority object. Provider routes, capabilities, descriptors and other mutable snapshot data are
revalidated before reuse. A record that claims a newer recovery epoch or ACS head cannot authorize it; the normal
AccountControlState/recovery chain must establish that state.

In the normal profile, RecoveryRoot can derive the outer record/provider/relationship recovery keys, so compromise of
RecoveryRoot plus the RecoveryStore rendezvous can expose provider-recovery metadata and relationship metadata even
while the §10.3 delay/veto still protects account **authority**. Maximum Privacy may instead use an independently
generated `RelationshipRecoveryKey`, not derived from or wrapped by RecoveryRoot. The record then stores only a
second-layer encrypted relationship snapshot; losing that separate key loses relationship restoration convenience,
not AccountID authority.

### 10.3 Activation: no silent takeover

```text
RecoveryRequest(epoch, new_device_key) signed by RecoveryRoot
  -> delivered to every route of the account's Home(s)
  -> every live controller shows: "Someone is recovering your account on a new device. Was it you?"
       [That was me]  [Stop it]        (Stop = RecoveryVeto signed by a controller)
  -> co-signed by >= 1 current controller ........ completes immediately
  -> no veto for the full delay ................... completes (Recommended 72 h, Hardened/Maximum 7 d,
                                                    user-configurable, minimum 24 h)
  -> vetoed ....................................... aborted; the custodian is presumed compromised and
                                                    the client guides the user to rotate it
RecoveryCompletion: new device becomes the sole controller, every earlier controller is revoked,
                    recovery epoch increments, contacts see "Maria recovered her account on a new device".
```

- While a request is pending, the recovering device is in a **restricted recovery state**. It can fetch the
  RecoveryRecord and show "Your account will be back in 3 days. We wait so a thief can't take it." It cannot
  send as the identity.
- A request is bound to its target device key and recovery epoch. Completion is rejected if that device was
  enrolled or revoked by any other transition while the request was pending (found by the V-002 model).
- Homes rate-limit RecoveryRequests per account and drop duplicates; repeated vetoed requests trigger custody
  rotation, not an endless veto loop. RecoveryRequest fan-out necessarily exposes the request AccountID and the fact
  of a recovery attempt to each mailbox Home performing the fan-out; metadata descriptors declare that operation-
  scoped leakage rather than claiming `no_by_design`.
- The delay protects users who **still have a device**. A user who has lost every device and whose custodian is
  also compromised loses the race. That is the disclosed residual risk (T-041).

### 10.4 Recovery as one vote

The RecoveryRoot counts as **one vote** toward the controller majority `floor(N/2)+1`. It can vote immediately only
together with at least one current, non-quarantined controller; alone, it waits for the §10.3 delay. Consequences:

- **Two devices, one lost:** survivor + recovery = majority, so it immediately reconstitutes the controller set.
  The former constitutional lockout ([`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) §9) no longer exists by default.
- **Takeover cost:** an attacker needs `floor(N/2)+1` devices, or `floor(N/2)` devices plus the custodian, or the
  custodian plus the full delay without any live controller vetoing. This is shown in the ContinuityAssessment.

### 10.5 Continuity check under the new default

ContinuityAssessment ([`ACCOUNT_CONTINUITY_AND_CONTROL.md`](ACCOUNT_CONTINUITY_AND_CONTROL.md) §11) keeps running. A default account does **not** need an
acknowledgement. The persistent, non-blocking check now appears only when:

- recovery is **pending** (no custodian could be set up);
- the user **explicitly turned recovery off**: they confirm once, "I understand that losing this phone loses this
  identity," and the confirmation is re-requested whenever the assessment gets worse;
- the only custodian is on the same device that holds the only controller, as with a device-bound passkey. OM
  then suggests saving a recovery key.

### 10.6 Profiles

| Profile | Default custodian | Delay | Notes |
|---|---|---|---|
| Recommended | synced passkey, else recovery key | 72 h | the credential manager sees the OM RP ID, random RecoveryHandle and human recovery locator; platform telemetry may correlate them |
| Hardened | recovery key + synced passkey (both methods) | 7 d | guardians suggested |
| Maximum Privacy | recovery key only (no platform custody) | 7 d | may explicitly choose none; may hold RelationshipRecoveryKey separately from RecoveryRoot |

Profiles change **custody and delay**, never cryptographic floors (ADR-0037).

### 10.7 Honest limits

- The synced-passkey custodian inherits the security of the user's platform account and the platform's passkey
  sync encryption. Phishing that account plus defeating the device-passcode escrow lets an attacker *start*
  recovery, and the live-device veto is what stops it. The credential manager also sees the RP ID and WebAuthn user
  metadata described above; a random `user.id` is not an unlinkability guarantee.
- RecoveryRoot compromise has two different consequences: activation delay/veto still protects account authority, but
  in the normal profile the attacker may fetch/decrypt RecoveryRecord provider and relationship metadata. Maximum
  Privacy can split relationship-snapshot custody with an independent `RelationshipRecoveryKey`.
- Passkeys are bound to the RP ID of the client distribution that created them. Recovering through a different
  distribution needs the recovery key or guardians, or a device of the original distribution. V-006 records the
  per-browser and per-platform support matrix (PRF on synced passkeys, `signalCurrentUserDetails`).
- If the Home that holds the RecoveryRecord is gone, identity recovery still works: the authority lives in the
  RecoveryRoot, not the Home. Relationships then have to be rebuilt through contacts' continuity notices and
  re-introduction.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0120](../../../decisions/ADR-0120-one-to-one-session-manager-rules.md): backups never contain handshake prekey secrets (signed, one-time or last-resort) or ratchet state. They do contain per-peer downgrade-floor records and peers' validated control heads, which are not secret. A restored device publishes fresh prekeys.

## Withholding-resistant activation (ADR-0131)

For a recovery action with no current non-quarantined controller co-signature, the activation delay is **verifier
local**. Each verifier durably records the first valid observation of the RecoveryRequest digest and waits the full
RecoveryPolicy delay using the crash-safe monotonic elapsed-time lower-bound rule in ADR-0147. A verifier that was offline or partitioned starts its own interval
when it first sees the request. Across reboot, only durably checkpointed monotonic runtime counts; unmeasured downtime and wall-clock jumps never shorten the interval (except the bounded ADR-0154 web-verifier gap credit described below), and missing/rolled-back timer state restarts or otherwise conservatively lengthens the delay. Home receipt timestamps, request wall clocks and custodian assertions are never time
authority. Any valid RecoveryVeto observed before local activation prevents acceptance.

ADR-0154 keeps this usable on the PWA. A runtime segment is one OS boot session on native platforms, so app-process death does not reset credit. A web (`process_only`) verifier under the Recommended profile may credit wall-clock gaps between app sessions, forward of a durable high-water mark only. It accepts only after a final authenticated refresh of the recovering account's control state shows no RecoveryVeto and no superseding controller head. Hardened/Maximum verifiers keep the strict monotonic-only rule.

This deliberately makes a long-partitioned verifier slower to accept recovery. The trade is constitutional: a
RecoveryRoot holder cannot hide a request from live controllers, wait elsewhere and later claim the veto window has
already elapsed.

## Distribution-bound passkey recovery

A synced WebAuthn/passkey custodian is convenient but RP-scoped. For a PWA, passkey-only recovery is classified as
**recovery available, distribution-bound**, not portable recovery. The client should continue offering an
origin-independent method (recovery key, guardian quorum or supported hardware route) without blocking ordinary use.
V-006 must include loss of the original ClientHost/RP followed by recovery from an unrelated origin/native client.

## R15 reference-client presentation note

ADR-0159 leaves recovery authority/timers/custody unchanged. The reference UX must present the actual default custodian/state, keep identity recovery separate from history restore and provider/discovery reconnect, and distinguish device Suspend/quarantine, pending/final revocation and Safety Freeze states. See [`RECONCILED_INTERACTION_MODEL.md`](../ux/RECONCILED_INTERACTION_MODEL.md) and [`SCREEN_MAP_AND_JOURNEYS.md`](../../ux-design/SCREEN_MAP_AND_JOURNEYS.md).
