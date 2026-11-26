package main

import (
	"bytes"
	"os"
	"testing"
)

// Builders use registry names rather than case IDs or corpus values.
func obj(s string, pairs ...any) Map {
	m := Map{}

	if nr.Structs[s].TopLevel {
		m[key(s, "object_type")] = objects[s].ID
		m[key(s, "object_version")] = uint64(1)
	}

	for i := 0; i < len(pairs); i += 2 {
		m[key(s, pairs[i].(string))] = pairs[i+1]
	}

	return m
}

func blob(n int, b byte) []byte {
	return bytes.Repeat([]byte{b}, n)
}

func check(
	t *testing.T,
	s string,
	v Value,
	ctx Context,
	want string,
) {
	t.Helper()

	wire := Encode(v)
	if wire == nil {
		t.Fatal("test encoding failed")
	}

	got, e := Validate(s, wire, ctx)

	if e != want {
		t.Fatalf("%s: got %q, want %q", s, e, want)
	}

	if e == "" && !bytes.Equal(Encode(got), wire) {
		t.Fatal("round trip failed")
	}
}

func TestPrincipal(t *testing.T) {
	const s = "BridgePrincipal"

	p := obj(s, "bridge_id", blob(32, 1))
	check(t, s, p, Context{}, "")

	p[key(s, "bridge_id")] = nil
	check(t, s, p, Context{}, "null_forbidden")

	delete(p, key(s, "bridge_id"))
	check(t, s, p, Context{}, "missing_field")

	p[key(s, "bridge_id")] = blob(31, 1)
	check(t, s, p, Context{}, "value_range")

	p[key(s, "bridge_id")] = blob(32, 1)
	p[uint64(99)] = true

	check(t, s, p, Context{}, "unknown_field")
	delete(p, uint64(99))

	p[key(s, "extensions")] = Map{
		"om.critical.test": Map{
			uint64(0): false,
			uint64(1): true,
		},
	}

	check(t, s, p, Context{}, "unknown_field")

	p[key(s, "extensions")] = Map{
		"example.optional": Map{
			uint64(0): false,
			uint64(1): true,
		},
	}

	check(t, s, p, Context{}, "")

	if _, e := Validate(
		s, append(Encode(p), 0), Context{},
	); e != "non_canonical" {
		t.Fatal(e)
	}

	if _, e := Validate(
		s, make([]byte, objects[s].Max+1), Context{},
	); e != "max_bytes" {
		t.Fatal(e)
	}
}

func TestTrace(t *testing.T) {
	const s = "InteropTraceBody"

	a, b := blob(32, 2), blob(32, 1)

	tr := obj(
		s,
		"origin_network_namespace", "test",
		"origin_fingerprint", blob(32, 9),
		"traversed_bridge_ids", []Value{a, b},
		"hop_count", uint64(2),
		"hop_limit", uint64(3),
		"transformation_ids", []Value{},
	)

	wrapper := obj("InteropTrace", "trace", tr)

	// Traversal order is not a sorted set.
	check(t, "InteropTrace", wrapper, Context{}, "")

	tr[key(s, "traversed_bridge_ids")] = []Value{a, a}
	check(t, "InteropTrace", wrapper, Context{}, "loop_detected")

	tr[key(s, "traversed_bridge_ids")] = []Value{a, b}
	tr[key(s, "hop_count")] = uint64(1)

	check(t, "InteropTrace", wrapper, Context{}, "value_range")
}

func TestProvenance(t *testing.T) {
	const s = "ForeignProvenanceBody"

	p := obj(
		s,
		"foreign_network_namespace", "test",
		"foreign_conversation_ref", "room",
		"foreign_event_ref", "event",
		"foreign_actor_ref", "actor",
		"bridge_observed_at", uint64(1),
		"fidelity_flags", []Value{
			enumID("bridge_fidelity_flag", "exact"),
		},
	)

	wrapper := obj("ForeignProvenance", "provenance", p)
	check(t, "ForeignProvenance", wrapper, Context{}, "")

	p[key(s, "fidelity_flags")] = []Value{uint64(2), uint64(1)}
	check(t, "ForeignProvenance", wrapper, Context{}, "set_order")

	p[key(s, "fidelity_flags")] = []Value{uint64(1), uint64(2)}
	check(t, "ForeignProvenance", wrapper, Context{}, "value_range")
}

func TestSignedChain(t *testing.T) {
	const s = "ForeignIdentityLink"

	d := obj(
		"Digest",
		"alg", enumID("hash_alg", "sha-256"),
		"value", blob(32, 5),
	)

	p := obj(
		s,
		"account_id", blob(32, 1),
		"foreign_network_namespace", "test",
		"foreign_actor_ref", "actor",
		"proof_method", enumID(
			"foreign_identity_proof_method", "signed_challenge",
		),
		"bridge_or_network_attestation_ref", d,
		"account_control_state_head", obj(
			"AccountControlBasis",
			"state_version", uint64(1),
			"state_hash", d,
		),
		"expires_at", uint64(100),
		"revocation_state", enumID("bridge_revocation_state", "active"),
		"link_version", uint64(1),
	)

	sig := obj(
		"Signature",
		"signer", blob(16, 3),
		"crypto_profile", uint64(1),
		"role", enumID("sig_role", "controller"),
		"signature", blob(64, 4),
	)

	envelope := func(b Value) Map {
		return obj(
			"SignedEnvelope",
			"body", Encode(b),
			"signatures", []Value{sig},
		)
	}

	check(t, s, envelope(p), Context{}, "")

	now := uint64(100)
	check(t, s, envelope(p), Context{Now: &now}, "stale_chain")

	q := Map{}
	for k, v := range p {
		q[k] = v
	}

	q[key(s, "link_version")] = uint64(2)
	q[key(s, "previous_link_hash")] = digest(s, p)

	ctx := Context{PriorValue: p}

	check(t, s, envelope(q), ctx, "")

	q[key(s, "previous_link_hash")] = d
	check(t, s, envelope(q), ctx, "forked_chain")

	sig[key("Signature", "role")] = enumID("sig_role", "device")
	check(t, s, envelope(p), Context{}, "wrong_role")
}

func TestW0CriticalPresence(t *testing.T) {
	e := obj("ConversationEvent", "foreign_provenance", nil)

	check(
		t,
		"ConversationEvent",
		e,
		Context{ReceiverWave: "W0"},
		"w0_receiver_rejection",
	)
}

// The embedded registry copies must not drift from the frozen registries.
func TestRegistryCopiesMatchSpec(t *testing.T) {
	for name, want := range map[string][]byte{
		"om-numeric-fields.json":    numericJSON,
		"om-object-types.json":      objectsJSON,
		"om-domain-separation.json": domainsJSON,
	} {
		got, err := os.ReadFile("../../../spec/registries/" + name)
		if err != nil {
			t.Skip("spec/registries not reachable from here")
		}
		if !bytes.Equal(got, want) {
			t.Fatalf("%s differs from spec/registries", name)
		}
	}
}

func TestOutboundMappingRequiresMetadata(t *testing.T) {
	const s = "BridgeMapping"
	for _, dir := range []string{"om_to_foreign", "bidirectional"} {
		m := mappingFixture(dir, false)
		check(t, s, m, Context{}, "wrong_role")
		m = mappingFixture(dir, true)
		check(t, s, m, Context{}, "")
	}
	// Inbound-only may leave it false.
	check(t, s, mappingFixture("foreign_to_om", false), Context{}, "")
}

func mappingFixture(direction string, requireMetadata bool) Map {
	const s = "BridgeMapping"
	d := obj(
		"Digest",
		"alg", enumID("hash_alg", "sha-256"),
		"value", blob(32, 7),
	)
	body := obj(
		s,
		"mapping_id", blob(16, 1),
		"bridge_id", blob(32, 2),
		"om_scope_ref", obj(
			"BridgeScopeRef",
			"scope_kind", uint64(2),
			"room_conversation_id", blob(32, 3),
		),
		"foreign_network_namespace", "matrix",
		"foreign_scope_ref", "!room:server",
		"direction", enumID("bridge_direction", direction),
		"capability_map", obj(
			"BridgeCapabilityMap",
			"allowed_event_classes", []Value{uint64(1)},
			"declared_semantics", []Value{uint64(1)},
		),
		"policy", obj(
			"BridgeMappingPolicy",
			"relay_bridged", false,
			"permit_fanout", false,
			"require_interop_metadata", requireMetadata,
		),
		"state_version", uint64(1),
		"approval_evidence", d,
		"revocation_state", enumID("bridge_revocation_state", "active"),
		"control_state_head", obj(
			"BridgeStateHead",
			"state_version", uint64(1),
			"state_hash", d,
		),
	)
	sig := obj(
		"Signature",
		"signer", blob(16, 3),
		"crypto_profile", uint64(1),
		"role", enumID("sig_role", "controller"),
		"signature", blob(64, 4),
	)
	return obj(
		"SignedEnvelope",
		"body", Encode(body),
		"signatures", []Value{sig},
	)
}
