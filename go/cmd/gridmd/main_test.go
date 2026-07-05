package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/parser"
	"github.com/fledgling-co/gridmd/go/internal/xlsxwrite"
)

// buildXlsxCarrying returns an .xlsx whose carry part holds carrySource (which
// may be intentionally invalid GridMD), using a valid seed only for the model.
func buildXlsxCarrying(t *testing.T, seedGmd, carrySource string) []byte {
	t.Helper()
	seed, err := os.ReadFile(seedGmd)
	if err != nil {
		t.Fatal(err)
	}
	wb := model.Build(parser.Parse(string(seed), "strict"))
	data, _, err := xlsxwrite.Write(wb, []byte(carrySource))
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func run(t *testing.T, args ...string) (int, string, string) {
	t.Helper()
	var out, errb bytes.Buffer
	code := Run(args, &out, &errb)
	return code, out.String(), errb.String()
}

func write(t *testing.T, dir, name, content string) string {
	t.Helper()
	p := filepath.Join(dir, name)
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

const doc = "---\ngridmd: \"1.0\"\n---\n# S\n@ A1 5\n@ B1 =A1*2 :: 10\n"

func TestParseArgs(t *testing.T) {
	p := parseArgs([]string{"file.gmd", "-o", "out.xlsx", "--strict", "--unknown", "extra"})
	if len(p.files) != 2 || p.files[0] != "file.gmd" || p.files[1] != "extra" {
		t.Errorf("files = %v", p.files)
	}
	if p.output != "out.xlsx" || !p.strict {
		t.Errorf("output=%q strict=%v", p.output, p.strict)
	}
	if q := parseArgs([]string{"a", "--output", "b"}); q.output != "b" {
		t.Errorf("--output = %q", q.output)
	}
	// trailing -o with no value is ignored.
	if q := parseArgs([]string{"a", "-o"}); q.output != "" {
		t.Errorf("dangling -o = %q", q.output)
	}
}

func TestRunUsageAndUnknown(t *testing.T) {
	if code, _, _ := run(t); code != 2 {
		t.Errorf("no args exit = %d", code)
	}
	if code, _, e := run(t, "bogus"); code != 2 || !strings.Contains(e, "unknown command") {
		t.Errorf("unknown verb: code=%d err=%q", code, e)
	}
	for _, verb := range []string{"dump", "to-xlsx", "from-xlsx"} {
		if code, _, _ := run(t, verb); code != 2 {
			t.Errorf("%s no file exit = %d", verb, code)
		}
	}
	// unreadable inputs exit 2.
	if code, _, _ := run(t, "dump", "/nope/x.gmd"); code != 2 {
		t.Errorf("dump missing file exit = %d", code)
	}
	if code, _, _ := run(t, "to-xlsx", "/nope/x.gmd"); code != 2 {
		t.Errorf("to-xlsx missing file exit = %d", code)
	}
	if code, _, _ := run(t, "from-xlsx", "/nope/x.xlsx"); code != 2 {
		t.Errorf("from-xlsx missing file exit = %d", code)
	}
}

func TestRunDump(t *testing.T) {
	dir := t.TempDir()
	good := write(t, dir, "good.gmd", doc)
	code, out, _ := run(t, "dump", good)
	if code != 0 || !strings.Contains(out, `"gridmd": "1.0"`) {
		t.Errorf("dump good: code=%d", code)
	}
	bad := write(t, dir, "bad.gmd", "no frontmatter")
	if code, _, e := run(t, "dump", bad); code != 1 || !strings.Contains(e, "error:") {
		t.Errorf("dump bad: code=%d err=%q", code, e)
	}
}

func TestRunRoundTripCLI(t *testing.T) {
	dir := t.TempDir()
	gmd := write(t, dir, "in.gmd", doc)
	xlsx := filepath.Join(dir, "out.xlsx")
	// export with explicit -o
	if code, out, _ := run(t, "to-xlsx", gmd, "-o", xlsx); code != 0 || !strings.Contains(out, "written") {
		t.Fatalf("to-xlsx: code=%d out=%q", code, out)
	}
	// import with explicit -o and self-check clean
	back := filepath.Join(dir, "back.gmd")
	if code, out, _ := run(t, "from-xlsx", xlsx, "-o", back); code != 0 || !strings.Contains(out, "self-check clean") {
		t.Fatalf("from-xlsx: code=%d out=%q", code, out)
	}
	// dumps match
	_, origDump, _ := run(t, "dump", gmd)
	_, rtDump, _ := run(t, "dump", back)
	if origDump != rtDump || origDump == "" {
		t.Error("round-trip dump mismatch via CLI")
	}
	// default output names (no -o)
	if code, _, _ := run(t, "to-xlsx", gmd); code != 0 {
		t.Errorf("to-xlsx default out exit = %d", code)
	}
	if _, err := os.Stat(filepath.Join(dir, "in.xlsx")); err != nil {
		t.Errorf("default xlsx not written: %v", err)
	}
}

func TestRunOutputWriteErrors(t *testing.T) {
	dir := t.TempDir()
	gmd := write(t, dir, "in.gmd", doc)
	badOut := filepath.Join(dir, "nope-subdir", "out.xlsx")
	if code, _, _ := run(t, "to-xlsx", gmd, "-o", badOut); code != 1 {
		t.Errorf("to-xlsx bad output path exit = %d", code)
	}
	xlsx := filepath.Join(dir, "ok.xlsx")
	run(t, "to-xlsx", gmd, "-o", xlsx)
	badGmd := filepath.Join(dir, "nope-subdir", "out.gmd")
	if code, _, _ := run(t, "from-xlsx", xlsx, "-o", badGmd); code != 1 {
		t.Errorf("from-xlsx bad output path exit = %d", code)
	}
}

func TestRunToXlsxLintError(t *testing.T) {
	dir := t.TempDir()
	bad := write(t, dir, "bad.gmd", "---\ntitle: no gridmd\n---\n# S\n@ A1 1\n@ A1 2\n")
	if code, _, e := run(t, "to-xlsx", bad, "-o", filepath.Join(dir, "x.xlsx")); code != 1 || !strings.Contains(e, "fix the document") {
		t.Errorf("to-xlsx lint error: code=%d err=%q", code, e)
	}
}

func TestRunFromXlsxErrors(t *testing.T) {
	dir := t.TempDir()
	// Not a GridMD-carrying package.
	notXlsx := write(t, dir, "x.xlsx", "not a zip")
	if code, _, _ := run(t, "from-xlsx", notXlsx); code != 1 {
		t.Errorf("from-xlsx non-xlsx exit = %d", code)
	}

	// An .xlsx carrying invalid GridMD fails the self-check (exit 1) and uses
	// the default .gmd output name.
	dir2 := t.TempDir()
	gmd := write(t, dir2, "seed.gmd", doc)
	badXlsx := buildXlsxCarrying(t, gmd, "no frontmatter here")
	badPath := write(t, dir2, "carry.xlsx", string(badXlsx))
	code, out, errb := run(t, "from-xlsx", badPath)
	if code != 1 || !strings.Contains(out, "self-check FAILED") || !strings.Contains(errb, "self-check:") {
		t.Errorf("from-xlsx bad carry: code=%d out=%q err=%q", code, out, errb)
	}
	// default .gmd output was written next to the input.
	if _, err := os.Stat(filepath.Join(dir2, "carry.gmd")); err != nil {
		t.Errorf("default gmd not written: %v", err)
	}
}
