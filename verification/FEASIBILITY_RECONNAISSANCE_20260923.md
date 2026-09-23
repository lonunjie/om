# Verification Feasibility Reconnaissance — 2026-09-23

This is **evidence planning**, not gate passage and not product engineering.

## V-001 — production crypto path

Current Signal `libsignal` remains a strong behavioral/reference source, and its underlying implementation is Rust, but its own repository explicitly says **use outside Signal is unsupported** and its APIs/implementations may change without notice. OM therefore cannot close V-001 by simply declaring `libsignal` its stable compatibility/runtime contract.

Source checked 2026-09-23:
- https://github.com/signalapp/libsignal/blob/main/README.md

Implication for OM:
- keep the architecture-selected PQXDH + Triple-Ratchet/SPQR-class profile;
- do not substitute a different wire design just because the easiest library API differs;
- V-001 still needs a Rust native+wasm state-machine feasibility prototype, persistence/reload tests, vectors, one-time-key handling, and a support/licensing/update story that does not depend on an unsupported private API.

## V-005 — canonical encoding vectors

This checkpoint now contains two independently written deterministic-CBOR foundation encoders (Python and Node.js) that agree on the current primitive fixture corpus. However, the canonical architecture says signed/hashed v1 objects will have frozen CDDL/field registries and exact domain-separation strings, and the repository does **not yet contain those per-object CDDL/field registries**.

Implication:
- this is a verification-artifact gap, not a reason to change the deterministic-CBOR decision;
- V-005 cannot pass until every signed/hashed v1 object has a frozen wire field map/CDDL + positive/negative cross-implementation vectors.

## V-006 — Web DeviceVault

WebAuthn Level 3 specifies the `prf` extension and explicitly describes using PRF outputs for symmetric encryption keys. MDN also documents `prf`, while noting that extension/authenticator support varies and PRF evaluation at credential creation is not universally available.

Sources checked 2026-09-23:
- https://www.w3.org/TR/webauthn/#sctn-prf-extension
- https://developer.mozilla.org/en-US/docs/Web/API/Web_Authentication_API/WebAuthn_extensions

Implication:
- the architecture is technically grounded, but V-006 still requires an actual browser+authenticator matrix;
- the reference PWA must detect capability at runtime and must not assume every passkey/authenticator exposes PRF in the same lifecycle phase;
- non-extractable WebCrypto key behavior, origin migration, backup/restore and recovery paths remain experimental proof obligations.

## V-007 — SFrame/WebRTC

SFrame is an IETF Standards Track mechanism in RFC 9605 and is explicitly designed for E2EE media through SFUs. Modern browsers expose WebRTC Encoded Transforms through `RTCRtpScriptTransform`; MDN currently labels that API Baseline 2025 and says it is available across current browser/device versions since October 2025. This makes a browser implementation path materially more plausible than it was earlier.

Sources checked 2026-09-23:
- https://www.rfc-editor.org/rfc/rfc9605.html
- https://developer.mozilla.org/en-US/docs/Web/API/RTCRtpScriptTransform
- https://developer.mozilla.org/en-US/docs/Web/API/WebRTC_API/Using_Encoded_Transforms

Caution:
- encoded-transform availability is not itself SFrame interoperability proof;
- counter/replay behavior, reconnect, simulcast/SVC, SFU forwarding and membership rekey still need actual cross-browser/native tests;
- upstream native WebRTC SFrame integration is still evolving, so OM must test what ships rather than infer support from source work.

## V-010 — plugin sandbox

The WebAssembly Component Model remains under active incremental development in WASI developer-preview releases. Its own design goals include eventual native browser integration and, before that, browser polyfilling/AOT compilation. Bytecode Alliance `jco` documents browser-side component transpilation.

Sources checked 2026-09-23:
- https://github.com/WebAssembly/component-model
- https://github.com/WebAssembly/component-model/blob/main/design/high-level/Goals.md
- https://github.com/bytecodealliance/jco/blob/main/docs/src/transpiling.md

Implication:
- native runtime use of a Component Model host is plausible;
- PWA parity cannot be assumed from native Wasmtime-style capability semantics;
- V-010 must demonstrate that the browser/transpiled path preserves OM's no-ambient-authority guarantees, resource limits, crash isolation and plaintext-read/network-egress separation.

## V-012 — expressive-media/content-pack parser safety

No architecture contradiction was found in this pass. The remaining work is intentionally empirical: bounded decoder budgets, malformed WebP/media/manifest corpora, signature/update/fork-lineage ambiguity tests and runtime resource measurements. This should begin only after the exact parser/decoder libraries used by the reference runtime are selected for the verification prototype.

## Conclusion

None of V-001/V-006/V-007/V-010/V-012 can honestly be marked passed from standards availability alone. The research narrows the experiments and confirms that the selected directions remain plausible enough to continue verification without reopening the design queue.
