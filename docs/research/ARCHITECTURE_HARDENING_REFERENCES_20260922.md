# Architecture Hardening References — 2026-09-22

This note records external sources that materially constrain the 2026-09-22 23:02 PHT architecture-hardening checkpoint. It is research evidence, not a substitute for the normative OM documents.

## Open-source / licensing

### Mozilla Public License 2.0

- https://www.mozilla.org/en-US/MPL/2.0/
- https://www.mozilla.org/en-US/MPL/2.0/FAQ/

Relevant constraints:

- MPL 2.0 is file-level copyleft: modifications to covered files remain under MPL when distributed, while separate files in a larger work may use other licenses.
- Section 2 provides copyright and contributor patent grants.
- This makes MPL suitable for `om-core`: preserve improvements to covered core files without forcing every independent OM-compatible application to use one license.

### GNU Affero General Public License v3

- https://www.gnu.org/licenses/agpl-3.0.html

Relevant constraint:

- Section 13 adds a source-offer obligation for users interacting remotely over a network with a modified version. This is why OM reference network-service implementations use AGPL rather than relying solely on distribution-triggered copyleft.

### Apache License 2.0

- https://www.apache.org/licenses/LICENSE-2.0.html

Relevant constraints:

- broad perpetual copyright license;
- explicit contributor patent grant and patent-termination terms;
- useful for normative schemas/conformance/protocol-reference material intended for widespread independent implementation.

### Developer Certificate of Origin 1.1

- https://developercertificate.org/

Relevant constraint:

- contributor certifies the right to submit the contribution under the license indicated in the file; it does not itself transfer copyright to the project steward.

### Open Source Definition

- https://opensource.org/osd
- https://opensource.org/definition-annotated

Relevant constraint:

- an open-source license cannot discriminate against persons/groups or fields of endeavor, including business use. OM anti-capture therefore cannot be implemented as a “no business use” restriction while still calling the software open source.

### Creative Commons BY-SA 4.0

- https://creativecommons.org/licenses/by-sa/4.0/

Relevant constraint:

- documentation may be copied/adapted, including commercially, with attribution and ShareAlike obligations for adaptations.

## Asynchronous 1:1 and multi-device messaging

### Signal PQXDH

- https://signal.org/docs/specifications/pqxdh/

Relevant constraint:

- PQXDH is explicitly asynchronous: the recipient can be offline after publishing handshake information to a server. OM therefore needs a first-class public key-material publication/fetch abstraction and cannot model secure asynchronous initiation as only “know AccountID, then send.”

### Signal Sesame

- https://signal.org/docs/specifications/sesame/

Relevant constraints:

- asynchronous multi-device messaging introduces a session-management layer above the handshake/ratchet;
- it must handle multiple devices, device addition/removal, simultaneous initiation, orphaned/restored sessions, unreliable/reordered delivery and per-device session state.
- OM may reuse the problem decomposition, but its device-authority source is AccountControlState rather than a centralized service record.

## MLS groups

### RFC 9420 — Messaging Layer Security

- https://www.rfc-editor.org/rfc/rfc9420.html

Relevant constraint:

- Section 14 states that applications MUST establish a way to resolve conflicting Commit messages for the same epoch, either by preventing them or choosing a canonical commit. OM therefore needs an application-level GroupCommitOrdering profile and cannot leave this responsibility implicit in MLS/OpenMLS.

## Web/PWA security and LAN capabilities

### W3C Web Cryptography API

- https://www.w3.org/TR/WebCryptoAPI/

Relevant constraints:

- script injection is equivalent to remote code execution in the origin and can enable key/data exfiltration or misuse;
- origin storage can be cleared by users;
- non-extractable CryptoKey handling does not make hostile same-origin code trustworthy.
- OM therefore treats ClientHost/origin integrity as a security boundary and does not claim in-PWA verification can recover a fully compromised origin.

### Local Network Access draft

- https://wicg.github.io/local-network-access/

Relevant constraint:

- local network access from public websites is moving toward explicit permission-gated behavior. OM therefore treats native automatic discovery and PWA LAN reachability/discovery as platform capabilities rather than promising identical discovery on every client.

## Calls/media

### RFC 9605 — SFrame

- https://www.rfc-editor.org/rfc/rfc9605.html

Relevant constraints:

- SFrame key management is application-provided;
- replay handling is outside the specification;
- sender-key rotation/join/leave behavior must be supplied by the application.

### WebRTC Encoded Transform / RTCRtpScriptTransform

- https://developer.mozilla.org/en-US/docs/Web/API/RTCRtpScriptTransform
- https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Using_Encoded_Transforms

Relevant constraint:

- current browsers expose worker-based encoded media transforms suitable for application-layer E2EE, but older clients may not support the API. Capability detection/fallback remains required.
