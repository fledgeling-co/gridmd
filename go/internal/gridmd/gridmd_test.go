package gridmd

import (
	"strings"
	"testing"
)

func TestLintCleanAndErrors(t *testing.T) {
	res := Lint("---\ngridmd: \"0.1\"\n---\n# S\n@ A1 1\n")
	if len(res.Errors) != 0 || res.Doc == nil {
		t.Errorf("clean lint errors=%v", res.Errors)
	}
	// Errors are returned line-sorted.
	res = Lint("---\ngridmd: \"0.1\"\n---\n# S\n@ A1 1\n@ A1 2\n@ ?? 3\n")
	if len(res.Errors) < 2 {
		t.Fatalf("expected multiple errors, got %v", res.Errors)
	}
	for i := 1; i < len(res.Errors); i++ {
		if res.Errors[i-1].Line > res.Errors[i].Line {
			t.Error("errors not sorted by line")
		}
	}
}

func TestDump(t *testing.T) {
	out, diags, ok := Dump("---\ngridmd: \"0.1\"\ntitle: X\n---\n# S\n@ A1 1\n")
	if !ok || len(diags) != 0 || !strings.Contains(out, `"title": "X"`) {
		t.Errorf("Dump ok path: ok=%v diags=%v", ok, diags)
	}
	if _, diags, ok := Dump("garbage without frontmatter"); ok || len(diags) == 0 {
		t.Errorf("Dump error path: ok=%v diags=%v", ok, diags)
	}
}
