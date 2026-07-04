package parser

import (
	"strings"
	"testing"
)

func TestFindPropsSplit(t *testing.T) {
	cases := []struct {
		in     string
		scalar string
		props  string
		ok     bool
	}{
		{"abc", "abc", "", false},         // no trailing }
		{"a}", "a}", "", false},           // depth < 0
		{"{{a}", "{{a}", "", false},       // never balanced to depth 0 group
		{"{a: 1}", "{a: 1}", "", false},   // group starts at 0
		{"x{a: 1}", "x{a: 1}", "", false}, // no preceding space
		{`"L" { link: "u" }`, `"L"`, `{ link: "u" }`, true},
		{`=SORT(A1:A3) { spill: A1:A3 }`, `=SORT(A1:A3)`, `{ spill: A1:A3 }`, true},
	}
	for _, c := range cases {
		s, p, ok := FindPropsSplit(c.in)
		if s != c.scalar || p != c.props || ok != c.ok {
			t.Errorf("FindPropsSplit(%q) = (%q,%q,%v), want (%q,%q,%v)", c.in, s, p, ok, c.scalar, c.props, c.ok)
		}
	}
}

func TestSplitPipeRow(t *testing.T) {
	cases := []struct {
		in   string
		want []string
		ok   bool
	}{
		{"| a | b |", []string{"a", "b"}, true},
		{"| a |  | b |", []string{"a", "", "b"}, true},
		{`| a\|b | c |`, []string{"a|b", "c"}, true},
		{"x", nil, false},
		{"|", nil, false},
		{"| unclosed", nil, false},
		{`| a\`, nil, false}, // trailing backslash, no closing pipe
	}
	for _, c := range cases {
		got, ok := SplitPipeRow(c.in)
		if ok != c.ok || (ok && !equal(got, c.want)) {
			t.Errorf("SplitPipeRow(%q) = (%v,%v), want (%v,%v)", c.in, got, ok, c.want, c.ok)
		}
	}
}

func TestParseInfoArgs(t *testing.T) {
	var errs []Diag
	a := ParseInfoArgs(`col "My Title" at B2:D9 size 480x320 lang=js part="x/y.xml" q=a""b`, 1, &errs)
	if len(a.Positional) != 2 || a.Positional[0] != "col" || a.Positional[1] != "My Title" {
		t.Errorf("positional = %#v", a.Positional)
	}
	if a.Anchor == nil || *a.Anchor != "B2:D9" {
		t.Errorf("anchor = %v", a.Anchor)
	}
	if a.Size == nil || a.Size.W != 480 || a.Size.H != 320 {
		t.Errorf("size = %v", a.Size)
	}
	if a.Flags["lang"] != "js" || a.Flags["part"] != "x/y.xml" || a.Flags["q"] != `a"b` {
		t.Errorf("flags = %#v", a.Flags)
	}
	if len(errs) != 0 {
		t.Errorf("unexpected errs %v", errs)
	}
}

func TestParseInfoArgsErrors(t *testing.T) {
	var errs []Diag
	ParseInfoArgs("at", 1, &errs)
	ParseInfoArgs("size", 2, &errs)
	ParseInfoArgs("size nope", 3, &errs)
	if len(errs) != 3 {
		t.Fatalf("want 3 errors, got %d: %v", len(errs), errs)
	}
}

func TestParseYamlAndTryProps(t *testing.T) {
	var errs []Diag
	v := ParseYaml("a: 1", 1, &errs)
	if m, ok := v.(map[string]interface{}); !ok || m["a"] != float64(1) {
		t.Errorf("ParseYaml = %#v", v)
	}
	ParseYaml("{ bad", 5, &errs)
	if len(errs) == 0 {
		t.Error("expected a YAML error diag")
	}
	if _, ok := TryProps("{ a: 1 }"); !ok {
		t.Error("TryProps passthrough failed")
	}
}

func TestParseFrontmatterErrors(t *testing.T) {
	if d := Parse("no frontmatter here", "strict"); len(d.Errors) == 0 {
		t.Error("expected missing-frontmatter error")
	}
	if d := Parse("---\ngridmd: \"0.1\"", "strict"); len(d.Errors) == 0 {
		t.Error("expected unterminated-frontmatter error")
	}
}

func TestParseDocumentStructure(t *testing.T) {
	src := strings.Join([]string{
		"---",
		"gridmd: \"0.1\"",
		"---",
		"",
		"> a doc comment",
		"## a level-2 heading (ignored)",
		"```{query} Q",
		"source: x",
		"```",
		"# Sheet1",
		"```{sheet}",
		"freeze: B2",
		"```",
		"```{grid} A1",
		"| a | 1 |",
		"```",
		"```{spill-cache} A1",
		"| x |",
		"```",
		"```{table} T at C1",
		"style: s",
		"---",
		"| h |",
		"| v |",
		"```",
		"```{script} S lang=js",
		"on: manual",
		"---",
		"code()",
		"```",
		"```{raw} ooxml part=\"customXml/i.xml\"",
		"<x/>",
		"```",
		"```{x-custom} foo",
		"payload",
		"```",
		"```{cf} A1:A2",
		"- when: \"> 1\"",
		"```",
		"@ A1 \"hi\"",
		"@ B1 { fill: \"#fff\" }",
		"@ C1 =A1 { bold: true }",
		"@ D1",
		"  note: |",
		"    a note",
		"",
	}, "\n")
	d := Parse(src, "strict")
	if len(d.Errors) != 0 {
		t.Fatalf("unexpected errors: %v", d.Errors)
	}
	if len(d.Sheets) != 1 {
		t.Fatalf("sheets = %d", len(d.Sheets))
	}
	if len(d.WorkbookBlocks) != 1 {
		t.Fatalf("workbook blocks = %d", len(d.WorkbookBlocks))
	}
	sheet := d.Sheets[0]
	kinds := map[string]int{}
	ats := 0
	for _, b := range sheet.Blocks {
		switch blk := b.(type) {
		case *FenceBlock:
			kinds[blk.Kind]++
		case *AtBlock:
			ats++
		}
	}
	for _, k := range []string{"sheet", "grid", "spill-cache", "table", "script", "raw", "x-custom", "cf"} {
		if kinds[k] != 1 {
			t.Errorf("kind %q count = %d", k, kinds[k])
		}
	}
	if ats != 4 {
		t.Errorf("@ blocks = %d, want 4", ats)
	}
}

func TestParseAtBodyNonMapAndUnclosedFence(t *testing.T) {
	// @ body that is not a mapping.
	d := Parse("---\ngridmd: \"0.1\"\n---\n# S\n@ A1\n  - not a map\n", "strict")
	if !hasMsg(d.Errors, "must be a YAML mapping") {
		t.Errorf("expected non-map body error, got %v", d.Errors)
	}
	// Unclosed fence.
	d = Parse("---\ngridmd: \"0.1\"\n---\n# S\n```{grid} A1\n| a |\n", "strict")
	if !hasMsg(d.Errors, "unclosed") {
		t.Errorf("expected unclosed fence error, got %v", d.Errors)
	}
	// Table missing --- separator.
	d = Parse("---\ngridmd: \"0.1\"\n---\n# S\n```{table} T at A1\n| a |\n```\n", "strict")
	if !hasMsg(d.Errors, "`---`-separated") {
		t.Errorf("expected table separator error, got %v", d.Errors)
	}
	// Bad pipe row inside grid.
	d = Parse("---\ngridmd: \"0.1\"\n---\n# S\n```{grid} A1\nnot a row\n```\n", "strict")
	if !hasMsg(d.Errors, "expected a pipe row") {
		t.Errorf("expected pipe-row error, got %v", d.Errors)
	}
}

func TestParseModes(t *testing.T) {
	src := "---\ngridmd: \"0.1\"\n---\n# S\ngarbage line\n"
	if d := Parse(src, "strict"); len(d.Errors) == 0 {
		t.Error("strict should error on garbage")
	}
	d := Parse(src, "lenient")
	if len(d.Errors) != 0 || len(d.Warnings) == 0 {
		t.Errorf("lenient should warn: errs=%v warns=%v", d.Errors, d.Warnings)
	}
	// Default mode is strict.
	if d := Parse(src, ""); len(d.Errors) == 0 {
		t.Error("default mode should be strict")
	}
}

func TestParseAtPropsOnlyAndScriptNoSeparator(t *testing.T) {
	// Script without a --- separator: whole body is code.
	d := Parse("---\ngridmd: \"0.1\"\n---\n# S\n```{script} S lang=js\ncode only\n```\n", "strict")
	var sb *FenceBlock
	for _, b := range d.Sheets[0].Blocks {
		if fb, ok := b.(*FenceBlock); ok && fb.Kind == "script" {
			sb = fb
		}
	}
	if sb == nil || strings.TrimSpace(sb.Code) != "code only" {
		t.Errorf("script code = %q", sb.Code)
	}
	// A very long unrecognized line is truncated in the message.
	long := strings.Repeat("z", 80)
	d = Parse("---\ngridmd: \"0.1\"\n---\n# S\n"+long+"\n", "strict")
	if len(d.Errors) == 0 || len(d.Errors[0].Msg) > 90 {
		t.Errorf("long line handling: %v", d.Errors)
	}
}

func TestBlockMarkersAndLongBadRow(t *testing.T) {
	(&AtBlock{}).isBlock()
	(&FenceBlock{}).isBlock()
	long := strings.Repeat("q", 70)
	d := Parse("---\ngridmd: \"0.1\"\n---\n# S\n```{grid} A1\n"+long+"\n```\n", "strict")
	if !hasMsg(d.Errors, "expected a pipe row") {
		t.Errorf("expected pipe-row error on long line, got %v", d.Errors)
	}
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func hasMsg(diags []Diag, substr string) bool {
	for _, d := range diags {
		if strings.Contains(d.Msg, substr) {
			return true
		}
	}
	return false
}
