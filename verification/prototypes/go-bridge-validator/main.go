package main

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
)

type vector struct {
	ID, Object string
	Wire       string          `json:"wire_hex"`
	Error      string          `json:"error_class"`
	Prior      json.RawMessage `json:"prior_value"`
	Control    json.RawMessage `json:"control_value"`
	Mapping    json.RawMessage `json:"mapping_value"`
	Signatures json.RawMessage
	Receiver   string `json:"receiver_wave"`
	Now        *uint64
}

func diagnostic(raw json.RawMessage) (Value, error) {
	if len(raw) == 0 {
		return nil, nil
	}

	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()

	var v any
	if e := d.Decode(&v); e != nil {
		return nil, e
	}

	return convert(v)
}

func convert(v any) (Value, error) {
	switch x := v.(type) {
	case json.Number:
		return strconv.ParseUint(string(x), 10, 64)

	case string, bool, nil:
		return x, nil

	case []any:
		a := make([]Value, 0, len(x))

		for _, v := range x {
			y, e := convert(v)
			if e != nil {
				return nil, e
			}
			a = append(a, y)
		}

		return a, nil

	case map[string]any:
		if b, ok := x["$bytes"]; ok {
			s, ok := b.(string)
			if !ok || len(x) != 1 {
				return nil, fmt.Errorf("invalid bytes wrapper")
			}
			return hex.DecodeString(s)
		}

		m := Map{}

		for k, v := range x {
			var key Value = k

			if n, e := strconv.ParseUint(k, 10, 64); e == nil &&
				strconv.FormatUint(n, 10) == k {
				key = n
			}

			y, e := convert(v)
			if e != nil {
				return nil, e
			}

			m[key] = y
		}

		return m, nil
	}

	return nil, fmt.Errorf("unsupported diagnostic type %T", v)
}

func run(path string) int {
	b, e := os.ReadFile(path)
	if e != nil {
		fmt.Fprintln(os.Stderr, e)
		return 1
	}

	var corpus struct {
		Positive, Negative []vector
	}

	if e = json.Unmarshal(b, &corpus); e != nil {
		fmt.Fprintln(os.Stderr, e)
		return 1
	}

	pass := [2]int{}
	total := [2]int{len(corpus.Positive), len(corpus.Negative)}

	for i, group := range [][]vector{corpus.Positive, corpus.Negative} {
		for _, x := range group {
			ctx := Context{
				ReceiverWave: x.Receiver,
				Now:          x.Now,
			}

			var err error

			for _, p := range []struct {
				raw json.RawMessage
				dst *Value
			}{
				{x.Prior, &ctx.PriorValue},
				{x.Control, &ctx.ControlValue},
				{x.Mapping, &ctx.MappingValue},
			} {
				*p.dst, err = diagnostic(p.raw)
				if err != nil {
					break
				}
			}

			got := ""
			var sig Value
			var wire []byte

			if err == nil {
				sig, err = diagnostic(x.Signatures)

				if sig != nil {
					var ok bool
					ctx.Signatures, ok = sig.([]Value)

					if !ok {
						err = fmt.Errorf("invalid signatures fixture")
					}
				}
			}

			if err == nil {
				wire, err = hex.DecodeString(x.Wire)
			}

			if err != nil {
				got = "fixture_error: " + err.Error()
			} else {
				var v Value

				v, got = Validate(x.Object, wire, ctx)

				if i == 0 && got == "" &&
					!bytes.Equal(Encode(v), wire) {
					got = "reencode_mismatch"
				}
			}

			expected := x.Error

			if i == 0 {
				expected = ""
			}

			if got != expected || (i == 1 && expected == "") {
				fmt.Printf(
					"%s expected=%q got=%q\n",
					x.ID, expected, got,
				)
			} else {
				pass[i]++
			}
		}
	}

	fmt.Printf(
		"positive %d/%d; negative %d/%d; total %d/%d; mismatches %d\n",
		pass[0], total[0],
		pass[1], total[1],
		pass[0]+pass[1], total[0]+total[1],
		total[0]+total[1]-pass[0]-pass[1],
	)

	if pass != total {
		return 1
	}

	return 0
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(
			os.Stderr,
			"usage: go run . bridges-vectors.json",
		)
		os.Exit(1)
	}

	os.Exit(run(os.Args[1]))
}
