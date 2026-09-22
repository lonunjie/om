# Identifier Spoofing and Safe Human Display

## Threat

Cryptographic identity can be correct while a human-readable alias is deceptive through Unicode confusables, mixed scripts, misleading domains or copied avatars/display names.

## Direction

At first-contact/discovery surfaces:

- retain exact user-authored Unicode;
- evaluate aliases/security-sensitive names with current Unicode UTS #39 confusable and mixed-script mechanisms;
- do not ban legitimate non-Latin names merely for using another script;
- surface calm warnings only when the identifier meaningfully resembles another/security-relevant identifier;
- show canonical domain/address detail on demand and before high-impact first-contact actions;
- establish trust through AccountID/contact continuity after acceptance, not continued string comparison.

## Domains/links

Internationalized domains should have a reveal/canonical-detail affordance. External link destination cannot be altered by themes. Suspicious display-label vs destination mismatches get clear UI.

## Avatars/display names

These are presentation claims and can be copied. They never imply verification. Avoid blue-check-style global trust semantics in the protocol.
