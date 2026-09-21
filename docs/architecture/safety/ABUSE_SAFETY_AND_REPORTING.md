# Abuse, Safety, and Reporting

## Goal

Reduce spam, harassment, fraud, and resource abuse without creating universal plaintext access or a global social-control identity system.

## First-contact boundary

Unknown senders use constrained IntroductionEndpoints / MessageRequests:

- tiny bounded payloads;
- strict sender/capability rate limits;
- no unsolicited large attachments;
- no unsolicited group addition;
- no direct calls by default;
- cheap validation before expensive storage/fanout/crypto work;
- optional escalating proof/resource requirements if attacks demand it.

Acceptance creates private relationship delivery capabilities.

## Enforcement boundary

Controls that a hostile modified client could remove must be enforced by recipients/relays/providers. UI-only spam limits are not security controls.

## Blocking

Blocks are primarily local/private. Where a provider needs a deny rule to avoid resource consumption, use scoped opaque capability/endpoint identifiers rather than mandatory global identity publication.

## Reporting

A user may intentionally disclose selected abusive content through a **Report Evidence Bundle**. The design goal is to provide enough authenticated evidence to evaluate the report while not granting the platform passive access to all conversations.

Logical report bundle may include:

```text
selected message plaintext chosen by reporter
corresponding authenticated event/envelope evidence
conversation/account/device context strictly required for validation
report category and local annotations
optional media objects explicitly selected by reporter
```

The exact cryptographic evidence mechanism requires careful design so a malicious reporter cannot trivially fabricate another user's authorship while also preserving deniability properties where expected.

## No mandatory global reputation score

OM should not create a universal numeric trust/reputation score. Provider/community-local abuse decisions can exist, and users may subscribe to optional safety services, but no score becomes part of root identity or protocol validity.

## Provider denial

A relay/media/storage operator may deny service according to its terms. Because providers are replaceable, provider denial is not equivalent to erasing a user's identity from OM.

## Public/discovery spaces

If OM later adds public directories, public groups, or searchable services, stronger moderation can apply there because those spaces have different expectations from private accepted relationships. Do not use “private E2EE” as a reason to make public discovery ungoverned.

## Children/vulnerable users

Any future safety features for minors should avoid universal scanning backdoors. Prefer account/device policy, contact approval, known-adult supervision models where legally/ethically appropriate, constrained discovery, and user-visible reporting controls.

## Abuse-resistance tests

- malicious client cannot bypass IntroductionEndpoint quotas;
- reporting reveals only intentionally selected evidence;
- report mechanism cannot silently become bulk conversation export;
- relay blocking does not require plaintext;
- resource exhaustion is bounded before expensive work;
- provider ban does not mutate AccountID;
- no global trust score is required for ordinary delivery.
