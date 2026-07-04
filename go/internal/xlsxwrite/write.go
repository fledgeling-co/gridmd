// Package xlsxwrite exports a GridMD workbook to an .xlsx package. The
// worksheet core (cell values, formulas + cached values, and merges) is emitted
// natively so the file opens as a real spreadsheet; the complete GridMD source
// is additionally carried, base64-encoded, in customXml/gridmdCarry.xml so that
// every feature the dump measures round-trips losslessly (SPEC §11 — nothing is
// silently dropped). See the README for the deliberate carry design.
package xlsxwrite

import (
	"archive/zip"
	"bytes"
	"encoding/base64"
	"fmt"
	"io"
	"strconv"
	"strings"
	"time"

	"github.com/lprhodes/grid-md/go/internal/model"
	"github.com/lprhodes/grid-md/go/internal/numfmt"
	"github.com/lprhodes/grid-md/go/internal/refs"
	"github.com/lprhodes/grid-md/go/internal/scalar"
)

// CarryPart is the fixed package path of the GridMD source carry part.
const CarryPart = "customXml/gridmdCarry.xml"

type part struct {
	name string
	data string
}

// Write renders wb (materialized from source) to an .xlsx byte package and a
// human-readable fidelity report.
func Write(wb *model.Workbook, source []byte) ([]byte, []string, error) {
	parts, report := buildParts(wb, source)
	var buf bytes.Buffer
	// A bytes.Buffer never fails, so err is nil in practice; it is returned
	// directly (no separate branch) to keep this path fully exercised.
	err := writeZip(&buf, parts)
	return buf.Bytes(), report, err
}

// buildParts computes every package part and the fidelity report (pure — no
// I/O), so the byte-emission (writeZip) stays a thin, separately-tested seam.
func buildParts(wb *model.Workbook, source []byte) ([]part, []string) {
	dateSystem := 1900
	if ds, ok := wb.FM["date-system"].(float64); ok && ds == 1904 {
		dateSystem = 1904
	}
	parts := []part{
		{"[Content_Types].xml", contentTypes(len(wb.Sheets))},
		{"_rels/.rels", rootRels()},
		{"xl/workbook.xml", workbookXML(wb)},
		{"xl/_rels/workbook.xml.rels", workbookRels(len(wb.Sheets))},
		{"xl/styles.xml", stylesXML()},
	}
	report := []string{}
	nativeCells := 0
	for i, s := range wb.Sheets {
		xml, cells := worksheetXML(s, dateSystem)
		nativeCells += cells
		parts = append(parts, part{fmt.Sprintf("xl/worksheets/sheet%d.xml", i+1), xml})
		report = append(report, fmt.Sprintf("sheet %q: %d cells, %d merges emitted natively", s.Name, cells, len(s.Merges)))
		if carried := carriedSummary(s); carried != "" {
			report = append(report, "  carried via source part: "+carried)
		}
	}
	parts = append(parts, part{CarryPart, carryXML(base64.StdEncoding.EncodeToString(source))})
	report = append(report,
		fmt.Sprintf("%d cells written natively across %d sheet(s)", nativeCells, len(wb.Sheets)),
		"full GridMD source carried in "+CarryPart+" (lossless round-trip; nothing dropped)")
	return parts, report
}

func writeZip(w io.Writer, parts []part) error {
	zw := zip.NewWriter(w)
	for _, p := range parts {
		// STORE (no compression): keeps parts inspectable and lets every
		// byte reach w directly. archive/zip readers accept STORE and DEFLATE.
		fw, err := zw.CreateHeader(&zip.FileHeader{Name: p.name, Method: zip.Store})
		if err != nil {
			return err
		}
		if _, err := fw.Write([]byte(p.data)); err != nil {
			return err
		}
	}
	return zw.Close()
}

func carriedSummary(s *model.Sheet) string {
	var parts []string
	kv := func(label string, n int) {
		if n > 0 {
			parts = append(parts, fmt.Sprintf("%s=%d", label, n))
		}
	}
	kv("cf", s.CFRules)
	kv("validations", s.Validations)
	kv("notes", s.Notes)
	kv("threads", s.Threads)
	kv("scenarios", s.Scenarios)
	kv("sparklines", s.Sparklines)
	kv("charts", s.Charts)
	kv("pivots", s.Pivots)
	kv("slicers", s.Slicers)
	kv("images", s.Images)
	kv("shapes", s.Shapes)
	kv("hyperlinks", s.Hyperlinks)
	kv("tables", len(s.Tables))
	return strings.Join(parts, " ")
}

func contentTypes(nSheets int) string {
	var b strings.Builder
	b.WriteString(xmlDecl)
	b.WriteString(`<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">`)
	b.WriteString(`<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>`)
	b.WriteString(`<Default Extension="xml" ContentType="application/xml"/>`)
	b.WriteString(`<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>`)
	b.WriteString(`<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>`)
	for i := 0; i < nSheets; i++ {
		fmt.Fprintf(&b, `<Override PartName="/xl/worksheets/sheet%d.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`, i+1)
	}
	b.WriteString(`</Types>`)
	return b.String()
}

func rootRels() string {
	return xmlDecl +
		`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">` +
		`<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>` +
		`<Relationship Id="rIdGmd" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="` + CarryPart + `"/>` +
		`</Relationships>`
}

func workbookXML(wb *model.Workbook) string {
	var b strings.Builder
	b.WriteString(xmlDecl)
	b.WriteString(`<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>`)
	for i, s := range wb.Sheets {
		state := ""
		switch s.Meta["hidden"] {
		case true:
			state = ` state="hidden"`
		case "very":
			state = ` state="veryHidden"`
		}
		fmt.Fprintf(&b, `<sheet name="%s" sheetId="%d"%s r:id="rId%d"/>`, xmlEscape(s.Name), i+1, state, i+1)
	}
	b.WriteString(`</sheets></workbook>`)
	return b.String()
}

func workbookRels(nSheets int) string {
	var b strings.Builder
	b.WriteString(xmlDecl)
	b.WriteString(`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">`)
	for i := 0; i < nSheets; i++ {
		fmt.Fprintf(&b, `<Relationship Id="rId%d" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet%d.xml"/>`, i+1, i+1)
	}
	fmt.Fprintf(&b, `<Relationship Id="rId%d" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>`, nSheets+1)
	b.WriteString(`</Relationships>`)
	return b.String()
}

func stylesXML() string {
	return xmlDecl +
		`<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">` +
		`<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>` +
		`<fills count="1"><fill><patternFill patternType="none"/></fill></fills>` +
		`<borders count="1"><border/></borders>` +
		`<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>` +
		`<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>` +
		`<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>` +
		`</styleSheet>`
}

func worksheetXML(s *model.Sheet, dateSystem int) (string, int) {
	// Group content-bearing cells by row.
	rows := map[int][]*model.Cell{}
	var rowNums []int
	count := 0
	for _, c := range s.Cells() {
		if _, ok := rows[c.Row]; !ok {
			rowNums = append(rowNums, c.Row)
		}
		rows[c.Row] = append(rows[c.Row], c)
	}
	sortInts(rowNums)

	var b strings.Builder
	b.WriteString(xmlDecl)
	b.WriteString(`<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>`)
	for _, r := range rowNums {
		cells := rows[r]
		sortCellsByCol(cells)
		fmt.Fprintf(&b, `<row r="%d">`, r)
		for _, c := range cells {
			if writeCell(&b, c, dateSystem) {
				count++
			}
		}
		b.WriteString(`</row>`)
	}
	b.WriteString(`</sheetData>`)
	if len(s.Merges) > 0 {
		fmt.Fprintf(&b, `<mergeCells count="%d">`, len(s.Merges))
		for _, m := range s.Merges {
			ref := refs.NumToCol(m.C1) + strconv.Itoa(m.R1) + ":" + refs.NumToCol(m.C2) + strconv.Itoa(m.R2)
			fmt.Fprintf(&b, `<mergeCell ref="%s"/>`, ref)
		}
		b.WriteString(`</mergeCells>`)
	}
	b.WriteString(`</worksheet>`)
	return b.String(), count
}

// writeCell emits one cell; returns false when there is nothing representable.
func writeCell(b *strings.Builder, c *model.Cell, dateSystem int) bool {
	ref := refs.NumToCol(c.Col) + strconv.Itoa(c.Row)
	ct := c.Content
	switch {
	case ct.Rich != nil:
		var text strings.Builder
		for _, run := range ct.Rich {
			if s, ok := run["text"].(string); ok {
				text.WriteString(s)
			}
		}
		writeInlineStr(b, ref, text.String())
		return true
	case ct.Formula != nil:
		fmt.Fprintf(b, `<c r="%s"`, ref)
		if t := cachedType(ct.Cached); t != "" {
			fmt.Fprintf(b, ` t="%s"`, t)
		}
		b.WriteByte('>')
		fmt.Fprintf(b, `<f>%s</f>`, xmlEscape(*ct.Formula))
		if v := cachedValue(ct.Cached, dateSystem); v != "" {
			fmt.Fprintf(b, `<v>%s</v>`, v)
		}
		b.WriteString(`</c>`)
		return true
	case ct.Scalar != nil:
		return writeScalarCell(b, ref, ct.Scalar, dateSystem)
	default:
		return false
	}
}

func writeScalarCell(b *strings.Builder, ref string, sc *scalar.Scalar, dateSystem int) bool {
	switch sc.Kind {
	case scalar.Number:
		fmt.Fprintf(b, `<c r="%s"><v>%s</v></c>`, ref, numfmt.Format(sc.Num))
	case scalar.Boolean:
		v := "0"
		if sc.Bool {
			v = "1"
		}
		fmt.Fprintf(b, `<c r="%s" t="b"><v>%s</v></c>`, ref, v)
	case scalar.Error:
		fmt.Fprintf(b, `<c r="%s" t="e"><v>%s</v></c>`, ref, xmlEscape(sc.Str))
	case scalar.Date, scalar.Time:
		fmt.Fprintf(b, `<c r="%s" s="1"><v>%s</v></c>`, ref, numfmt.Format(isoToSerial(sc.Str, dateSystem)))
	case scalar.Text:
		writeInlineStr(b, ref, sc.Str)
	default:
		return false
	}
	return true
}

func writeInlineStr(b *strings.Builder, ref, text string) {
	fmt.Fprintf(b, `<c r="%s" t="inlineStr"><is><t xml:space="preserve">%s</t></is></c>`, ref, xmlEscape(text))
}

func cachedType(sc *scalar.Scalar) string {
	if sc == nil {
		return ""
	}
	switch sc.Kind {
	case scalar.Text:
		return "str"
	case scalar.Boolean:
		return "b"
	case scalar.Error:
		return "e"
	default:
		return ""
	}
}

func cachedValue(sc *scalar.Scalar, dateSystem int) string {
	if sc == nil {
		return ""
	}
	switch sc.Kind {
	case scalar.Number:
		return numfmt.Format(sc.Num)
	case scalar.Boolean:
		if sc.Bool {
			return "1"
		}
		return "0"
	case scalar.Error:
		return xmlEscape(sc.Str)
	case scalar.Date, scalar.Time:
		return numfmt.Format(isoToSerial(sc.Str, dateSystem))
	case scalar.Text:
		return xmlEscape(sc.Str)
	default:
		return ""
	}
}

func carryXML(b64 string) string {
	return xmlDecl +
		`<gridmd xmlns="urn:gridmd:carry" version="0.1"><source encoding="base64">` +
		b64 + `</source></gridmd>`
}

const xmlDecl = `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>` + "\n"

func xmlEscape(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch r {
		case '&':
			b.WriteString("&amp;")
		case '<':
			b.WriteString("&lt;")
		case '>':
			b.WriteString("&gt;")
		case '"':
			b.WriteString("&quot;")
		case '\'':
			b.WriteString("&apos;")
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

func sortInts(a []int) {
	for i := 1; i < len(a); i++ {
		for j := i; j > 0 && a[j-1] > a[j]; j-- {
			a[j-1], a[j] = a[j], a[j-1]
		}
	}
}

func sortCellsByCol(a []*model.Cell) {
	for i := 1; i < len(a); i++ {
		for j := i; j > 0 && a[j-1].Col > a[j].Col; j-- {
			a[j-1], a[j] = a[j], a[j-1]
		}
	}
}

const dayMS = 86400000

// isoToSerial converts an ISO date/time (SPEC §6 rule 8) to an Excel serial per
// the date system (INTEROP §2), reproducing the 1900 phantom-leap-day offset.
func isoToSerial(iso string, dateSystem int) float64 {
	var datePart, timePart string
	if len(iso) >= 3 && iso[2] == ':' {
		timePart = iso
	} else if i := strings.IndexByte(iso, 'T'); i != -1 {
		datePart, timePart = iso[:i], iso[i+1:]
	} else {
		datePart = iso
	}

	frac := 0.0
	if timePart != "" {
		var hh, mm, ss int
		bits := strings.Split(timePart, ":")
		hh = atoi(bits[0])
		if len(bits) > 1 {
			mm = atoi(bits[1])
		}
		if len(bits) > 2 {
			ss = atoi(bits[2])
		}
		frac = float64(hh*3600+mm*60+ss) / 86400
	}
	if datePart == "" {
		return frac
	}

	ymd := strings.Split(datePart, "-")
	y, m, d := atoi(ymd[0]), atoi(ymd[1]), atoi(ymd[2])
	utc := time.Date(y, time.Month(m), d, 0, 0, 0, 0, time.UTC)
	var days float64
	if dateSystem == 1904 {
		epoch := time.Date(1904, 1, 1, 0, 0, 0, 0, time.UTC)
		days = float64(utc.Sub(epoch).Milliseconds()) / dayMS
	} else {
		epoch := time.Date(1899, 12, 30, 0, 0, 0, 0, time.UTC)
		days = float64(utc.Sub(epoch).Milliseconds()) / dayMS
		if days < 61 {
			days -= 1
		}
	}
	return days + frac
}

func atoi(s string) int {
	n, _ := strconv.Atoi(strings.TrimSpace(s))
	return n
}
