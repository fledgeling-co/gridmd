package model

import (
	"testing"

	"github.com/lprhodes/grid-md/go/internal/parser"
	"github.com/lprhodes/grid-md/go/internal/scalar"
)

func build(src string) *Workbook {
	return Build(parser.Parse(src, "strict"))
}

func cellAtRef(s *Sheet, col, row int) *Cell {
	for _, c := range s.Cells() {
		if c.Col == col && c.Row == row {
			return c
		}
	}
	return nil
}

func TestBuildCounts(t *testing.T) {
	src := `---
gridmd: "0.1"
date-system: 1904
---

# Data

` + "```{sheet}\nhidden: very\n```" + `

` + "```{grid} F1\n| 1 | 2 |\n```" + `

@ A1 =SORT(B1:B3) { spill: A1:A3 }
` + "```{spill-cache} A1\n| x |\n| y |\n```" + `

` + "```{table} T at A5\ntotal:\n  q: =SUM([q])\ncols:\n  q: { numfmt: \"0\" }\n---\n| q |\n| 1 |\n| 2 |\n```" + `

` + "```{cf} B1:B3\n- when: \"> 1\"\n- bars: { color: \"#123456\" }\n```" + `

` + "```{validation} C1:C3\ntype: list\nvalues: [a, b]\n```" + `

` + "```{sparklines} E1:E3\ntype: line\nsource: F1:F3\n```" + `

` + "```{pivot} P at H1\nsource: T\n```" + `

` + "```{slicer} at M1\nfor: T\nfield: q\n```" + `

` + "```{image} at M8\nsrc: logo.png\n```" + `

` + "```{shape} rect at N1\n```" + `

` + "```{textbox} at O1\ntext: hi\n```" + `

` + "```{comments} B2\n- by: X\n  at: 2026-01-01T00:00:00Z\n  text: hi\n```" + `

` + "```{scenario} Sc\ncells: { B1: 9 }\n```" + `

` + "```{filter} A1:C3\ncols: {}\n```" + `

@ D1 "linked" { link: "https://x", tip: "t" }
@ D2 { note: "prop note" }
@ D3
  note: |
    body note
@ E5 TRUE { control: checkbox }
@ A10:C10 { merge: true }
`
	wb := build(src)
	if len(wb.Sheets) != 1 {
		t.Fatalf("sheets=%d", len(wb.Sheets))
	}
	s := wb.Sheets[0]
	got := map[string]int{
		"cf": s.CFRules, "validations": s.Validations, "notes": s.Notes,
		"threads": s.Threads, "scenarios": s.Scenarios, "sparklines": s.Sparklines,
		"charts": s.Charts, "pivots": s.Pivots, "slicers": s.Slicers, "images": s.Images,
		"shapes": s.Shapes, "hyperlinks": s.Hyperlinks,
	}
	want := map[string]int{
		"cf": 2, "validations": 1, "notes": 2, "threads": 1, "scenarios": 1,
		"sparklines": 1, "charts": 0, "pivots": 1, "slicers": 1, "images": 1,
		"shapes": 2, "hyperlinks": 1,
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("count %s = %d, want %d", k, got[k], v)
		}
	}
	if s.Meta["hidden"] != "very" {
		t.Errorf("meta hidden = %v", s.Meta["hidden"])
	}
	if len(s.Merges) != 1 {
		t.Errorf("merges = %d", len(s.Merges))
	}
	if len(s.Tables) != 1 || s.Tables[0].BodyRows != 2 || !s.Tables[0].HasTotals {
		t.Errorf("table = %+v", s.Tables)
	}
	// spill anchor merged its first cached cell.
	a1 := cellAtRef(s, 1, 1)
	if a1 == nil || a1.Content.Formula == nil || a1.Content.Cached == nil || a1.Content.Cached.Str != "x" {
		t.Errorf("A1 = %+v", a1.Content)
	}
	if a1.Content.ArrayRef == nil || *a1.Content.ArrayRef != "A1:A3" {
		t.Errorf("A1 arrayRef = %v", a1.Content.ArrayRef)
	}
}

func TestBuildChartSheetAndBodyContent(t *testing.T) {
	src := `---
gridmd: "0.1"
---

# Chart

` + "```{sheet}\nkind: chart\n```" + `

` + "```{chart} column \"T\" at sheet\nseries:\n  - { val: A }\n```" + `

# Rich

@ A1
  rich:
    - { text: "Hello " }
    - { text: "world" }
@ A2
  entity: { type: stock, id: "MSFT" }
  fields: { Price: 1 }
@ A3
  entity: { text: "T" }
@ A4
  value: 42
@ A5
  formula: =SUM(B:B)
  value: 5
@ A6
  entity: not-a-map
`
	wb := build(src)
	if wb.Sheets[0].Kind != "chart" || wb.Sheets[0].Charts != 1 {
		t.Errorf("chart sheet = %+v", wb.Sheets[0])
	}
	rich := wb.Sheets[1]
	a1 := cellAtRef(rich, 1, 1)
	if a1 == nil || a1.Content.Rich == nil {
		t.Fatalf("A1 rich missing")
	}
	if a2 := cellAtRef(rich, 1, 2); a2.Content.Scalar.Str != "MSFT" {
		t.Errorf("entity id text = %q", a2.Content.Scalar.Str)
	}
	if a3 := cellAtRef(rich, 1, 3); a3.Content.Scalar.Str != "T" {
		t.Errorf("entity text = %q", a3.Content.Scalar.Str)
	}
	if a4 := cellAtRef(rich, 1, 4); a4.Content.Scalar.Kind != scalar.Number || a4.Content.Scalar.Num != 42 {
		t.Errorf("value = %+v", a4.Content.Scalar)
	}
	if a5 := cellAtRef(rich, 1, 5); a5.Content.Formula == nil || a5.Content.Cached == nil {
		t.Errorf("body formula+value = %+v", a5.Content)
	}
	if a6 := cellAtRef(rich, 1, 6); a6.Content.Scalar.Str != "" {
		t.Errorf("entity non-map = %q", a6.Content.Scalar.Str)
	}
}

func TestSetContentNoMerge(t *testing.T) {
	// Duplicate definitions: the second is dropped (model does not error).
	wb := build("---\ngridmd: \"0.1\"\n---\n# S\n@ A1 1\n@ A1 2\n")
	a1 := cellAtRef(wb.Sheets[0], 1, 1)
	if a1.Content.Scalar.Num != 1 {
		t.Errorf("expected first def to win, got %v", a1.Content.Scalar.Num)
	}
}

func TestNilAnchors(t *testing.T) {
	wb := build("---\ngridmd: \"0.1\"\n---\n# S\n```{grid}\n| a |\n```\n```{spill-cache}\n| a |\n```\n```{table} T\n---\n| h |\n```\n")
	if len(wb.Sheets[0].Cells()) != 0 || len(wb.Sheets[0].Tables) != 0 {
		t.Errorf("nil anchors should produce nothing: cells=%d tables=%d", len(wb.Sheets[0].Cells()), len(wb.Sheets[0].Tables))
	}
}

func TestRangeFillAndArrayCSE(t *testing.T) {
	wb := build("---\ngridmd: \"0.1\"\n---\n# S\n@ A1:A3 =B1*2\n@ D1 =C1 { array: D1:D2 }\n")
	s := wb.Sheets[0]
	a2 := cellAtRef(s, 1, 2)
	if a2 == nil || a2.Content.Formula == nil || *a2.Content.Formula != "B2*2" {
		t.Errorf("fill A2 = %+v", a2)
	}
	d1 := cellAtRef(s, 4, 1)
	if d1 == nil || !d1.Content.CSE || d1.Content.ArrayRef == nil {
		t.Errorf("array cse = %+v", d1.Content)
	}
}

func TestBuildEdgeCases(t *testing.T) {
	src := "---\ngridmd: \"0.1\"\n---\n# S\n" +
		"@ A1 =X { spill: A1:A3 }\n" +
		"```{spill-cache} A1\n| p |\n|  |\n| r |\n```\n" + // blank middle cell
		"```{table} T at C1\nheader: false\ntotal:\n  nope: 5\n---\n| a | 1 |\n| b | 2 |\n```\n" +
		"@ ?? 1\n" + // invalid @ target
		"@ E1\n  formula: =SUM(B:B)\n  spill: E1:E3\n" +
		"@ F1\n  formula: =SUM(B:B)\n  array: F1:F3\n"
	wb := build(src)
	s := wb.Sheets[0]
	// header:false table keeps no header columns; total col unknown → skipped.
	if len(s.Tables) != 1 || len(s.Tables[0].Columns) != 0 {
		t.Errorf("header:false table = %+v", s.Tables)
	}
	// spill-cache blank cell leaves that position empty.
	if c := cellAtRef(s, 1, 2); c != nil {
		t.Errorf("A2 should be empty (blank spill-cache cell), got %+v", c)
	}
	if e1 := cellAtRef(s, 5, 1); e1.Content.ArrayRef == nil || e1.Content.CSE {
		t.Errorf("body spill = %+v", e1.Content)
	}
	if f1 := cellAtRef(s, 6, 1); f1.Content.ArrayRef == nil || !f1.Content.CSE {
		t.Errorf("body array (cse) = %+v", f1.Content)
	}
}

func TestYamlScalar(t *testing.T) {
	cases := []struct {
		in   interface{}
		kind scalar.Kind
	}{
		{5.0, scalar.Number},
		{true, scalar.Boolean},
		{"2026-07-04", scalar.Date},
		{"12:30", scalar.Time},
		{"hello", scalar.Text},
	}
	for _, c := range cases {
		if got := yamlScalar(c.in); got.Kind != c.kind {
			t.Errorf("yamlScalar(%v).Kind = %q, want %q", c.in, got.Kind, c.kind)
		}
	}
}

func TestTranslateFormula(t *testing.T) {
	cases := []struct {
		in     string
		dr, dc int
		want   string
	}{
		{"A1", 1, 1, "B2"},
		{"$A$1", 1, 1, "$A$1"},
		{"$A1", 1, 1, "$A2"},
		{"A$1", 1, 1, "B$1"},
		{`"A1"`, 1, 1, `"A1"`},
		{"SUM(A1:A3)", 1, 0, "SUM(A2:A4)"},
		{"A1(", 1, 0, "A1("}, // ref immediately followed by ( is not a ref
		{"_A1", 1, 0, "_A1"}, // preceded by an identifier char
		{"'My Sheet'!A1", 1, 0, "'My Sheet'!A2"},
		{`"a""b" A1`, 1, 0, `"a""b" A2`},
		{`"unterminated`, 1, 1, `"unterminated`},
	}
	for _, c := range cases {
		if got := TranslateFormula(c.in, c.dr, c.dc); got != c.want {
			t.Errorf("TranslateFormula(%q,%d,%d) = %q, want %q", c.in, c.dr, c.dc, got, c.want)
		}
	}
}

func TestHelpers(t *testing.T) {
	if truthy(nil) || truthy(false) || truthy("") || truthy(0.0) {
		t.Error("falsy values")
	}
	if !truthy(true) || !truthy("x") || !truthy(1.0) || !truthy([]int{}) {
		t.Error("truthy values")
	}
	if coalesce(nil, "x") != "x" || coalesce("a", "b") != "a" {
		t.Error("coalesce")
	}
	if toStr(nil) != "" || toStr("x") != "x" || toStr(true) != "true" || toStr(false) != "false" || toStr(5.0) != "5" || toStr([]int{}) != "" {
		t.Error("toStr")
	}
	if r := toRuns("not a list"); r != nil {
		t.Error("toRuns non-list")
	}
	if r := toRuns([]interface{}{map[string]interface{}{"text": "a"}, "bad"}); len(r) != 2 || r[1] == nil {
		t.Errorf("toRuns = %#v", r)
	}
	if columnIndex([]string{"a", "b"}, "B") != 1 || columnIndex([]string{"a"}, "z") != -1 {
		t.Error("columnIndex")
	}
	if first(nil) != "" || first([]string{"x"}) != "x" {
		t.Error("first")
	}
	if maxInt(3, 1) != 3 || maxInt(1, 3) != 3 {
		t.Error("maxInt")
	}
}
