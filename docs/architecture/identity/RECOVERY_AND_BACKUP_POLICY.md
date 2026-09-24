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

Conceptual policy:

```text
RecoveryPolicy {
  version
  methods[]
  authorization_rule
  recovers: [identity | history | provider-metadata]
  activation_delay_or_challenge_policy
  rotation_epoch
  policy_commitment
}
```

Supported/future methods can coexist:

- `None`
- `OfflineRecoveryRoot`
- `ShamirGuardianShares`
- `SpareControlDevice`
- `HardwareAuthenticator`
- `FutureThresholdAuthorization`
- enterprise/recovery-provider adapters that meet OM trust boundaries.

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

Depending on the user's profile, identity recovery may include:

- configurable time delay;
- notification to surviving control devices;
- challenge/abort window;
- guardian quorum;
- recovery-code proof;
- optional out-of-band confirmation.

Maximum-privacy/disposable identities may intentionally choose none of these because they have no recovery at all.

## 8. History backup

History uses an independent `BackupRecoveryPolicy` and encryption key hierarchy.

Backups may contain:

- encrypted message/event history;
- encrypted attachments according to retention policy;
- local contact labels/preferences where configured;
- historical control-state records for audit/context.

Backups MUST NOT make old control state authoritative.

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
   `user.id` is a random 32-byte **RecoveryHandle**, not the AccountID, so the credential manager cannot link it to
   the OM identity. `user.name` is the user's human recovery locator (their handle or Home address), updated with
   `signalCurrentUserDetails` when they move Home. `RecoveryRoot = HKDF(PRF(passkey, "om/recovery-root/v1" ‖ epoch),
   "om recovery root" ‖ RecoveryHandle)`. No wrapped secret needs to be stored anywhere: the passkey *is* the custody.
   The Home stores only an encrypted **RecoveryRecord**, indexed by `H(RecoveryHandle)`, that holds the provider
   metadata and RelationshipSnapshot (§10.1).
   *User sees:* "Recovery is on. It's saved with your passkeys (Google Password Manager)."
2. **Recovery key** (printable sheet or password-manager text: 256-bit root, checksummed, plus locator). This is the
   default when no PRF-capable synced passkey exists, and the default custodian under Maximum Privacy (§10.6).
   Everyone is also offered it as a second custodian after their first conversation.
3. **Trusted contacts** (guardians, Shamir M-of-N of a *second* recovery root). Offered once the user has at
   least three contacts. This is an additional method, never required.
4. **Spare device or hardware security key.**

If the automatic custodian cannot be established, as with a PRF-less browser and no saved key, the identity is
created with a **pending** recovery policy. The continuity check (§10.5) then stays visible until a custodian exists.

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
  rotation, not an endless veto loop.
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
| Recommended | synced passkey, else recovery key | 72 h | the platform credential vault learns "uses OM" (the rpId) plus a random handle |
| Hardened | recovery key + synced passkey (both methods) | 7 d | guardians suggested |
| Maximum Privacy | recovery key only (no platform custody) | 7 d | may explicitly choose none |

Profiles change **custody and delay**, never cryptographic floors (ADR-0037).

### 10.7 Honest limits

- The synced-passkey custodian inherits the security of the user's platform account and the platform's passkey
  sync encryption. Phishing that account plus defeating the device-passcode escrow lets an attacker *start*
  recovery, and the live-device veto is what stops it.
- Passkeys are bound to the RP ID of the client distribution that created them. Recovering through a different
  distribution needs the recovery key or guardians, or a device of the original distribution. V-006 records the
  per-browser and per-platform support matrix (PRF on synced passkeys, `signalCurrentUserDetails`).
- If the Home that holds the RecoveryRecord is gone, identity recovery still works: the authority lives in the
  RecoveryRoot, not the Home. Relationships then have to be rebuilt through contacts' continuity notices and
  re-introduction.

## 2026-09-24 verification rulings

Normative, from the V-series models. The ADR is authoritative where this summary is shorter.

- [ADR-0120](../../../decisions/ADR-0120-one-to-one-session-manager-rules.md): backups never contain handshake prekey secrets (signed, one-time or last-resort) or ratchet state. They do contain per-peer downgrade-floor records and peers' validated control heads, which are not secret. A restored device publishes fresh prekeys.
