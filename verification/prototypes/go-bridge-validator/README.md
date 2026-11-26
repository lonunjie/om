# Go bridge wire validator (second implementation)

A schema- and semantics-aware validator for the W4 bridge objects, written in Go with the standard library only. It sits on top of the strict OM-CBOR-v1 reader in `../go-cbor-reader` (`reader.go` is a byte-identical copy) and is independent of the Python implementation in `../bridges-wire`. It was built by an outside model from the registries, [`bridge.cddl`](../../../spec/cddl/bridge.cddl), [`common.cddl`](../../../spec/cddl/common.cddl) and the bridge README, then reviewed and completed here.

## Result

```sh
go vet ./... && go test ./...
go run . ../bridges-wire/vectors.json
```

Replays the frozen bridge corpus from `wire_hex` alone plus the permitted context fixtures: 27/27 positive (accepted and re-encoded byte for byte) and 98/98 negative (exact error class). Neither the case id nor the expected class reaches `Validate`.

One negative case initially disagreed (`bad-mapping-export-without-metadata`): the builder had no access to ADR-0264 item 4, which makes metadata preservation mandatory for outbound mappings. The rule was added as a general predicate (outbound or bidirectional mapping with `require_interop_metadata` false is `wrong_role`) and has its own test.

## What it does not cover

Not signature validity, eligible controllers or quorum, MLS state, fingerprint MAC verification, adapter proofs, durable dedup or any real external leg. The reader's 8,192-byte limit is below some registered object ceilings, so large valid objects are rejected before shape checks. Error precedence for objects with several simultaneous faults is not specified; extension entries are visited in map order. Later-wave event bodies not bound in the registry are conservatively rejected.

This is one more codec agreeing on the corpus, not closure of V-013: independent review and real adapter and multi-bridge evidence remain open.

The three registry copies are embedded; a test fails if they drift from `spec/registries/`.
