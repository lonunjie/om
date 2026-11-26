package main

import (
	"bytes"
	"crypto/sha256"
	_ "embed"
	"encoding/json"
	"strconv"
	"strings"
)

// Context contains independently authenticated diagnostic bodies, not envelopes.
// Nil means absent. Signatures is an additional structural fixture, not a
// substitute for signatures actually carried by the wire.
type Context struct {
	PriorValue, ControlValue, MappingValue Value
	ReceiverWave                           string
	Now                                    *uint64
	Signatures                             []Value
}

//go:embed om-numeric-fields.json
var numericJSON []byte

//go:embed om-object-types.json
var objectsJSON []byte

//go:embed om-domain-separation.json
var domainsJSON []byte

type spec map[string]any

type field struct {
	Key  uint64
	Name string
	Type spec
	Req  bool
	Wave string
}

type union struct {
	On       uint64
	Variants map[string]string
}

type structure struct {
	TopLevel bool `json:"top_level"`
	Fields   []field
	Union    *union
}

type objectSpec struct {
	ID               uint64 `json:"type_id"`
	Name, Wave, Hash string
	Max              int `json:"max_bytes"`
	Signed           *struct {
		Label string
		Roles []string
	}
}

var nr struct {
	Common     []field `json:"common_top_level_fields"`
	Structs    map[string]structure
	Enums      map[string]map[string]string
	Waves      map[string]map[string]string `json:"enum_value_waves"`
	Extensions []struct {
		Name, Object, Wave string
		Critical           bool
		Value              spec
	}
}

var objects = map[string]objectSpec{}
var domains = map[string]string{}

func init() {
	var o struct{ Types []objectSpec }
	var d struct{ Labels []struct{ Label, Use string } }

	for _, p := range []struct {
		b []byte
		v any
	}{{numericJSON, &nr}, {objectsJSON, &o}, {domainsJSON, &d}} {
		if err := json.Unmarshal(p.b, p.v); err != nil {
			panic(err)
		}
	}

	for _, l := range d.Labels {
		if _, ok := domains[l.Label]; ok {
			panic("duplicate domain")
		}
		domains[l.Label] = l.Use
	}

	for _, t := range o.Types {
		if t.Hash != "" && domains[t.Hash] != "hash" {
			panic("missing hash domain")
		}
		if t.Signed != nil && domains[t.Signed.Label] != "signature" {
			panic("missing signature domain")
		}
		objects[t.Name] = t
	}
}

func fields(s string) []field {
	t := nr.Structs[s]
	if !t.TopLevel {
		return t.Fields
	}
	return append(append([]field{}, nr.Common...), t.Fields...)
}

func key(s, n string) uint64 {
	for _, f := range fields(s) {
		if f.Name == n {
			return f.Key
		}
	}
	panic(s + "." + n)
}

func get(v Value, s, n string) Value {
	m, _ := v.(Map)
	return m[key(s, n)]
}

func present(v Value, s, n string) bool {
	m, _ := v.(Map)
	_, ok := m[key(s, n)]
	return ok
}

func u(v Value) uint64 {
	x, _ := v.(uint64)
	return x
}

func arr(v Value) []Value {
	x, _ := v.([]Value)
	return x
}

func yes(v Value) bool {
	x, _ := v.(bool)
	return x
}

func dec(v Value) string {
	return strconv.FormatUint(u(v), 10)
}

func enum(e string, v Value) string {
	return nr.Enums[e][dec(v)]
}

func enumID(e, n string) uint64 {
	for k, v := range nr.Enums[e] {
		if v == n {
			x, _ := strconv.ParseUint(k, 10, 64)
			return x
		}
	}
	panic(e + "." + n)
}

func equal(a, b Value) bool {
	return bytes.Equal(Encode(a), Encode(b))
}

func has(a []Value, v Value) bool {
	for _, x := range a {
		if equal(x, v) {
			return true
		}
	}
	return false
}

func unique(a []Value, project func(Value) Value) bool {
	seen := map[string]bool{}
	for _, x := range a {
		k := string(Encode(project(x)))
		if seen[k] {
			return false
		}
		seen[k] = true
	}
	return true
}

func identity(v Value) Value {
	return v
}

func by(s, n string) func(Value) Value {
	return func(v Value) Value {
		return get(v, s, n)
	}
}

func find(a []Value, s, n string, v Value) Value {
	for _, x := range a {
		if equal(get(x, s, n), v) {
			return x
		}
	}
	return nil
}

func waveOK(required, actual string) bool {
	return required == "" || required <= actual
}

func bounds(n uint64, v any) bool {
	a := v.([]any)
	return n >= uint64(a[0].(float64)) &&
		(a[1] == nil || n <= uint64(a[1].(float64)))
}

type checker struct {
	object, wave string
}

func (c checker) shape(s string, v Value) string {
	if v == nil {
		return "null_forbidden"
	}

	m, ok := v.(Map)
	if !ok {
		return "value_range"
	}

	t, ok := nr.Structs[s]
	if !ok {
		return "value_range"
	}

	if t.Union != nil {
		x, ok := m[t.Union.On]
		if !ok {
			return "missing_field"
		}
		if x == nil {
			return "null_forbidden"
		}
		if _, ok := x.(uint64); !ok {
			return "value_range"
		}
		return c.shape(t.Union.Variants[dec(x)], m)
	}

	fs := fields(s)
	allowed := map[uint64]bool{}

	for _, f := range fs {
		allowed[f.Key] = true
	}

	for k := range m {
		n, ok := k.(uint64)
		if !ok || !allowed[n] {
			return "unknown_field"
		}
	}

	for _, f := range fs {
		x, ok := m[f.Key]

		if !ok {
			if f.Req {
				return "missing_field"
			}
			continue
		}

		if !waveOK(f.Wave, c.wave) {
			return "unknown_field"
		}

		if e := c.typ(f.Type, x, m, s); e != "" {
			return e
		}
	}

	if t.TopLevel && u(get(m, s, "object_type")) != objects[s].ID {
		return "value_range"
	}

	return ""
}

func (c checker) typ(t spec, v Value, parent Map, s string) string {
	if v == nil {
		return "null_forbidden"
	}

	switch {
	case hasType(t, "uint"):
		n, ok := v.(uint64)
		if ok && bounds(n, t["uint"]) {
			return ""
		}

	case hasType(t, "bstr"):
		b, ok := v.([]byte)
		if ok && bounds(uint64(len(b)), t["bstr"]) {
			return ""
		}

	case hasType(t, "tstr"):
		b, ok := v.(string)
		if ok && bounds(uint64(len(b)), t["tstr"]) {
			return ""
		}

	case hasType(t, "bool"):
		b, ok := v.(bool)
		if ok && (t["bool"] == nil || t["bool"] == b) {
			return ""
		}

	case hasType(t, "enum"):
		e := t["enum"].(string)
		if _, ok := v.(uint64); ok &&
			enum(e, v) != "" &&
			waveOK(nr.Waves[e][dec(v)], c.wave) {
			return ""
		}

	case hasType(t, "ref"):
		return c.shape(t["ref"].(string), v)

	case hasType(t, "array") || hasType(t, "set"):
		a, ok := v.([]Value)
		if !ok || !bounds(uint64(len(a)), t["n"]) {
			return "value_range"
		}

		k := "array"
		if hasType(t, "set") {
			k = "set"
		}

		var prev []byte

		for _, x := range a {
			if e := c.typ(t[k].(map[string]any), x, nil, s); e != "" {
				return e
			}

			b := Encode(x)

			if k == "set" && prev != nil &&
				bytes.Compare(prev, b) >= 0 {
				return "set_order"
			}

			prev = b
		}

		return ""

	case hasType(t, "ext"):
		return c.extensions(v)

	case hasType(t, "any"):
		return ""

	case hasType(t, "dispatch"):
		d := t["dispatch"].(map[string]any)
		sel := get(parent, s, d["on"].(string))
		table := d["table"].(map[string]any)

		if name, ok := table[dec(sel)].(string); ok {
			return c.shape(name, v)
		}

		// ConversationEvent's additional W0 kinds have matching named structs.
		// Later-wave kinds need an explicit reviewed binding, not opaque acceptance.
		if s == "ConversationEvent" {
			if nr.Waves["event_kind"][dec(sel)] != "" {
				return "value_range"
			}

			name := ""
			for _, p := range strings.Split(enum("event_kind", sel), ".") {
				if p != "" {
					name += strings.ToUpper(p[:1]) + p[1:]
				}
			}

			return c.shape(name, v)
		}

	case hasType(t, "signed"):
		if b, ok := v.([]byte); ok {
			_, e := Validate(t["signed"].(string), b, Context{})
			return e
		}
	}

	return "value_range"
}

func hasType(t spec, k string) bool {
	_, ok := t[k]
	return ok
}

func (c checker) extensions(v Value) string {
	m, ok := v.(Map)
	if !ok || len(m) == 0 {
		return "value_range"
	}

	for k, x := range m {
		id, ok := k.(string)
		if !ok || len(id) < 1 || len(id) > 128 {
			return "value_range"
		}

		if x == nil {
			return "null_forbidden"
		}

		entry, ok := x.(Map)
		if !ok {
			return "value_range"
		}

		// ExtEntry is defined in common.cddl, not as a registry struct.
		for k := range entry {
			if k != uint64(0) && k != uint64(1) {
				return "unknown_field"
			}
		}

		critical, ok := entry[uint64(0)]
		if !ok {
			return "missing_field"
		}

		payload, ok := entry[uint64(1)]
		if !ok {
			return "missing_field"
		}

		if critical == nil || payload == nil {
			return "null_forbidden"
		}

		flag, ok := critical.(bool)
		if !ok {
			return "value_range"
		}

		known := false

		for _, r := range nr.Extensions {
			if r.Name == id && r.Object == c.object &&
				waveOK(r.Wave, c.wave) {
				known = true

				if flag != r.Critical {
					return "value_range"
				}

				if e := c.typ(r.Value, payload, nil, c.object); e != "" {
					return e
				}

				break
			}
		}

		if !known && (flag || strings.HasPrefix(id, "om.critical.")) {
			return "unknown_field"
		}
	}

	return ""
}

func decode(b []byte) (Value, string) {
	v, e := DecodeStrict(b)

	if e != nil {
		if d, ok := e.(*DecodeError); ok && d.Reason == "oversize" {
			return nil, "max_bytes"
		}
		return nil, "non_canonical"
	}

	return v, ""
}

func digest(s string, v Value) Value {
	body := Encode(v)
	if body == nil {
		return nil
	}

	p := Encode([]Value{objects[s].Hash, body})
	if p == nil {
		return nil
	}

	h := sha256.Sum256(p)

	return Map{
		key("Digest", "alg"):   enumID("hash_alg", "sha-256"),
		key("Digest", "value"): h[:],
	}
}

func (c checker) signatures(a []Value, t objectSpec, threshold uint64) string {
	for _, f := range fields("SignedEnvelope") {
		if f.Name == "signatures" {
			if e := c.typ(f.Type, a, nil, "SignedEnvelope"); e != "" {
				return e
			}
		}
	}

	for _, s := range a {
		role := enum("sig_role", get(s, "Signature", "role"))
		ok := false

		for _, r := range t.Signed.Roles {
			if r == role {
				ok = true
			}
		}

		if !ok {
			return "wrong_role"
		}
	}

	if !unique(a, by("Signature", "signer")) ||
		uint64(len(a)) < threshold {
		return "wrong_role"
	}

	return ""
}

// Validate returns the outer decoded item (including an envelope if present).
func Validate(object string, wire []byte, ctx Context) (Value, string) {
	t, ok := objects[object]
	if !ok || !nr.Structs[object].TopLevel {
		return nil, "value_range"
	}

	if len(wire) > t.Max {
		return nil, "max_bytes"
	}

	v, e := decode(wire)
	if e != "" {
		return v, e
	}

	body := v

	c := checker{object, "W4"}
	if ctx.ReceiverWave != "" {
		c.wave = ctx.ReceiverWave
	}

	if object == "ConversationEvent" && c.wave == "W0" &&
		(present(v, object, "foreign_provenance") ||
			present(v, object, "representation")) {
		return v, "w0_receiver_rejection"
	}

	var sigs []Value

	if t.Signed != nil {
		if e = c.shape("SignedEnvelope", v); e != "" {
			return v, e
		}

		body, e = decode(get(v, "SignedEnvelope", "body").([]byte))
		if e != "" {
			return v, e
		}

		sigs = arr(get(v, "SignedEnvelope", "signatures"))
	}

	if e = c.shape(object, body); e != "" {
		return v, e
	}

	for _, f := range []struct {
		s string
		v Value
	}{
		{object, ctx.PriorValue},
		{"BridgeControlState", ctx.ControlValue},
		{"BridgeMapping", ctx.MappingValue},
	} {
		if f.v != nil {
			if e = (checker{f.s, "W4"}).shape(f.s, f.v); e != "" {
				return v, e
			}
		}
	}

	if object == "BridgeControlState" &&
		u(get(body, object, "approval_threshold")) >
			uint64(len(arr(get(body, object, "controllers")))) {
		return v, "value_range"
	}

	if t.Signed != nil {
		threshold := uint64(1)

		if object == "BridgeControlState" {
			b := body
			if ctx.PriorValue != nil {
				b = ctx.PriorValue
			}
			threshold = u(get(b, object, "approval_threshold"))
		}

		if e = c.signatures(sigs, t, threshold); e != "" {
			return v, e
		}

		if ctx.Signatures != nil {
			if e = c.signatures(ctx.Signatures, t, threshold); e != "" {
				return v, e
			}
		}
	}

	return v, semantic(object, body, ctx)
}

func chain(
	s string,
	b, p Value,
	version, previous, state string,
	ids ...string,
) string {
	g := func(v Value, n string) Value {
		return get(v, s, n)
	}

	n := u(g(b, version))
	prev := g(b, previous)

	if n == 1 && prev != nil {
		return "forked_chain"
	}

	if n > 1 && prev == nil {
		return "missing_field"
	}

	if p == nil {
		return ""
	}

	for _, id := range ids {
		if !equal(g(b, id), g(p, id)) {
			return "forked_chain"
		}
	}

	old := u(g(p, version))

	if n == old && !equal(b, p) {
		return "forked_chain"
	}

	if n != old+1 {
		return "stale_chain"
	}

	if !equal(prev, digest(s, p)) {
		return "forked_chain"
	}

	if state != "" &&
		enum("bridge_revocation_state", g(p, state)) == "revoked" &&
		enum("bridge_revocation_state", g(b, state)) != "revoked" {
		return "stale_chain"
	}

	return ""
}

func trace(v Value) string {
	const s = "InteropTraceBody"

	g := func(n string) Value {
		return get(v, s, n)
	}

	path := arr(g("traversed_bridge_ids"))
	n := u(g("hop_count"))

	if n != uint64(len(path)) || n > u(g("hop_limit")) {
		return "value_range"
	}

	if !unique(path, identity) {
		return "loop_detected"
	}

	if !unique(arr(g("transformation_ids")), identity) {
		return "value_range"
	}

	return ""
}

func provenance(v Value) string {
	const s = "ForeignProvenanceBody"

	g := func(n string) Value {
		return get(v, s, n)
	}

	flags := arr(g("fidelity_flags"))

	flag := func(n string) bool {
		return has(flags, enumID("bridge_fidelity_flag", n))
	}

	if flag("exact") && len(flags) != 1 {
		return "value_range"
	}

	if g("foreign_actor_ref") == nil && !flag("foreign_actor_unverified") {
		return "missing_field"
	}

	if t := g("interop_trace"); t != nil {
		if e := trace(t); e != "" {
			return e
		}

		if g("origin_fingerprint") == nil {
			return "missing_field"
		}

		if !equal(
			g("origin_fingerprint"),
			get(t, "InteropTraceBody", "origin_fingerprint"),
		) {
			return "provenance_mismatch"
		}
	}

	return ""
}

func semantic(s string, b Value, ctx Context) string {
	g := func(n string) Value {
		return get(b, s, n)
	}

	switch s {
	case "BridgePrincipal":

	case "BridgeControlState":
		if e := chain(
			s, b, ctx.PriorValue,
			"state_version", "previous_state_hash", "",
			"bridge_id",
		); e != "" {
			return e
		}

		endpoints := arr(g("endpoint_credentials"))
		adapters := arr(g("adapter_definitions"))

		const ep = "BridgeEndpointCredential"
		const ad = "BridgeAdapterDefinition"
		const cap = "InteropCapabilities"

		if !unique(endpoints, by(ep, "endpoint_id")) ||
			!unique(adapters, by(ad, "adapter_id")) ||
			!unique(adapters, by(ad, "foreign_network_namespace")) {
			return "set_order"
		}

		for _, a := range adapters {
			c := get(a, ad, "capabilities")
			limits := arr(get(c, cap, "per_semantic_size_limits"))

			if !unique(limits, by("BridgeSizeLimit", "semantic")) {
				return "set_order"
			}

			for _, l := range limits {
				if !has(
					arr(get(c, cap, "supported_semantics")),
					get(l, "BridgeSizeLimit", "semantic"),
				) {
					return "value_range"
				}
			}
		}

		if ctx.PriorValue != nil {
			for _, old := range arr(get(
				ctx.PriorValue, s, "endpoint_credentials",
			)) {
				new := find(
					endpoints, ep, "endpoint_id",
					get(old, ep, "endpoint_id"),
				)

				if enum(
					"bridge_endpoint_state",
					get(old, ep, "credential_state"),
				) == "revoked" &&
					(new == nil || enum(
						"bridge_endpoint_state",
						get(new, ep, "credential_state"),
					) != "revoked") {
					return "stale_chain"
				}

				if new != nil &&
					(!equal(
						get(old, ep, "public_key"),
						get(new, ep, "public_key"),
					) || !equal(
						get(old, ep, "crypto_profile"),
						get(new, ep, "crypto_profile"),
					)) {
					return "stale_chain"
				}
			}
		}

	case "BridgeMapping":
		if e := chain(
			s, b, ctx.PriorValue,
			"state_version", "previous_mapping_hash", "revocation_state",
			"mapping_id", "bridge_id", "om_scope_ref",
			"foreign_network_namespace", "foreign_scope_ref",
		); e != "" {
			return e
		}

		return mappingControl(b, ctx.ControlValue)

	case "InteropTrace":
		return trace(g("trace"))

	case "ForeignProvenance":
		return provenance(g("provenance"))

	case "ForeignIdentityLink":
		if e := chain(
			s, b, ctx.PriorValue,
			"link_version", "previous_link_hash", "revocation_state",
			"account_id", "foreign_network_namespace", "foreign_actor_ref",
		); e != "" {
			return e
		}

		bridge := enum(
			"foreign_identity_proof_method", g("proof_method"),
		) == "bridge_attestation"

		if bridge && g("attesting_bridge_id") == nil {
			return "missing_field"
		}

		if !bridge && g("attesting_bridge_id") != nil {
			return "value_range"
		}

		if g("not_before") != nil &&
			u(g("not_before")) >= u(g("expires_at")) {
			return "value_range"
		}

		if ctx.Now != nil && *ctx.Now >= u(g("expires_at")) {
			return "stale_chain"
		}

	case "ConversationEvent":
		return event(b, ctx)

	default:
		return "value_range"
	}

	return ""
}

func mappingControl(b, control Value) string {
	const s = "BridgeMapping"

	g := func(n string) Value {
		return get(b, s, n)
	}

	policy := g("policy")
	out := enum("bridge_direction", g("direction")) != "foreign_to_om"

	if yes(get(policy, "BridgeMappingPolicy", "relay_bridged")) &&
		!yes(get(policy, "BridgeMappingPolicy", "require_interop_metadata")) {
		return "wrong_role"
	}

	// ADR-0264 item 4: for an outbound mapping, requiring interop metadata
	// is mandatory, independent of relay and of any control fixture.
	if out && !yes(get(policy, "BridgeMappingPolicy", "require_interop_metadata")) {
		return "wrong_role"
	}

	if control == nil {
		return ""
	}

	const cs = "BridgeControlState"

	head := g("control_state_head")

	if !equal(g("bridge_id"), get(control, cs, "bridge_id")) ||
		!equal(
			get(head, "BridgeStateHead", "state_version"),
			get(control, cs, "state_version"),
		) ||
		!equal(
			get(head, "BridgeStateHead", "state_hash"),
			digest(cs, control),
		) {
		return "wrong_role"
	}

	a := find(
		arr(get(control, cs, "adapter_definitions")),
		"BridgeAdapterDefinition",
		"foreign_network_namespace",
		g("foreign_network_namespace"),
	)

	if a == nil {
		return "wrong_role"
	}

	caps := get(a, "BridgeAdapterDefinition", "capabilities")

	for _, v := range arr(get(
		g("capability_map"), "BridgeCapabilityMap", "declared_semantics",
	)) {
		if !has(
			arr(get(caps, "InteropCapabilities", "supported_semantics")),
			v,
		) {
			return "wrong_role"
		}
	}

	if (out || yes(get(
		policy, "BridgeMappingPolicy", "require_interop_metadata",
	))) && !yes(get(
		caps, "InteropCapabilities", "carries_interop_metadata",
	)) {
		return "wrong_role"
	}

	return ""
}

func event(b Value, ctx Context) string {
	const s = "ConversationEvent"

	g := func(n string) Value {
		return get(b, s, n)
	}

	author := g("author_principal")
	id := get(author, "PrincipalRef", "id")
	kind := enum("principal_kind", get(author, "PrincipalRef", "kind"))
	p := g("foreign_provenance")

	if enum("event_kind", g("kind")) == "message.create" {
		body := g("body")

		if get(body, "MessageCreate", "content") == nil &&
			len(arr(get(body, "MessageCreate", "attachments"))) == 0 {
			return "value_range"
		}
	}

	if p == nil {
		if kind == "bridge" {
			return "missing_field"
		}

		if g("representation") != nil && kind != "service" {
			return "wrong_role"
		}

		return ""
	}

	if kind != "bridge" || g("representation") != nil {
		return "wrong_role"
	}

	for _, f := range fields("BridgePrincipal") {
		if f.Name == "bridge_id" {
			if e := (checker{s, "W4"}).typ(f.Type, id, nil, s); e != "" {
				return e
			}
		}
	}

	if e := provenance(p); e != "" {
		return e
	}

	t := get(p, "ForeignProvenanceBody", "interop_trace")

	if t == nil ||
		get(p, "ForeignProvenanceBody", "origin_fingerprint") == nil {
		return "missing_field"
	}

	path := arr(get(t, "InteropTraceBody", "traversed_bridge_ids"))

	if len(path) == 0 || !equal(path[len(path)-1], id) {
		return "provenance_mismatch"
	}

	if u(get(t, "InteropTraceBody", "hop_count")) >=
		u(get(t, "InteropTraceBody", "hop_limit")) {
		return "loop_detected"
	}

	if c := ctx.ControlValue; c != nil {
		if !equal(id, get(c, "BridgeControlState", "bridge_id")) {
			return "wrong_role"
		}

		ep := find(
			arr(get(c, "BridgeControlState", "endpoint_credentials")),
			"BridgeEndpointCredential",
			"endpoint_id",
			g("author_endpoint_id"),
		)

		if ep == nil || enum(
			"bridge_endpoint_state",
			get(ep, "BridgeEndpointCredential", "credential_state"),
		) != "active" {
			return "wrong_role"
		}
	}

	if m := ctx.MappingValue; m != nil {
		const ms = "BridgeMapping"

		if !equal(id, get(m, ms, "bridge_id")) ||
			!equal(
				g("conversation_id"),
				get(
					get(m, ms, "om_scope_ref"),
					"BridgeScopeRef",
					"room_conversation_id",
				),
			) ||
			enum("bridge_direction", get(m, ms, "direction")) == "om_to_foreign" ||
			enum("bridge_revocation_state", get(m, ms, "revocation_state")) != "active" {
			return "wrong_role"
		}

		if !equal(
			get(p, "ForeignProvenanceBody", "foreign_network_namespace"),
			get(m, ms, "foreign_network_namespace"),
		) ||
			!equal(
				get(p, "ForeignProvenanceBody", "foreign_conversation_ref"),
				get(m, ms, "foreign_scope_ref"),
			) {
			return "provenance_mismatch"
		}

		if e := mappingControl(m, ctx.ControlValue); e != "" {
			return e
		}
	}

	return ""
}
