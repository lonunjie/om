# Recovery and Backup Policy

## 1. Product rule: recovery is optional and off by default

OM supports disposable identities as a first-class use case.

Default:

```text
RecoveryPolicy = none
```

If every authorized device is lost and no recovery method was configured, the identity is gone. OM must say this plainly but should not nag during first-run onboarding.

As an identity accumulates value—long-lived relationships, organizations, aliases, history—the client may gently offer **Protect this identity**. The user may ignore it forever.

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

Normal users see goals, not cryptographic terms:

- **No recovery — if I lose all devices, this identity is gone**
- **Recovery code — store this somewhere very safe**
- **Trusted people/places — require N of M**
- **Spare device/security key**

Advanced details remain inspectable.
