package validate

import (
	"strings"
	"testing"

	"github.com/fledgling-co/gridmd/go/internal/parser"
)

func errsFor(src string) []string {
	doc := parser.Parse(src, "strict")
	Validate(doc)
	msgs := make([]string, len(doc.Errors))
	for i, d := range doc.Errors {
		msgs[i] = d.Msg
	}
	return msgs
}

func has(msgs []string, substr string) bool {
	for _, m := range msgs {
		if strings.Contains(m, substr) {
			return true
		}
	}
	return false
}

const head = "---\ngridmd: \"1.0\"\n---\n# S\n"

func wantErr(t *testing.T, src, substr string) {
	t.Helper()
	if !has(errsFor(src), substr) {
		t.Errorf("expected error %q for:\n%s\ngot: %v", substr, src, errsFor(src))
	}
}

func wantClean(t *testing.T, src string) {
	t.Helper()
	if e := errsFor(src); len(e) != 0 {
		t.Errorf("expected no errors, got %v for:\n%s", e, src)
	}
}

func TestFrontmatter(t *testing.T) {
	wantErr(t, "---\ntitle: x\n---\n# S\n", "gridmd")
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - just-a-string\n---\n# S\n", "names entries require a name")
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - { ref: A1 }\n---\n# S\n", "names entries require a name")
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - { name: X }\n---\n# S\n", "exactly one of ref")
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - { name: X, ref: A1, formula: B1 }\n---\n# S\n", "exactly one of ref")
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - { name: X, ref: A1 }\n  - { name: X, ref: B1 }\n---\n# S\n", "duplicate defined name")
	wantClean(t, "---\ngridmd: \"1.0\"\nnames:\n  - { name: X, ref: A1 }\n---\n# S\n")
}

func TestSheets(t *testing.T) {
	wantErr(t, "---\ngridmd: \"1.0\"\n---\n", "at least one sheet")
	wantErr(t, "---\ngridmd: \"1.0\"\n---\n# "+strings.Repeat("x", 32)+"\n", "exceeds 31 chars")
	wantErr(t, "---\ngridmd: \"1.0\"\n---\n# a/b\n", "forbidden character")
	wantErr(t, "---\ngridmd: \"1.0\"\n---\n# S\n# S\n", "duplicate sheet name")
}

func TestDefineOnceAndBounds(t *testing.T) {
	wantErr(t, head+"@ A1 1\n@ A1 2\n", "defined more than once")
	wantErr(t, head+"```{grid} XFD1\n| a | b |\n```\n", "cell out of bounds")
}

func TestGrid(t *testing.T) {
	wantErr(t, head+"```{grid}\n| a |\n```\n", "{grid} requires a cell anchor")
	wantErr(t, head+"```{grid} A1\n| \"open |\n```\n", "grid cell:")
	wantClean(t, head+"```{grid} A1\n| a | 1 |\n```\n")
}

func TestTable(t *testing.T) {
	wantErr(t, head+"```{table} A1 at B1\n---\n| h |\n| v |\n```\n", "requires a valid table name")
	wantErr(t, head+"```{table} T\n---\n| h |\n| v |\n```\n", "requires `at <cell>`")
	wantErr(t, head+"```{table} T at A1\n---\n```\n", "requires payload rows")
	wantErr(t, head+"```{table} T at A1\n---\n| 5 |\n| v |\n```\n", "non-empty text")
	wantErr(t, head+"```{table} T at A1\n---\n| a | a |\n| 1 | 2 |\n```\n", "duplicate table column name")
	wantErr(t, head+"```{table} T at A1\n---\n| h |\n| \"open |\n```\n", "table cell:")
	// name collision with a defined name
	wantErr(t, "---\ngridmd: \"1.0\"\nnames:\n  - { name: T, ref: A1 }\n---\n# S\n```{table} T at C1\n---\n| h |\n| v |\n```\n", "collides with an existing name")
	// total cell addDef + unknown-column total (skipped) both clean
	wantClean(t, head+"```{table} T at A1\ntotal:\n  h: =SUM([h])\n  nope: 0\n---\n| h |\n| 1 |\n```\n")
}

func TestValidateAt(t *testing.T) {
	wantErr(t, head+"@ ?? 1\n", "invalid @ target")
	wantErr(t, head+"@ Other!A1 1\n", "must name the containing sheet")
	wantErr(t, head+"@ A1 \"open\n", "scalar:")
	wantErr(t, head+"@ A1 =B1 :: =C1\n", "cached value must not be a formula")
	wantErr(t, head+"@ A1 5\n  value: 6\n", "inline content and body content keys")
	wantErr(t, head+"@ A1:C1 5\n", "range targets accept formula content only")
	wantErr(t, head+"@ A1 { merge: true }\n", "merge: requires a range target")
	wantErr(t, head+"@ A1:B1 { merge: false }\n", "only `true` is valid")
	wantErr(t, head+"@ A1 =B1 { spill: X }\n", "spill: must be a range")
	wantErr(t, head+"@ A1 =B1 { spill: B2:B4 }\n", "must start at the anchor cell")
	wantErr(t, head+"@ A1 =B1 { spill: true }\n", "spill: must be a range")
	wantErr(t, head+"@ A1 =B1 { spill: 5 }\n", "spill: must be a range")
	// clean: cached-only body value with an inline formula, and a valid spill.
	wantClean(t, head+"@ A1 =SUM(B1:B2)\n  value: 5\n")
	wantClean(t, head+"@ A1 =B1 { spill: A1:A3 }\n```{spill-cache} A1\n| x |\n```\n")
	// clean: range fill.
	wantClean(t, head+"@ A1:A3 =B1*2\n")
}

func TestSheetBlockDispatch(t *testing.T) {
	// x- directives are ignored; unknown reserved-less directives error.
	wantClean(t, head+"```{x-thing} foo\npayload\n```\n")
	// a {sheet} meta block is skipped by the block loop.
	wantClean(t, head+"```{sheet}\nfreeze: B2\n```\n@ A1 1\n")
	wantErr(t, head+"```{bogus} foo\nk: v\n```\n", "unknown directive {bogus}")
	// header: false table stores no header columns and stays clean.
	wantClean(t, head+"```{table} T at A1\nheader: false\n---\n| a | 1 |\n| b | 2 |\n```\n")
	// spill value as boolean false exercises asStr's false branch.
	wantErr(t, head+"@ A1 =B1 { spill: false }\n", "spill: must be a range")
}

func TestSpillCache(t *testing.T) {
	wantErr(t, head+"```{spill-cache}\n| 1 |\n```\n", "requires a cell anchor")
	wantErr(t, head+"```{spill-cache} D2\n| 1 |\n```\n", "no owning spill/array formula")
	wantErr(t, head+"@ A1 =B1 { array: A1:A2 }\n```{spill-cache} A1\n| 1 |\n| 2 |\n| 3 |\n```\n", "exceeds the declared spill")
}
