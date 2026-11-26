# OM Matrix bridge adapter

This sans-IO library is for a replaceable endpoint outside the OM client on a
Matrix test network. Configure the homeserver HTTPS origin at the endpoint.
The host owns HTTPS transport, Matrix device authentication, OM MLS membership
and consent, and storage. OM end-to-end encryption terminates at the bridge
for exported content.

The adapter handles joined-room sync, event sends and redaction requests. It
uses plain text fallback for rich text, inert Matrix media references for
attachments, mapped replies and threads, text edits and reaction additions.
Redactions are requests. Poll exports are read-only summaries provided by the
host. Foreign display falls back to the Matrix user ID. The identity link
lookup uses Core's verified link state for the exact handle.

## Host integration

- Use the authenticated binding, current Room and endpoint state, and the
  Room-scoped fingerprint secret. Confirm the Matrix room is unencrypted and
  approved for plaintext. Establish the initial sync cursor under the host's
  history policy and handle limited timelines before advancing it.
- Persist the exact prepared request before dispatch. Recheck Room authority
  at dispatch and retry using the same Matrix device, URL and bytes. Resolve
  uncertain sends before processing potentially matching inbound events.
  Persist the target and echo table with its binding incarnation.
- Serialize import check, durable materialization and commit per Room.
  Restore verified dedup state from the Room journal before consuming sync.
  The host owns Room event construction and authorization. Preserve the
  prepared provenance and use the adapter verdict for dedup eligibility, so
  an unverified observed leg does not enter the verified index. Record refusal
  reasons locally and surface fidelity notices from the returned actions.

## Evidence and scope

The offline tests use frozen JSON and a two-room fake homeserver with two
bridges and a Room model. No real homeserver was run. The tests do not exercise
Matrix E2EE, room discovery, media transfer, profile lookup, Matrix proof
issuance, reaction removal or live OM MLS. Test-network evidence is still
needed for preservation of the experimental Matrix content key, particularly
on the dedicated redaction endpoint.

Matrix wire shapes follow the [Matrix Client-Server API](https://spec.matrix.org/v1.18/client-server-api/).
