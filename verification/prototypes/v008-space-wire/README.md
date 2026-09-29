# V-008 W3 Space/Publication wire-freeze prototype

This is repo-local verification evidence for ADR-0160, not product code.

- [`spec/cddl/space-publication.cddl`](../../../spec/cddl/space-publication.cddl) is the canonical W3 CDDL.
- `vectors.json` contains positive and negative semantic vectors.
- [`python_codec.py`](python_codec.py) and [`node_codec.mjs`](node_codec.mjs) are independently written minimal canonical-CBOR encoders/validators.
- `run.py` requires exact verdict, canonical body bytes and canonical SignedEnvelope bytes to agree between both codecs.

The prototype intentionally does not implement signatures or cryptographic verification. It verifies the wire shape,
field typing, canonical CBOR construction, signed-envelope framing, sequence rules and typed ConversationEvent bodies.
Independent implementation/review remains a later gate requirement.
