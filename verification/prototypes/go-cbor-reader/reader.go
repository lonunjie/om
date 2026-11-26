// SPDX-License-Identifier: MPL-2.0
package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"sort"
	"unicode/utf8"
)

// Value is one of uint64, []byte, string, []Value, Map,
// bool, or nil (CBOR null). Byte strings are opaque, not nested CBOR.
type Value any

// Map keys are uint64 or string. Other key types are outside this reader profile.
type Map map[Value]Value

// Conservative implementation limits, not normative OM per-object limits.
const (
	MaxBytes      = 8192
	MaxDepth      = 32   // number of nested array/map containers, root container = 1
	MaxCollection = 1024 // array elements or map pairs, independently
)

// DecodeError identifies the first detected violation and its byte offset.
type DecodeError struct {
	Reason string
	Offset int
	Detail string
}

func (e *DecodeError) Error() string {
	return fmt.Sprintf("%s at byte %d: %s", e.Reason, e.Offset, e.Detail)
}
func fail(reason string, offset int, detail string) error {
	return &DecodeError{reason, offset, detail}
}

type decoder struct {
	b   []byte
	pos int
}

// DecodeStrict consumes exactly one canonical item. It never normalizes text,
// silently drops keys, or consults the vector metadata.
func DecodeStrict(b []byte) (Value, error) {
	if len(b) > MaxBytes {
		return nil, fail("oversize", 0, fmt.Sprintf("input %d exceeds %d bytes", len(b), MaxBytes))
	}
	d := decoder{b: b}
	v, err := d.item(0)
	if err != nil {
		return nil, err
	}
	if d.pos != len(b) {
		return nil, fail("trailing_bytes", d.pos, fmt.Sprintf("%d unconsumed bytes", len(b)-d.pos))
	}
	return v, nil
}
func (d *decoder) take(n int) ([]byte, error) {
	if n > len(d.b)-d.pos {
		return nil, fail("truncated", d.pos, "insufficient bytes")
	}
	p := d.b[d.pos : d.pos+n]
	d.pos += n
	return p, nil
}

// arg decodes an unsigned integer or a definite length and checks minimal width.
func (d *decoder) arg(ai byte, start int, length bool) (uint64, error) {
	if ai < 24 {
		return uint64(ai), nil
	}
	if ai == 31 {
		return 0, fail("indefinite_length", start, "additional information 31 is forbidden")
	}
	if ai > 27 {
		return 0, fail("reserved_additional_info", start, "additional information 28–30 is reserved")
	}
	width := 1 << uint(ai-24)
	p, err := d.take(width)
	if err != nil {
		return 0, err
	}
	var n uint64
	for _, x := range p {
		n = n<<8 | uint64(x)
	}
	min := [...]uint64{24, 256, 65536, 4294967296}
	if n < min[ai-24] {
		reason := "non_shortest_integer"
		if length {
			reason = "non_shortest_length"
		}
		return 0, fail(reason, start, "argument uses a wider encoding than necessary")
	}
	return n, nil
}
func (d *decoder) item(depth int) (Value, error) {
	start := d.pos
	p, err := d.take(1)
	if err != nil {
		return nil, err
	}
	major, ai := p[0]>>5, p[0]&31
	switch major {
	case 1:
		return nil, fail("forbidden_major_type", start, "negative integers are outside this profile")
	case 6:
		return nil, fail("tags", start, "CBOR tags are forbidden")
	case 7:
		switch ai {
		case 20:
			return false, nil
		case 21:
			return true, nil
		case 22:
			return nil, nil
		case 25, 26, 27:
			return nil, fail("floats", start, "floating-point values are forbidden")
		case 31:
			return nil, fail("indefinite_length", start, "break is forbidden")
		default:
			return nil, fail("forbidden_simple_value", start, "only false, true and null are allowed in direct form")
		}
	}
	n, err := d.arg(ai, start, major != 0)
	if err != nil {
		return nil, err
	}
	switch major {
	case 0:
		return n, nil
	case 2, 3:
		if n > MaxBytes {
			return nil, fail("oversize", start, "declared string length exceeds byte limit")
		}
		if n > uint64(len(d.b)-d.pos) {
			return nil, fail("truncated", d.pos, "declared string length exceeds remaining input")
		}
		p, _ := d.take(int(n))
		if major == 2 {
			b := make([]byte, len(p))
			copy(b, p)
			return b, nil
		}
		if !utf8.Valid(p) {
			return nil, fail("invalid_utf8", start, "text string is not valid UTF-8")
		}
		return string(p), nil
	case 4, 5:
		if depth >= MaxDepth {
			return nil, fail("nesting_depth", start, "container nesting exceeds limit")
		}
		if n > MaxCollection {
			return nil, fail("collection_count", start, "declared collection count exceeds limit")
		}
		remaining := uint64(len(d.b) - d.pos)
		if major == 4 {
			a := make([]Value, 0, int(min(n, remaining)))
			for i := uint64(0); i < n; i++ {
				v, e := d.item(depth + 1)
				if e != nil {
					return nil, e
				}
				a = append(a, v)
			}
			return a, nil
		}
		m := make(Map, int(min(n, remaining/2)))
		var prev []byte
		for i := uint64(0); i < n; i++ {
			kstart := d.pos
			if d.pos == len(d.b) {
				return nil, fail("truncated", d.pos, "missing map key")
			}
			major := d.b[d.pos] >> 5
			if major != 0 && major != 3 {
				return nil, fail("forbidden_map_key", kstart, "map keys must be unsigned integers or text")
			}
			k, e := d.item(depth + 1)
			if e != nil {
				return nil, e
			}
			raw := d.b[kstart:d.pos]
			if _, ok := m[k]; ok {
				return nil, fail("duplicate_map_key", kstart, "key repeats")
			}
			if i > 0 && bytes.Compare(prev, raw) >= 0 {
				return nil, fail("unsorted_map_keys", kstart, "key encoding does not increase bytewise")
			}
			v, e := d.item(depth + 1)
			if e != nil {
				return nil, e
			}
			m[k] = v
			prev = raw
		}
		return m, nil
	}
	return nil, fail("forbidden_major_type", start, "unsupported major type")
}

// Encode produces shortest-form deterministic CBOR for a valid Value.
// It returns nil for unsupported types, invalid UTF-8, or exceeded limits.
// No valid encoding is empty. It does not mutate the input Value.
func Encode(v Value) []byte {
	e := encoder{}
	if !e.item(v, 0) {
		return nil
	}
	return e.b
}

type encoder struct{ b []byte }

func (e *encoder) put(p ...byte) bool {
	if len(p) > MaxBytes-len(e.b) {
		return false
	}
	e.b = append(e.b, p...)
	return true
}
func (e *encoder) head(major byte, n uint64) bool {
	base := major << 5
	var b [9]byte
	switch {
	case n < 24:
		return e.put(base | byte(n))
	case n <= 0xff:
		return e.put(base|24, byte(n))
	case n <= 0xffff:
		b[0] = base | 25
		binary.BigEndian.PutUint16(b[1:3], uint16(n))
		return e.put(b[:3]...)
	case n <= 0xffffffff:
		b[0] = base | 26
		binary.BigEndian.PutUint32(b[1:5], uint32(n))
		return e.put(b[:5]...)
	default:
		b[0] = base | 27
		binary.BigEndian.PutUint64(b[1:9], n)
		return e.put(b[:]...)
	}
}
func (e *encoder) item(v Value, depth int) bool {
	switch x := v.(type) {
	case nil:
		return e.put(0xf6)
	case bool:
		if x {
			return e.put(0xf5)
		}
		return e.put(0xf4)
	case uint64:
		return e.head(0, x)
	case []byte:
		return len(x) <= MaxBytes && e.head(2, uint64(len(x))) && e.put(x...)
	case string:
		return len(x) <= MaxBytes && utf8.ValidString(x) && e.head(3, uint64(len(x))) && e.put([]byte(x)...)
	case []Value:
		if depth >= MaxDepth || len(x) > MaxCollection || !e.head(4, uint64(len(x))) {
			return false
		}
		for _, v := range x {
			if !e.item(v, depth+1) {
				return false
			}
		}
		return true
	case Map:
		if depth >= MaxDepth || len(x) > MaxCollection || !e.head(5, uint64(len(x))) {
			return false
		}
		type entry struct {
			key   []byte
			value Value
		}
		entries := make([]entry, 0, len(x))
		keyBytes := 0
		for k, v := range x {
			switch k.(type) {
			case uint64, string:
			default:
				return false
			}
			ke := encoder{}
			if !ke.item(k, depth+1) {
				return false
			}
			keyBytes += len(ke.b)
			if keyBytes > MaxBytes-len(e.b) {
				return false
			}
			entries = append(entries, entry{ke.b, v})
		}
		sort.Slice(entries, func(i, j int) bool { return bytes.Compare(entries[i].key, entries[j].key) < 0 })
		for _, p := range entries {
			if !e.put(p.key...) || !e.item(p.value, depth+1) {
				return false
			}
		}
		return true
	default:
		return false
	}
}
