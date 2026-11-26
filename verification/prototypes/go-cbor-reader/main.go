// SPDX-License-Identifier: MPL-2.0
package main

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"text/tabwriter"
)

type vector struct {
	ID         string  `json:"id"`
	Wire       *string `json:"wire_hex"`
	ErrorClass string  `json:"error_class"`
}
type corpus struct {
	Positive []vector `json:"positive"`
	Negative []vector `json:"negative"`
}
type tally struct{ positive, positiveTotal, cbor, cborTotal, other, otherTotal int }

func layerNegative(v vector) bool {
	return v.ErrorClass == "non_canonical" || v.ErrorClass == "max_bytes"
}
func readCorpus(path string) (corpus, error) {
	var c corpus
	b, err := os.ReadFile(path)
	if err != nil {
		return c, err
	}
	if err = json.Unmarshal(b, &c); err != nil {
		return c, err
	}
	if c.Positive == nil || c.Negative == nil {
		return c, fmt.Errorf("missing positive or negative array")
	}
	return c, nil
}
func wire(v vector) ([]byte, error) {
	if v.Wire == nil {
		return nil, fmt.Errorf("missing wire_hex")
	}
	return hex.DecodeString(*v.Wire)
}
func replay(c corpus) (t tally, issues []string, bad bool) {
	t.positiveTotal = len(c.Positive)
	for _, v := range c.Positive {
		b, e := wire(v)
		if e != nil {
			issues = append(issues, fmt.Sprintf("%s: positive fixture error: %v", v.ID, e))
			bad = true
			continue
		}
		decoded, e := DecodeStrict(b)
		if e != nil {
			issues = append(issues, fmt.Sprintf("%s: positive rejected: %v", v.ID, e))
			bad = true
			continue
		}
		if !bytes.Equal(Encode(decoded), b) {
			issues = append(issues, fmt.Sprintf("%s: positive re-encoding differs from wire_hex", v.ID))
			bad = true
			continue
		}
		t.positive++
	}
	for _, v := range c.Negative {
		strict := layerNegative(v)
		if strict {
			t.cborTotal++
		} else {
			t.otherTotal++
		}
		b, e := wire(v)
		if e != nil {
			issues = append(issues, fmt.Sprintf("%s: negative fixture error: %v", v.ID, e))
			bad = true
			continue
		}
		_, e = DecodeStrict(b)
		if strict {
			if e == nil {
				issues = append(issues, fmt.Sprintf("%s: expected CBOR rejection (%s), accepted", v.ID, v.ErrorClass))
				bad = true
			} else {
				t.cbor++
			}
		} else {
			if e == nil {
				t.other++
			} else {
				issues = append(issues, fmt.Sprintf("%s: unexpected CBOR-layer rejection (semantic class %s): %v", v.ID, v.ErrorClass, e))
			}
		}
	}
	return
}
func run(args []string, out, errout io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(errout, "usage: go run . bridges-vectors.json plugins-vectors.json")
		return 1
	}
	w := tabwriter.NewWriter(out, 0, 4, 2, ' ', 0)
	fmt.Fprintln(w, "File\tPositives passed/total\tCBOR-layer negatives rejected/total\tOther negatives decoded OK/total")
	bad := false
	var issues []string
	for _, path := range args {
		c, e := readCorpus(path)
		if e != nil {
			issues = append(issues, fmt.Sprintf("%s: fixture error: %v", path, e))
			bad = true
			continue
		}
		t, lines, failed := replay(c)
		bad = bad || failed
		fmt.Fprintf(w, "%s\t%d/%d\t%d/%d\t%d/%d\n", path, t.positive, t.positiveTotal, t.cbor, t.cborTotal, t.other, t.otherTotal)
		for _, line := range lines {
			issues = append(issues, path+": "+line)
		}
	}
	w.Flush()
	fmt.Fprintln(out, "\nDisagreements:")
	if len(issues) == 0 {
		fmt.Fprintln(out, "none")
	} else {
		for _, line := range issues {
			fmt.Fprintln(out, line)
		}
	}
	if bad {
		return 1
	}
	return 0
}
func main() { os.Exit(run(os.Args[1:], os.Stdout, os.Stderr)) }
