# Release digest log (single-operator)

`digests.jsonl` records, for each signed web release, its version, source commit and the sha256 of `site/bundle-manifest.json` and of `SHA256SUMS`.
Each line is signed by the delegated release key (same root -> delegation -> key chain as the bundle) and carries the sha256 of the previous line
(`prev`; the first line uses 64 zeros). `delegations/<sha256>.json` holds the delegations the signatures refer to.

**What this is not.** This is a single-operator log. It is **not** an independent transparency log: one custodian holds every key that signs
it, nobody else monitors it, there are no witnesses, mirrors or gossip, and the custodian could rewrite and re-sign the whole file. It
makes an after-the-fact edit visible only to someone who kept an earlier copy. SeedLife makes **no claim of transparency** for it
(ADR-0183 waives PR-005 criteria 5 and 7: independent mirrors, and a transparency log with independent monitors; both stay open).

Verify (no dependencies beyond Node 24): `cd apps/web && node build/digest-log.mjs verify ../../deploy/release-log/digests.jsonl`.
Append: `node build/release.mjs --sign-from-vault --append-digest-log`, then commit the new line and any new `delegations/` file in the same change.
Lines are never edited or removed; a wrong entry is superseded by a new release, not rewritten.
