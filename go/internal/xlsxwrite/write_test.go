package xlsxwrite

import (
	"archive/zip"
	"bytes"
	"encoding/base64"
	"errors"
	"io"
	"regexp"
	"strings"
	"testing"

	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/parser"
	"github.com/fledgling-co/gridmd/go/internal/scalar"
)

const src = `---
gridmd: "1.0"
date-system: 1904
---
# S
` + "```{sheet}\nhidden: very\n```" + `
@ A1 5
@ A2 TRUE
@ A3 #N/A
@ A4 2026-07-04
@ A5 12:30
@ A6 "hi"
@ A7
  rich:
    - { text: "r1" }
@ B1 =X :: "s"
@ B2 =X :: 9
@ B3 =X :: TRUE
@ B4 =X :: #N/A
@ B5 =X :: 2026-07-04
@ B6 =X
@ C1:D1 { merge: true }
` + "```{chart} column \"c\" at C5\nseries:\n  - { val: A1 }\n```" + `
# H
` + "```{sheet}\nhidden: true\n```" + `
@ A1 1
`

func TestWriteIntegration(t *testing.T) {
	source := []byte(src)
	wb := model.Build(parser.Parse(src, "strict"))
	data, report, err := Write(wb, source)
	if err != nil {
		t.Fatal(err)
	}
	if len(report) == 0 || len(data) == 0 {
		t.Fatal("empty output")
	}
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		t.Fatal(err)
	}
	files := map[string]string{}
	for _, f := range zr.File {
		rc, _ := f.Open()
		b, _ := io.ReadAll(rc)
		rc.Close()
		files[f.Name] = string(b)
	}
	for _, name := range []string{"[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels", "xl/styles.xml", "xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml", CarryPart} {
		if _, ok := files[name]; !ok {
			t.Errorf("missing part %s", name)
		}
	}
	ws := files["xl/worksheets/sheet1.xml"]
	for _, frag := range []string{`t="b"`, `t="e"`, `s="1"`, `t="inlineStr"`, `<f>`, `t="str"`, `<mergeCells count="1">`} {
		if !strings.Contains(ws, frag) {
			t.Errorf("worksheet missing %q", frag)
		}
	}
	wbxml := files["xl/workbook.xml"]
	if !strings.Contains(wbxml, `state="veryHidden"`) || !strings.Contains(wbxml, `state="hidden"`) {
		t.Errorf("workbook hidden states missing: %s", wbxml)
	}
	// The carry part decodes back to the exact source.
	m := regexp.MustCompile(`(?s)<source encoding="base64">(.*?)</source>`).FindStringSubmatch(files[CarryPart])
	dec, err := base64.StdEncoding.DecodeString(strings.TrimSpace(m[1]))
	if err != nil || !bytes.Equal(dec, source) {
		t.Errorf("carry mismatch: err=%v equal=%v", err, bytes.Equal(dec, source))
	}
}

func TestWriteCellFallbacks(t *testing.T) {
	var b strings.Builder
	// Content with no representable value (spill-cache-anchor style).
	if writeCell(&b, &model.Cell{Col: 1, Row: 1, Content: &model.Content{}}, 1900) {
		t.Error("empty content should not emit")
	}
	// Scalar of an unrepresentable kind.
	if writeScalarCell(&b, "A1", &scalar.Scalar{Kind: scalar.Blank}, 1900) {
		t.Error("blank scalar should not emit")
	}
	if b.Len() != 0 {
		t.Errorf("nothing should have been written, got %q", b.String())
	}
}

func TestCachedTypeAndValue(t *testing.T) {
	if cachedType(nil) != "" || cachedValue(nil, 1900) != "" {
		t.Error("nil cached")
	}
	if cachedType(&scalar.Scalar{Kind: scalar.Text}) != "str" ||
		cachedType(&scalar.Scalar{Kind: scalar.Boolean}) != "b" ||
		cachedType(&scalar.Scalar{Kind: scalar.Error}) != "e" ||
		cachedType(&scalar.Scalar{Kind: scalar.Number}) != "" {
		t.Error("cachedType")
	}
	cases := []struct {
		s    *scalar.Scalar
		want string
	}{
		{&scalar.Scalar{Kind: scalar.Number, Num: 3}, "3"},
		{&scalar.Scalar{Kind: scalar.Boolean, Bool: true}, "1"},
		{&scalar.Scalar{Kind: scalar.Boolean, Bool: false}, "0"},
		{&scalar.Scalar{Kind: scalar.Error, Str: "#N/A"}, "#N/A"},
		{&scalar.Scalar{Kind: scalar.Date, Str: "2026-07-04"}, "46207"},
		{&scalar.Scalar{Kind: scalar.Text, Str: "x"}, "x"},
		{&scalar.Scalar{Kind: scalar.Blank}, ""},
	}
	for _, c := range cases {
		if got := cachedValue(c.s, 1900); got != c.want {
			t.Errorf("cachedValue(%+v) = %q, want %q", c.s, got, c.want)
		}
	}
}

func TestIsoToSerial(t *testing.T) {
	cases := []struct {
		iso  string
		sys  int
		want float64
	}{
		{"1900-01-01", 1900, 1},  // phantom-leap offset applied
		{"1900-03-01", 1900, 61}, // first date past the phantom leap day
		{"2026-07-04", 1900, 46207},
		{"1904-01-01", 1904, 0},
		{"06:00", 1900, 0.25},
		{"2026-07-04T06:00:00", 1900, 46207.25},
	}
	for _, c := range cases {
		if got := isoToSerial(c.iso, c.sys); got != c.want {
			t.Errorf("isoToSerial(%q,%d) = %v, want %v", c.iso, c.sys, got, c.want)
		}
	}
}

func TestCarriedSummary(t *testing.T) {
	s := &model.Sheet{CFRules: 2, Hyperlinks: 1}
	if got := carriedSummary(s); !strings.Contains(got, "cf=2") || !strings.Contains(got, "hyperlinks=1") {
		t.Errorf("carriedSummary = %q", got)
	}
	if carriedSummary(&model.Sheet{}) != "" {
		t.Error("empty sheet summary should be blank")
	}
}

func TestWriteZipErrors(t *testing.T) {
	small := []part{{"a.xml", "hello"}}
	// Small archive: the only flush to w is at Close.
	if err := writeZip(&failAfter{after: 0}, small); err == nil {
		t.Error("expected Close flush error")
	}
	// A >4KiB part forces a mid-write bufio flush, so fw.Write reaches w.
	big := []part{{"a.xml", strings.Repeat("x", 9000)}}
	if err := writeZip(&failAfter{after: 0}, big); err == nil {
		t.Error("expected data-write flush error")
	}
	// An over-long entry name makes CreateHeader fail directly.
	longName := []part{{strings.Repeat("n", 70000), "x"}}
	if err := writeZip(&bytes.Buffer{}, longName); err == nil {
		t.Error("expected CreateHeader error for over-long name")
	}
}

func TestXMLEscape(t *testing.T) {
	if got := xmlEscape(`a&b<c>d"e'f`); got != `a&amp;b&lt;c&gt;d&quot;e&apos;f` {
		t.Errorf("xmlEscape = %q", got)
	}
}

func TestSortHelpers(t *testing.T) {
	a := []int{3, 1, 2}
	sortInts(a)
	if a[0] != 1 || a[2] != 3 {
		t.Errorf("sortInts = %v", a)
	}
	cells := []*model.Cell{{Col: 3}, {Col: 1}, {Col: 2}}
	sortCellsByCol(cells)
	if cells[0].Col != 1 || cells[2].Col != 3 {
		t.Errorf("sortCellsByCol = %v", cells)
	}
}

// failAfter succeeds for `after` writes, then errors — lets a test position the
// failure on a specific bufio flush.
type failAfter struct {
	after int
	seen  int
}

func (f *failAfter) Write(b []byte) (int, error) {
	if f.seen >= f.after {
		return 0, errors.New("boom")
	}
	f.seen++
	return len(b), nil
}

var _ io.Writer = &failAfter{}
