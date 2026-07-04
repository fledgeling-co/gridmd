// Package conformance runs the three GridMD conformance laws
// (conformance/README.md) against the real shared fixtures. This is the
// acceptance test for the Go implementation.
package conformance

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/lprhodes/grid-md/go/internal/dump"
	"github.com/lprhodes/grid-md/go/internal/gridmd"
	"github.com/lprhodes/grid-md/go/internal/model"
	"github.com/lprhodes/grid-md/go/internal/parser"
	"github.com/lprhodes/grid-md/go/internal/xlsxread"
	"github.com/lprhodes/grid-md/go/internal/xlsxwrite"
)

// repo is the repository root relative to this test's package directory.
const repo = "../../.."

type fixture struct {
	name     string
	gmd      string
	expected string
}

func validFixtures() []fixture {
	return []fixture{
		{"01-cells", repo + "/conformance/fixtures/01-cells.gmd", repo + "/conformance/expected/01-cells.json"},
		{"02-structure", repo + "/conformance/fixtures/02-structure.gmd", repo + "/conformance/expected/02-structure.json"},
		{"03-features", repo + "/conformance/fixtures/03-features.gmd", repo + "/conformance/expected/03-features.json"},
		{"quarterly-report", repo + "/examples/quarterly-report.gmd", repo + "/conformance/expected/quarterly-report.json"},
	}
}

func read(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return string(b)
}

// Law 1: parse + dump is byte-identical to the recorded expectation.
func TestLaw1DumpMatches(t *testing.T) {
	for _, f := range validFixtures() {
		out, diags, ok := gridmd.Dump(read(t, f.gmd))
		if !ok {
			t.Fatalf("%s: unexpected errors: %v", f.name, diags)
		}
		if want := read(t, f.expected); out != want {
			t.Errorf("%s: dump mismatch", f.name)
		}
	}
}

// Law 2: every invalid fixture is rejected.
func TestLaw2RejectInvalid(t *testing.T) {
	dir := repo + "/conformance/invalid"
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	seen := 0
	for _, e := range entries {
		if filepath.Ext(e.Name()) != ".gmd" {
			continue
		}
		seen++
		if _, _, ok := gridmd.Dump(read(t, filepath.Join(dir, e.Name()))); ok {
			t.Errorf("%s: expected rejection", e.Name())
		}
	}
	if seen == 0 {
		t.Fatal("no invalid fixtures found")
	}
}

// Law 3: dump(import(export(doc))) == dump(doc) for every valid fixture.
func TestLaw3RoundTrip(t *testing.T) {
	for _, f := range validFixtures() {
		source := read(t, f.gmd)
		wb := model.Build(parser.Parse(source, "strict"))
		xlsx, _, err := xlsxwrite.Write(wb, []byte(source))
		if err != nil {
			t.Fatalf("%s: export: %v", f.name, err)
		}
		reimported, _, err := xlsxread.Read(xlsx)
		if err != nil {
			t.Fatalf("%s: import: %v", f.name, err)
		}
		got, _, ok := gridmd.Dump(reimported)
		if !ok {
			t.Fatalf("%s: reimported doc did not lint clean", f.name)
		}
		orig := dump.Model(wb)
		if got != orig {
			t.Errorf("%s: round-trip dump mismatch", f.name)
		}
	}
}
