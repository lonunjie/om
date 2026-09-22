# Research references — decentralized services, bridges and portability

- **Status:** Research/evidence, non-normative
- **Checkpoint:** 2026-09-23 07:56 PHT

## Telegram bots

Official Telegram documentation (retrieved 2026-09-23):

- https://core.telegram.org/bots
- https://core.telegram.org/bots/features
- https://core.telegram.org/api/bots
- https://core.telegram.org/api/bots/managed-bots

Observed architecture points:

- the common creation flow uses `@BotFather` to register a bot and issue its authentication token;
- bots are special Telegram accounts and are visibly labeled/differentiated from ordinary users;
- bot accounts have network-defined behavioral differences such as contact initiation/privacy behavior;
- newer managed-bot APIs allow user-triggered creation through MTProto without manually chatting with BotFather, but the identity/account, manager eligibility and creation limits remain Telegram-controlled network state;
- the ease of creation and permission/request UX is useful inspiration, while its central account issuer is intentionally not copied by OM.

## Matrix application services / federation

Official Matrix specification/docs:

- https://spec.matrix.org/latest/application-service-api/
- https://matrix.org/docs/matrix-concepts/elements-of-matrix/

Observed architecture points:

- Matrix is federated across homeservers rather than having one network server;
- Application Services can bridge external systems and can control/observe configured namespaces;
- current AS registration is homeserver-admin configuration; older HTTP self-registration was removed because overly broad re-registration could expose traffic;
- OM should learn from the explicit gateway/bridge role while avoiding broad ambient namespace/read authority for ordinary installed services.

## ActivityPub actor vocabulary

W3C references:

- https://www.w3.org/TR/activitypub/
- https://www.w3.org/wiki/ActivityPub/Primer/Actors

Useful observation:

- network actors need not be collapsed into one human-only type; Person/Application/Service-style distinctions can describe actor role without pretending to determine cognition or biological identity.

OM goes further by making principal type an authorization/security distinction and deliberately declining to infer whether content was AI-generated.
