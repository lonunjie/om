# Metadata Trace Test Plan (V-015)

- **Status:** Test plan (V-015 pre-engineering criterion). Executed on real W0 builds as the V-015 pre-release criterion.
- **Claims under test:** [`spec/metadata/w0-metadata-exposure-descriptors.json`](../../spec/metadata/w0-metadata-exposure-descriptors.json),
  already checked against the abstract flow model in [`verification/bounded/metadata_trace_model.py`](../../verification/bounded/metadata_trace_model.py).

## Principle

Every descriptor field is a **falsifiable claim** about bytes a provider can see. The test harness records what
each provider role actually receives, extracts the observable fields, and runs the same comparison as the abstract
model: no under-claim, no over-claim, and never a canonical identifier.

## Capture points

| Role | Capture | Tool |
|---|---|---|
| Home (relay/T0/T1), key material, blob, IntroGate, discovery, recovery store | Home request logs at full verbosity, plus TLS-terminated request bodies on a test deployment | Home debug log tap; for Cloudflare, `wrangler tail` on a test account; for the static binary, its structured log |
| Network observer | client ↔ provider traffic | `tcpdump`/Wireshark on a test network; the browser via the Chrome DevTools Protocol network domain |
| Push provider | payload as submitted to APNs/FCM/Web Push | the Home's outgoing push request log |
| ClientHost origin | origin access logs | host access logs |
| Platform credential manager | WebAuthn ceremony parameters (rpId, user.id, user.name) | browser WebAuthn event log (CDP) |

## Extraction

A detector for each field scans the captured bytes. `stable_account_id`, `conversation_id` and long-term keys are
seeded test values and are searched for literally, in any encoding (hex, base64, base64url, CBOR). Timing and size
fields are present whenever a request is logged. `contact_graph_edge` is present when one request links two
test identities.

## Flows × profiles

Run every flow from the model (`send_1to1`, `first_contact`, `attachment`, `recovery`, `pwa_load`, plus T0
redemption, T1 membership, migration and multi-device) under Recommended, Hardened and Maximum Privacy, on each
reference Home (Cloudflare and static binary), in Chromium, Firefox and WebKit, and on one native shell.

## Pass rule

Every run's extracted observations satisfy the descriptors, and the UI privacy claims ([`METADATA_THREAT_MODEL.md`](../threat-model/METADATA_THREAT_MODEL.md) §18)
match the observations under each profile. Any mismatch fixes either the implementation or the descriptor, and a
descriptor change needs an ADR.
