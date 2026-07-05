package xlsxread

import (
	"archive/zip"
	"bytes"
	"encoding/binary"
	"testing"

	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/parser"
	"github.com/fledgling-co/gridmd/go/internal/xlsxwrite"
)

func zipWith(name, content string) []byte {
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, _ := zw.Create(name)
	w.Write([]byte(content))
	zw.Close()
	return buf.Bytes()
}

// storeZip writes content as a STORE (uncompressed) entry so tests can corrupt
// specific bytes to exercise the entry read/open failure paths.
func storeZip(name, content string) []byte {
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, _ := zw.CreateHeader(&zip.FileHeader{Name: name, Method: zip.Store})
	w.Write([]byte(content))
	zw.Close()
	return buf.Bytes()
}

func TestReadRoundTrip(t *testing.T) {
	src := "---\ngridmd: \"1.0\"\n---\n# S\n@ A1 5\n"
	wb := model.Build(parser.Parse(src, "strict"))
	data, _, err := xlsxwrite.Write(wb, []byte(src))
	if err != nil {
		t.Fatal(err)
	}
	got, report, err := Read(data)
	if err != nil {
		t.Fatal(err)
	}
	if got != src {
		t.Errorf("round-trip source mismatch: %q", got)
	}
	if len(report) == 0 {
		t.Error("empty report")
	}
}

func TestReadErrors(t *testing.T) {
	if _, _, err := Read([]byte("not a zip")); err == nil {
		t.Error("expected invalid-zip error")
	}
	if _, _, err := Read(zipWith("other.xml", "x")); err == nil {
		t.Error("expected missing-carry error")
	}
	if _, _, err := Read(zipWith(xlsxwrite.CarryPart, `<gridmd><source encoding="hex">aa</source></gridmd>`)); err == nil {
		t.Error("expected unsupported-encoding error")
	}
	if _, _, err := Read(zipWith(xlsxwrite.CarryPart, `<gridmd><source encoding="base64">!!!not base64!!!</source></gridmd>`)); err == nil {
		t.Error("expected corrupt-base64 error")
	}
	if _, _, err := Read(zipWith(xlsxwrite.CarryPart, `<gridmd><source`)); err == nil {
		t.Error("expected malformed-xml error")
	}

	// Corrupt the STORE entry content: header opens, read fails on the checksum.
	marker := "MARKERDATA"
	data := storeZip(xlsxwrite.CarryPart, "<gridmd/>"+marker)
	idx := bytes.Index(data, []byte(marker))
	data[idx] = '#'
	if _, _, err := Read(data); err == nil {
		t.Error("expected a checksum read error")
	}

	// Patch the compression method to an unsupported value: Open() fails.
	m := storeZip(xlsxwrite.CarryPart, "<gridmd/>")
	binary.LittleEndian.PutUint16(m[8:], 99) // local header method
	ci := bytes.Index(m, []byte("PK\x01\x02"))
	binary.LittleEndian.PutUint16(m[ci+10:], 99) // central directory method
	if _, _, err := Read(m); err == nil {
		t.Error("expected an unsupported-algorithm error")
	}
}

func TestTrimSpace(t *testing.T) {
	if trimSpace(" a\n b\t c\r ") != "abc" {
		t.Errorf("trimSpace = %q", trimSpace(" a\n b\t c\r "))
	}
}
