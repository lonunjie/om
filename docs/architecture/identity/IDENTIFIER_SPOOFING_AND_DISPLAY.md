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

## CONFORMANCE-IDENTIFIER-SPOOFING

The following are normative pass/fail cases for the registered W0 conformance suite. Passing these cases is evidence; it does not by itself authorize a scope.

| ID | Case | Pass condition |
|---|---|---|
| IS-01 | confusable display names | UI preserves stable disambiguation path and never treats name as identity proof |
| IS-02 | copied avatar/name | safety number/AccountID verification remains independently reachable |
| IS-03 | bidi/control characters | rendering is bounded and cannot rewrite surrounding security chrome |
| IS-04 | alias changes owner | prior relationship identity is not silently rebound |
| IS-05 | service/bridge imitates person | typed principal indicator remains reserved and visible |
