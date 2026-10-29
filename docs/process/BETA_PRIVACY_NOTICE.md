# OM open beta: privacy notice and abuse procedure

**Draft, not reviewed by counsel.** Status: open beta. No human cryptographer has audited OM. Data may be lost during the beta.
Contact for privacy and abuse reports: GitHub, using the [private report form](https://github.com/lonunjie/om/security/advisories/new) for anything sensitive or [public issues](https://github.com/lonunjie/om/issues) for everything else.

## Short version for users

- Your messages are sealed before they leave your device. The Home that holds them cannot read what you write.
- The Home and the web host can still see network-level facts: the internet address your requests come from, when you
  send and collect messages, and roughly how big a message is (padded to five size buckets, from 1 KiB to 128 KiB).
- Anyone who runs a Home can see those facts for the requests it receives. Timing and addresses can still link activity.
  OM does not hide your network address.
- Report illegal content through the [private report form](https://github.com/lonunjie/om/security/advisories/new). Do not paste the material into a public issue. We acknowledge within 7 days.

## What the hosted Home can observe

Source: [`docs/reports/2026-10-28-relay-threat-model.md`](../reports/2026-10-28-relay-threat-model.md), section 1 and 2.

- The ciphertext of every deposit, padded to one of five buckets (1024 / 4096 / 16384 / 65536 / 131072 plaintext bytes).
  The size reveals only the bucket.
- Opaque delivery and queue identifiers, and the time of each deposit, collection and drain.
- The volume and burst pattern of traffic, and which queues are active.
- The network address and timing of each request. The reference code does not store addresses, but a modified Home could log them.
- Invitation records. They are stored in clear under an unguessable id and can include a display label and a mailbox.
- Key bundles and one-time prekeys, which are public key material, and which addresses fetch them and when.

The Home cannot observe:

- message plaintext, attachments, or conversation structure;
- the sender's account, device or identity key for established one-to-one messages;
- the recipient's account identifier, or which queues belong to the same person or relationship.

Sealed delivery is not anonymity. Timing, network address and volume can still link activity.

The Home operator can also drop, delay, duplicate or refuse messages, and can delete queues it holds. A silent drop is
not visible to the sender.

## What the web host can observe

The site and client are served by Cloudflare Pages. Source: [`docs/architecture/providers/METADATA_OBSERVER_MATRIX.md`](../architecture/providers/METADATA_OBSERVER_MATRIX.md),
section 3, row "ClientHost origin".

- The network address and timing of each page or release fetch, and which release is selected.
- It does not receive OM account or device fields, and serving the client does not give it access to your account.

## Abuse and illegal-content reports (draft procedure)

1. **Receive.** Reports go through the private GitHub report form with the title `OM abuse`. The reporter includes what they received,
   when, and from which OM address or Home if known. The Home cannot read message content, so it cannot check the
   content itself. The assessment works from the report.
2. **Acknowledge** within **7 days**.
3. **Assess** whether the material is likely illegal, and whether the report is complete.
4. **Remove** what the Home operator can remove: queued ciphertext it holds, and any invitation record. It cannot delete
   content from a recipient's device.
5. **Preserve and refer** as the law requires: keep the report and the Home's own records that the law requires, and
   refer to the relevant authority where required. Do not keep more than the law requires.
6. **Tell the reporter** what was done, where the law allows.

Counsel has not reviewed this procedure. Before the beta drops the label, or at about 1,000 monthly active accounts,
or on any legal request, this notice and procedure must be reviewed by counsel.
