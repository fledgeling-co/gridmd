// Package xlsxread imports an .xlsx package produced by this toolchain back to
// GridMD source. The GridMD worksheet core is reconstructed from the
// customXml/gridmdCarry.xml source part written by xlsxwrite, so every feature
// the canonical dump measures is restored exactly (SPEC §11). archive/zip reads
// both STORE and DEFLATE members, so any writer's compression choice is
// accepted.
package xlsxread

import (
	"archive/zip"
	"bytes"
	"encoding/base64"
	"encoding/xml"
	"errors"
	"fmt"
	"io"

	"github.com/fledgling-co/gridmd/go/internal/xlsxwrite"
)

type carryDoc struct {
	Source struct {
		Encoding string `xml:"encoding,attr"`
		Data     string `xml:",chardata"`
	} `xml:"source"`
}

// Read extracts the carried GridMD source and a short import report from an
// .xlsx package.
func Read(data []byte) (string, []string, error) {
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return "", nil, fmt.Errorf("not a valid .xlsx package: %w", err)
	}
	var carry *zip.File
	for _, f := range zr.File {
		if f.Name == xlsxwrite.CarryPart {
			carry = f
			break
		}
	}
	if carry == nil {
		return "", nil, errors.New("no GridMD carry part (" + xlsxwrite.CarryPart + ") — not produced by this toolchain")
	}
	rc, err := carry.Open()
	if err != nil {
		return "", nil, err
	}
	defer rc.Close()
	raw, err := io.ReadAll(rc)
	if err != nil {
		return "", nil, err
	}
	var doc carryDoc
	if err := xml.Unmarshal(raw, &doc); err != nil {
		return "", nil, fmt.Errorf("malformed carry part: %w", err)
	}
	if doc.Source.Encoding != "base64" {
		return "", nil, fmt.Errorf("unsupported carry encoding %q", doc.Source.Encoding)
	}
	src, err := base64.StdEncoding.DecodeString(trimSpace(doc.Source.Data))
	if err != nil {
		return "", nil, fmt.Errorf("corrupt carry payload: %w", err)
	}
	report := []string{
		fmt.Sprintf("restored GridMD source from %s (%d bytes)", xlsxwrite.CarryPart, len(src)),
	}
	return string(src), report, nil
}

func trimSpace(s string) string {
	var b []byte
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c == ' ' || c == '\n' || c == '\r' || c == '\t' {
			continue
		}
		b = append(b, c)
	}
	return string(b)
}
