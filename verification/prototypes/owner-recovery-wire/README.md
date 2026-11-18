# Owner recovery wire fixtures

ADR-0203 adds an owner-held checkpoint and a distinct controller approval
exchange. Run `./run.sh` with the repository toolchain and the local V-005
CDDL tools installed. `./run.sh --update-vectors` regenerates the compressed
fixture. All keys and account data are synthetic.

The Python generator uses the canonical Python CBOR encoder. The V-005
Rust and Node strict codecs independently check wire acceptance, rejection
and exact bytes. The Rust and Ruby CDDL tools check the same object shapes.
A separate Node check recomputes the version 2 sheet check and encodings,
purpose-bound HKDF, comparison code, approval binding and evidence digest.

Crossed-purpose, expiry, staleness, incomplete transfer and target-substitution
cases provide inputs for the Core implementation. The account transition and
ceremony lifecycle require product tests with real signatures, durable Core
state and an independent verifier.
