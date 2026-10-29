# Security policy

**Status: OM is in open beta.** The web client at `om.lonunjie.org` is labelled "Beta". No human cryptographer has
audited it. We make no security claims for it, and data may be lost during the beta. Do not rely on OM for anything
you cannot afford to lose or expose. The beta is a label, not a warning wall: the product works as described on the
[Security & privacy section of the site](https://om.lonunjie.org/#security).

## Scope

In scope:

- The web client at `om.lonunjie.org` (the code in `apps/web` and the packages it uses).
- The reference Home (the helper service that holds sealed messages until a device collects them).
- The deploy kit (`deploy/` and the release build in `apps/web/build`), including release signing and verification.

Out of scope: third-party platforms, services and devices, and copies of OM run by other people. Report problems in those
to their owners. If a deployed copy is run by someone else, tell us as well, because we may be able to warn its users.

## How to report

Report privately with GitHub's [private vulnerability report](https://github.com/lonunjie/om/security/advisories/new) form (it needs a GitHub account and is visible only to the maintainers). Include steps to reproduce, the
affected version or URL, and your handle if you want credit.

- Please do not post exploit details publicly, and do not test against people's data or other users' accounts.
- Design flaws that are not exploitable anywhere yet may be filed as a public issue on
  [GitHub](https://github.com/lonunjie/om/issues), with the document, section and requirement or threat id.

We acknowledge reports within **7 days**.

## Coordinated disclosure

- We investigate, fix, and agree a disclosure date with you.
- The default **embargo is 90 days** from the report. We may shorten it when a fix is out and users are protected, and we
  may extend it by agreement when a fix needs more time.
- We credit reporters who want credit.

## Advisories (interim)

Until the signed advisory channel exists, advisories are published as **signed git tags in the public repository**.
This is an interim measure. Verify the tag signature before you trust its content.

## Bounty

There is no bounty programme.

## Safe harbour (draft, not reviewed by counsel)

If you research OM in good faith and follow this policy, we will not start legal action against you for that research.
This does not cover actions that break the law, that access other people's data, or that disrupt services for others.
This paragraph is a draft and has not been reviewed by counsel.

## Related

- [Beta privacy notice and abuse procedure](docs/process/BETA_PRIVACY_NOTICE.md) (draft, not reviewed by counsel): what the
  hosted Home and the web host can observe, and how to report illegal content.
- [Security & privacy section of the site](https://om.lonunjie.org/#security).
- Implementer contact list (voluntary): [`docs/process/IMPLEMENTER_CONTACTS.md`](docs/process/IMPLEMENTER_CONTACTS.md).
- Design of the vulnerability process: [`VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md`](docs/architecture/providers/VULNERABILITY_RESPONSE_AND_SECURITY_ADVISORIES.md).
