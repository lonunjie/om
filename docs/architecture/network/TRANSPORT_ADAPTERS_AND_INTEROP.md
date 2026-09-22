# Transport Adapters and Interoperability

## Principle

Conversation semantics are independent of transport.

```text
conversation / canonical event model
              |
         routing layer
              |
  +-----------+-----------+-----------+
  |           |           |           |
OM native    SMS/MMS      RCS       bridges/future
```

The first implementation ships only what is needed for the OM-native secure product. Carrier and store approvals must not block core delivery.

## Initial scope

`OmNativeTransport` is the only required user-message transport for the first daily-driver release.

Calls are part of OM-native capability and can be developed before carrier transport integrations.

## Future SMS/MMS

Android may later add `SmsMmsTransport` if product/store/default-app requirements are satisfied. Carrier messages normalize into the local conversation model but preserve provenance/security attributes.

SMS/MMS must never be visually or cryptographically represented as equivalent to OM-native E2EE.

## Future RCS

`RcsTransport` remains provider/platform/carrier dependent. OM must not depend on Google/Jibe or any one carrier interface to function.

Where authorized APIs exist, an adapter translates available RCS features into canonical OM semantics. Missing features are capability differences, not reasons to redesign the event model.

## Transport capability model

Each adapter advertises structured capabilities, e.g.:

```text
TransportCapabilities {
  e2ee_level
  max_text_size
  attachments
  edit
  delete
  reactions
  receipts
  typing
  group_semantics
  calling?
  identity_binding
  metadata_exposure
}
```

The UI can present human language such as “Private OM message” or “SMS” while the routing layer uses detailed capability data.

## No silent downgrade

If an OM-native recipient becomes temporarily unreachable, the app must not silently resend plaintext via SMS/RCS. A transport security change requires explicit user policy/intent and clear presentation.

## Bridges

Canonical external-network bridge semantics are frozen in `INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`.

XMPP/Matrix/Telegram/email/SMS/RCS/business/future bridges are typed `BridgePrincipal` trust boundaries. Foreign actors remain bridge-attested provenance rather than synthetic OM AccountIDs. If a bridge receives plaintext to transform/deliver it, users must be told that the external side is outside OM-native E2EE.

Bridge adapters advertise semantic/fidelity capabilities and carry loop/dedup lineage. Unsupported edits/deletes/reactions/topics/polls degrade explicitly rather than silently changing meaning.

## Roadmap socket

Core code should define `TransportAdapter` from the beginning even when only `OmNativeTransport` exists. This avoids later architecture surgery while keeping initial scope lean.
