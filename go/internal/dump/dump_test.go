package dump

import (
	"strings"
	"testing"

	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/parser"
)

func dumpSrc(src string) string {
	return Model(model.Build(parser.Parse(src, "strict")))
}

func TestModelIntegration(t *testing.T) {
	src := `---
gridmd: "1.0"
title: T
date-system: 1904
names:
  - juststring
  - { name: Zeta, value: '{"a"}' }
  - { name: Alpha, ref: "S!A1" }
  - { name: Beta, formula: "LAMBDA(x,x)" }
---
# S
` + "```{sheet}\nhidden: very\nfreeze: B2\nprotect: { enabled: true }\n```" + `
@ A1 5
@ A2 TRUE
@ A3 2026-07-04
@ A4 12:30
@ A5 #N/A
@ A6 "text"
@ A7 =B1 :: 3
@ A8 =C1 { spill: A8:A9 }
@ A9
  rich:
    - { text: "hi " }
    - { text: "there" }
@ B1:B3 { merge: true }
` + "```{table} TabB at E1\n---\n| h |\n| 1 |\n```" + `
` + "```{table} TabA at F1\n---\n| k |\n| 2 |\n```" + `
# Empty
` + "```{sheet}\nkind: chart\n```" + `
`
	out := dumpSrc(src)
	for _, want := range []string{
		`"dateSystem": 1904`, `"hidden": "very"`, `"freeze": "B2"`, `"protected": true`,
		`"t": "n"`, `"t": "b"`, `"t": "d"`, `"t": "e"`, `"t": "s"`, `"t": "f"`,
		`"array": "A8:A9"`, `"cached": null`, `"merges": [`, `"t": "rich"`,
		`"v": "hi there"`, `"cells": {}`, `"bodyRows": 1`, `"hasTotals": false`,
	} {
		if !strings.Contains(out, want) {
			t.Errorf("dump missing %q", want)
		}
	}
	// tables sorted by name: TabA before TabB.
	if strings.Index(out, `"TabA"`) > strings.Index(out, `"TabB"`) {
		t.Error("tables not sorted by name")
	}
	// Names sorted Alpha, Beta, Zeta (the non-map entry is dropped).
	ai := strings.Index(out, `"Alpha"`)
	bi := strings.Index(out, `"Beta"`)
	zi := strings.Index(out, `"Zeta"`)
	if !(ai != -1 && ai < bi && bi < zi) {
		t.Errorf("name order wrong: Alpha=%d Beta=%d Zeta=%d", ai, bi, zi)
	}
}

func TestScalarLikeAndStringify(t *testing.T) {
	for _, v := range []interface{}{nil, "s", true, 1.5} {
		if scalarLike(v) != v {
			t.Errorf("scalarLike passthrough %v", v)
		}
	}
	if scalarLike([]int{}) != nil {
		t.Error("scalarLike default")
	}
	if stringify("x") != "x" || stringify(true) != "true" || stringify(false) != "false" ||
		stringify(3.0) != "3" || stringify(nil) != "" || stringify([]int{}) != "" {
		t.Error("stringify")
	}
}

func TestSmallHelpers(t *testing.T) {
	if hiddenValue(true) != true || hiddenValue("very") != "very" || hiddenValue(false) != false || hiddenValue("x") != false {
		t.Error("hiddenValue")
	}
	if !boolish(true) || boolish(false) || boolish("x") {
		t.Error("boolish")
	}
	if asString("y") != "y" || asString(5) != "" {
		t.Error("asString")
	}
	if refOrNil(map[string]interface{}{"ref": "A1"}, "ref") != "A1" {
		t.Error("refOrNil present")
	}
	if refOrNil(map[string]interface{}{}, "ref") != nil {
		t.Error("refOrNil absent")
	}
	if refOrNil(map[string]interface{}{"ref": nil}, "ref") != nil {
		t.Error("refOrNil nil value")
	}
}

func TestSerializePrimitives(t *testing.T) {
	var b strings.Builder
	serialize(&b, false, 0)
	serialize(&b, 2.5, 0)
	serialize(&b, nil, 0)
	serialize(&b, 42, 0) // unknown type -> "null"
	serialize(&b, []interface{}{}, 0)
	if b.String() != "false2.5nullnull[]" {
		t.Errorf("serialize primitives = %q", b.String())
	}
}

func TestWriteJSONString(t *testing.T) {
	var b strings.Builder
	// quote, backslash, the named control chars, a \u-escaped control char, and
	// two non-ASCII runes (U+00B7, U+00E9) passed through verbatim.
	in := "a\"b\\c\n\t\r\b\f\x01·é"
	writeJSONString(&b, in)
	want := "\"a\\\"b\\\\c\\n\\t\\r\\b\\f\\u0001·é\""
	if b.String() != want {
		t.Errorf("writeJSONString = %q, want %q", b.String(), want)
	}
}
