// SPDX-License-Identifier: MPL-2.0
package main

import (
	"bytes"
	"encoding/hex"
	"errors"
	"math"
	"strings"
	"testing"
)

func unhex(t testing.TB, s string) []byte {
	t.Helper()
	b, e := hex.DecodeString(s)
	if e != nil {
		t.Fatal(e)
	}
	return b
}
func expectReason(t testing.TB, b []byte, want string) {
	t.Helper()
	_, err := DecodeStrict(b)
	var de *DecodeError
	if !errors.As(err, &de) || de.Reason != want {
		t.Fatalf("wanted %s, got %v", want, err)
	}
}
func TestCanonicalValues(t *testing.T) {
	cases := []struct {
		v Value
		h string
	}{
		{uint64(0), "00"}, {uint64(23), "17"}, {uint64(24), "1818"}, {uint64(255), "18ff"}, {uint64(256), "190100"}, {uint64(65535), "19ffff"}, {uint64(65536), "1a00010000"}, {uint64(math.MaxUint32), "1affffffff"}, {uint64(1 << 32), "1b0000000100000000"}, {uint64(math.MaxUint64), "1bffffffffffffffff"},
		{false, "f4"}, {true, "f5"}, {nil, "f6"}, {[]byte{}, "40"}, {"", "60"}, {[]Value{}, "80"}, {Map{}, "a0"}, {Map{"a": uint64(0), uint64(256): true, "": nil, "bb": false}, "a4190100f560f6616100626262f4"}, {[]byte{0xff, 0xc0}, "42ffc0"}, {"é", "6365cc81"}, {"é", "62c3a9"}, {[]Value{nil, true, uint64(7)}, "83f6f507"},
		{Map{uint64(256): false, uint64(24): nil, uint64(23): true, uint64(0): uint64(1)}, "a4000117f51818f6190100f4"},
		{Map{uint64(1): []byte{0xbf, 0xff}, uint64(2): []Value{Map{uint64(1): "x"}}}, "a20142bfff0281a1016178"},
	}
	for _, c := range cases {
		t.Run(c.h, func(t *testing.T) {
			b := unhex(t, c.h)
			if got := Encode(c.v); !bytes.Equal(got, b) {
				t.Fatalf("encode %x want %x", got, b)
			}
			v, e := DecodeStrict(b)
			if e != nil {
				t.Fatal(e)
			}
			if got := Encode(v); !bytes.Equal(got, b) {
				t.Fatalf("round trip %x want %x", got, b)
			}
		})
	}
}
func TestReject(t *testing.T) {
	cases := map[string]string{
		"": "truncated", "1800": "non_shortest_integer", "1900ff": "non_shortest_integer", "1a0000ffff": "non_shortest_integer", "1b00000000ffffffff": "non_shortest_integer",
		"5800": "non_shortest_length", "780161": "non_shortest_length", "9800": "non_shortest_length", "b800": "non_shortest_length",
		"5fff": "indefinite_length", "7fff": "indefinite_length", "9fff": "indefinite_length", "bfff": "indefinite_length", "ff": "indefinite_length", "1f": "indefinite_length",
		"20": "forbidden_major_type", "c000": "tags", "d8184100": "tags", "f90000": "floats", "fa00000000": "floats", "fb0000000000000000": "floats", "f7": "forbidden_simple_value", "f814": "forbidden_simple_value", "e0": "forbidden_simple_value", "fc": "forbidden_simple_value", "1c": "reserved_additional_info",
		"61ff": "invalid_utf8", "62c080": "invalid_utf8", "63eda080": "invalid_utf8", "64f4908080": "invalid_utf8", "0000": "trailing_bytes", "a200010002": "duplicate_map_key", "a201000000": "unsorted_map_keys", "a1180000": "non_shortest_integer", "a12000": "forbidden_map_key", "a2616100616101": "duplicate_map_key", "a26161000000": "unsorted_map_keys", "a178016100": "non_shortest_length", "a1f400": "forbidden_map_key", "81a200010002": "duplicate_map_key",
		"1b00": "truncated", "4201": "truncated", "a100": "truncated", "8200": "truncated", "5bffffffffffffffff": "oversize", "9bffffffffffffffff": "collection_count", "bbffffffffffffffff": "collection_count",
	}
	for h, want := range cases {
		t.Run(h, func(t *testing.T) { expectReason(t, unhex(t, h), want) })
	}
}
func TestLimits(t *testing.T) {
	b := append([]byte{0x59, 0x1f, 0xfd}, make([]byte, MaxBytes-3)...)
	if v, e := DecodeStrict(b); e != nil || !bytes.Equal(Encode(v), b) {
		t.Fatalf("exact size limit: %v", e)
	}
	expectReason(t, append(b, 0), "oversize")
	b = append(bytes.Repeat([]byte{0x81}, MaxDepth), 0xf6)
	if v, e := DecodeStrict(b); e != nil || !bytes.Equal(Encode(v), b) {
		t.Fatalf("exact depth limit: %v", e)
	}
	expectReason(t, append([]byte{0x81}, b...), "nesting_depth")
	b = append([]byte{0x99, 0x04, 0x00}, make([]byte, MaxCollection)...)
	if v, e := DecodeStrict(b); e != nil || !bytes.Equal(Encode(v), b) {
		t.Fatalf("exact count limit: %v", e)
	}
	expectReason(t, []byte{0x99, 0x04, 0x01}, "collection_count")
	expectReason(t, []byte{0xb9, 0x04, 0x01}, "collection_count")
	for _, v := range []Value{int(1), float64(1), "\xff", make([]byte, MaxBytes), make([]Value, MaxCollection+1)} {
		if Encode(v) != nil {
			t.Fatalf("accepted invalid encoder value %T", v)
		}
	}
	cycle := make([]Value, 1)
	cycle[0] = cycle
	if Encode(cycle) != nil {
		t.Fatal("accepted cyclic value")
	}
}
func TestByteStringOwnership(t *testing.T) {
	b := []byte{0x41, 0x01}
	v, e := DecodeStrict(b)
	if e != nil {
		t.Fatal(e)
	}
	b[1] = 2
	if !bytes.Equal(Encode(v), []byte{0x41, 1}) {
		t.Fatal("decoded bytes alias input")
	}
}
func TestCorpora(t *testing.T) {
	for _, tc := range []struct {
		file string
		want tally
	}{{"../bridges-wire/vectors.json", tally{27, 27, 10, 10, 88, 88}}, {"../plugins-wire/vectors.json", tally{24, 24, 14, 14, 102, 102}}, {"../services-wire/vectors.json", tally{21, 21, 5, 5, 75, 75}}} {
		t.Run(tc.file, func(t *testing.T) {
			c, e := readCorpus(tc.file)
			if e != nil {
				t.Fatal(e)
			}
			got, issues, bad := replay(c)
			if bad || len(issues) != 0 || got != tc.want {
				t.Fatalf("tally %+v want %+v, issues %v", got, tc.want, issues)
			}
			for _, v := range c.Negative {
				if layerNegative(v) {
					b, e := wire(v)
					if e != nil {
						t.Fatal(e)
					}
					_, e = DecodeStrict(b)
					var de *DecodeError
					if !errors.As(e, &de) || de.Reason == "" {
						t.Fatalf("%s: no named reason: %v", v.ID, e)
					}
					t.Logf("%s (%s): %v", v.ID, v.ErrorClass, e)
				}
			}
		})
	}
}
func TestReplayPolicy(t *testing.T) {
	s := func(h string) *string { return &h }
	c := corpus{Positive: []vector{{ID: "ok", Wire: s("00")}}, Negative: []vector{{ID: "semantic", Wire: s("ff"), ErrorClass: "missing_field"}}}
	_, issues, bad := replay(c)
	if bad || len(issues) != 1 || !strings.Contains(issues[0], "unexpected CBOR-layer rejection") {
		t.Fatalf("semantic policy: %v %v", bad, issues)
	}
	c.Negative = append(c.Negative, vector{ID: "accepted", Wire: s("00"), ErrorClass: "non_canonical"})
	_, _, bad = replay(c)
	if !bad {
		t.Fatal("accepted mandatory negative not fatal")
	}
	c = corpus{Positive: []vector{{ID: "bad", Wire: s("ff")}}}
	_, _, bad = replay(c)
	if !bad {
		t.Fatal("rejected positive not fatal")
	}
	c = corpus{Negative: []vector{{ID: "badhex", Wire: s("zz"), ErrorClass: "max_bytes"}}}
	got, _, bad := replay(c)
	if !bad || got.cbor != 0 {
		t.Fatal("bad hex counted as decoder rejection")
	}
}
func FuzzRoundTrip(f *testing.F) {
	for _, h := range []string{"00", "a10080", "f6", "ff", "1bffffffffffffffff", "a200010002", "62c3a9"} {
		f.Add(unhex(f, h))
	}
	f.Fuzz(func(t *testing.T, b []byte) {
		v, e := DecodeStrict(b)
		if e == nil && !bytes.Equal(Encode(v), b) {
			t.Fatalf("accepted non-roundtripping bytes %x", b)
		}
	})
}
