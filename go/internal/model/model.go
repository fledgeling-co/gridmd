// Package model materializes a parsed GridMD document into a per-sheet workbook
// model the dumper and XLSX writer consume: effective cells, merges, tables,
// and per-feature counts. Mirrors js/src/xlsx/model.js for the fields the
// canonical dump measures. Features with no worksheet-core form are carried by
// the writer via the source part (SPEC §11) — never silently dropped.
package model

import (
	"regexp"
	"strconv"
	"strings"

	"github.com/fledgling-co/gridmd/go/internal/numfmt"
	"github.com/fledgling-co/gridmd/go/internal/parser"
	"github.com/fledgling-co/gridmd/go/internal/refs"
	"github.com/fledgling-co/gridmd/go/internal/scalar"
)

var (
	dateRe = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$`)
	timeRe = regexp.MustCompile(`^\d{2}:\d{2}(:\d{2})?$`)
)

// Content is a cell's effective content.
type Content struct {
	Formula   *string
	CSE       bool
	Cached    *scalar.Scalar
	HasCached bool
	ArrayRef  *string
	Rich      []map[string]interface{}
	Scalar    *scalar.Scalar
}

// Cell is a positioned cell (content-bearing cells are dumped).
type Cell struct {
	Col, Row int
	Content  *Content
}

// MergeRange is a merged rectangle.
type MergeRange struct {
	C1, R1, C2, R2 int
}

// Table is a structured table (Excel ListObject).
type Table struct {
	Name      string
	Anchor    refs.Cell
	Columns   []string
	BodyRows  int
	HasTotals bool
}

// Sheet is a materialized worksheet or chart sheet.
type Sheet struct {
	Name        string
	Kind        string
	Meta        map[string]interface{}
	cellOrder   []string
	cells       map[string]*Cell
	Merges      []MergeRange
	Tables      []Table
	CFRules     int
	Validations int
	Notes       int
	Threads     int
	Scenarios   int
	Sparklines  int
	Charts      int
	Pivots      int
	Slicers     int
	Images      int
	Shapes      int
	Hyperlinks  int
}

// Cells returns the sheet cells in insertion order.
func (s *Sheet) Cells() []*Cell {
	out := make([]*Cell, 0, len(s.cellOrder))
	for _, k := range s.cellOrder {
		out = append(out, s.cells[k])
	}
	return out
}

// Workbook is the whole materialized document.
type Workbook struct {
	FM     map[string]interface{}
	Sheets []*Sheet
}

// Build materializes a validated Document into a Workbook.
func Build(doc *parser.Document) *Workbook {
	wb := &Workbook{FM: doc.Frontmatter}
	for _, sheet := range doc.Sheets {
		wb.Sheets = append(wb.Sheets, buildSheet(sheet))
	}
	return wb
}

func buildSheet(sheet *parser.Sheet) *Sheet {
	meta := map[string]interface{}{}
	for _, b := range sheet.Blocks {
		if fb, ok := b.(*parser.FenceBlock); ok && fb.Kind == "sheet" {
			if m, ok := fb.Meta.(map[string]interface{}); ok {
				meta = m
			}
			break
		}
	}
	kind := "worksheet"
	if meta["kind"] == "chart" {
		kind = "chart"
	}
	s := &Sheet{Name: sheet.Name, Kind: kind, Meta: meta, cells: map[string]*Cell{}}

	for _, b := range sheet.Blocks {
		switch blk := b.(type) {
		case *parser.AtBlock:
			s.applyAt(blk)
		case *parser.FenceBlock:
			s.applyFence(blk)
		}
	}
	return s
}

func (s *Sheet) cellAt(col, row int) *Cell {
	k := refs.RefKey(col, row)
	c, ok := s.cells[k]
	if !ok {
		c = &Cell{Col: col, Row: row}
		s.cells[k] = c
		s.cellOrder = append(s.cellOrder, k)
	}
	return c
}

func (s *Sheet) setContent(col, row int, content *Content) {
	c := s.cellAt(col, row)
	if c.Content == nil {
		c.Content = content
		return
	}
	if content.HasCached && c.Content.Formula != nil && c.Content.Cached == nil {
		c.Content.Cached = content.Cached
	}
}

func scalarContent(sc *scalar.Scalar) *Content {
	if sc.Kind == scalar.Formula {
		f := sc.FValue
		return &Content{Formula: &f, CSE: sc.CSE, Cached: sc.Cached, HasCached: true}
	}
	return &Content{Scalar: sc}
}

func (s *Sheet) applyFence(b *parser.FenceBlock) {
	switch b.Kind {
	case "sheet":
		// captured as meta
	case "grid":
		a := refs.ParseCell(first(b.Args.Positional))
		if a == nil {
			return
		}
		for ri, row := range b.Rows {
			for ci, text := range row.Cells {
				sc := scalar.Parse(text)
				if sc.Kind != scalar.Blank {
					s.setContent(a.Col+ci, a.Row+ri, scalarContent(sc))
				}
			}
		}
	case "spill-cache":
		a := refs.ParseCell(first(b.Args.Positional))
		if a == nil {
			return
		}
		for ri, row := range b.Rows {
			for ci, text := range row.Cells {
				sc := scalar.Parse(text)
				if sc.Kind == scalar.Blank {
					continue
				}
				if ri == 0 && ci == 0 {
					s.setContent(a.Col, a.Row, &Content{Cached: sc, HasCached: true})
				} else {
					s.setContent(a.Col+ci, a.Row+ri, &Content{Scalar: sc})
				}
			}
		}
	case "table":
		s.applyTable(b)
	case "cf":
		if rules, ok := b.Meta.([]interface{}); ok {
			s.CFRules += len(rules)
		}
	case "validation":
		s.Validations++
	case "chart":
		s.Charts++
	case "sparklines":
		s.Sparklines++
	case "pivot":
		s.Pivots++
	case "slicer":
		s.Slicers++
	case "image":
		s.Images++
	case "shape", "textbox":
		s.Shapes++
	case "comments":
		s.Threads++
	case "scenario":
		s.Scenarios++
	default:
		// filter/outline/page/checkbox/raw/query/script: carried or not
		// measured by the canonical dump.
	}
}

func (s *Sheet) applyTable(b *parser.FenceBlock) {
	anchor := ""
	if b.Args.Anchor != nil {
		anchor = *b.Args.Anchor
	}
	a := refs.ParseCell(anchor)
	if a == nil {
		return
	}
	tm, _ := b.Meta.(map[string]interface{})
	header := true
	if v, ok := tm["header"]; ok && v == false {
		header = false
	}
	var columns []string
	for ri, row := range b.Rows {
		for ci, text := range row.Cells {
			sc := scalar.Parse(text)
			if header && ri == 0 && sc.Kind == scalar.Text {
				columns = append(columns, sc.Str)
			}
			if sc.Kind != scalar.Blank {
				s.setContent(a.Col+ci, a.Row+ri, scalarContent(sc))
			}
		}
	}
	totalVal, hasTotalKey := tm["total"]
	hasTotals := hasTotalKey && totalVal != nil
	if totalMap, ok := totalVal.(map[string]interface{}); ok {
		totalRow := a.Row + len(b.Rows)
		for colName, v := range totalMap {
			ci := columnIndex(columns, colName)
			if ci == -1 {
				continue
			}
			sc := scalar.Parse(toStr(v))
			s.setContent(a.Col+ci, totalRow, scalarContent(sc))
		}
	}
	bodyRows := len(b.Rows)
	if header {
		bodyRows--
	}
	s.Tables = append(s.Tables, Table{
		Name: first(b.Args.Positional), Anchor: *a, Columns: columns,
		BodyRows: bodyRows, HasTotals: hasTotals,
	})
}

func (s *Sheet) applyAt(b *parser.AtBlock) {
	t := refs.ParseTarget(b.TargetText)
	if t == nil {
		return
	}
	body := b.Body
	if body == nil {
		body = map[string]interface{}{}
	}
	flow := b.Props
	if flow == nil {
		flow = map[string]interface{}{}
	}

	if b.ScalarText != nil {
		sc := scalar.Parse(*b.ScalarText)
		if t.Kind == refs.KindCell && sc.Kind != scalar.Blank {
			content := scalarContent(sc)
			if content.Formula != nil {
				spill := coalesce(flow["spill"], body["spill"])
				arr := coalesce(flow["array"], body["array"])
				if spill != nil || arr != nil {
					ref := toStr(coalesce(spill, arr))
					content.ArrayRef = &ref
				}
				if arr != nil {
					content.CSE = true
				}
			}
			s.setContent(t.C1, t.R1, content)
		} else if t.Kind == refs.KindRange && sc.Kind == scalar.Formula {
			for r := t.R1; r <= t.R2; r++ {
				for c := t.C1; c <= t.C2; c++ {
					f := TranslateFormula(sc.FValue, r-t.R1, c-t.C1)
					s.setContent(c, r, &Content{Formula: &f, CSE: false, HasCached: true})
				}
			}
		}
	} else {
		if content := bodyContent(body, flow); content != nil && t.Kind == refs.KindCell {
			s.setContent(t.C1, t.R1, content)
		}
	}

	merged := map[string]interface{}{}
	for k, v := range flow {
		merged[k] = v
	}
	for k, v := range bodyProps(body) {
		merged[k] = v
	}
	if merged["merge"] == true && t.Kind == refs.KindRange {
		s.Merges = append(s.Merges, MergeRange{C1: t.C1, R1: t.R1, C2: t.C2, R2: t.R2})
	}
	if truthy(merged["link"]) {
		s.Hyperlinks++
	}
	note := merged["note"]
	if note == nil {
		note = body["note"]
	}
	if truthy(note) {
		s.Notes++
	}
}

func bodyContent(body, flow map[string]interface{}) *Content {
	if fv, ok := body["formula"]; ok {
		f := strings.TrimPrefix(toStr(fv), "=")
		content := &Content{Formula: &f, CSE: false, HasCached: true}
		if v, ok := body["value"]; ok {
			content.Cached = yamlScalar(v)
		}
		spill := coalesce(body["spill"], flow["spill"])
		arr := coalesce(body["array"], flow["array"])
		if spill != nil || arr != nil {
			ref := toStr(coalesce(spill, arr))
			content.ArrayRef = &ref
		}
		if arr != nil {
			content.CSE = true
		}
		return content
	}
	if rv, ok := body["rich"]; ok {
		return &Content{Rich: toRuns(rv)}
	}
	if ev, ok := body["entity"]; ok {
		text := ""
		if em, ok := ev.(map[string]interface{}); ok {
			text = toStr(coalesce(em["text"], em["id"]))
		}
		return &Content{Scalar: &scalar.Scalar{Kind: scalar.Text, Str: text}}
	}
	if v, ok := body["value"]; ok {
		return &Content{Scalar: yamlScalar(v)}
	}
	return nil
}

var bodyContentKeys = map[string]bool{
	"value": true, "formula": true, "rich": true, "entity": true,
	"fields": true, "spill": true, "array": true,
}

func bodyProps(body map[string]interface{}) map[string]interface{} {
	out := map[string]interface{}{}
	for k, v := range body {
		if !bodyContentKeys[k] {
			out[k] = v
		}
	}
	return out
}

func yamlScalar(v interface{}) *scalar.Scalar {
	switch val := v.(type) {
	case float64:
		return &scalar.Scalar{Kind: scalar.Number, Num: val}
	case bool:
		return &scalar.Scalar{Kind: scalar.Boolean, Bool: val}
	default:
		str := toStr(v)
		if dateRe.MatchString(str) {
			return &scalar.Scalar{Kind: scalar.Date, Str: str}
		}
		if timeRe.MatchString(str) {
			return &scalar.Scalar{Kind: scalar.Time, Str: str}
		}
		return &scalar.Scalar{Kind: scalar.Text, Str: str}
	}
}

// TranslateFormula performs relative fill (SPEC §8.5): shift unanchored A1 refs
// by (dr, dc), skipping string literals and quoted sheet names.
func TranslateFormula(formula string, dr, dc int) string {
	var out strings.Builder
	i := 0
	rs := []byte(formula)
	for i < len(rs) {
		ch := rs[i]
		if ch == '"' || ch == '\'' {
			q := ch
			j := i + 1
			for j < len(rs) {
				if rs[j] == q {
					if j+1 < len(rs) && rs[j+1] == q {
						j += 2
						continue
					}
					break
				}
				j++
			}
			end := j + 1
			if end > len(rs) {
				end = len(rs)
			}
			out.Write(rs[i:end])
			i = end
			continue
		}
		m := refFillRe.FindStringSubmatch(formula[i:])
		var prev byte
		if o := out.String(); len(o) > 0 {
			prev = o[len(o)-1]
		}
		if m != nil && m[5] == "" && !isRefBoundary(prev) {
			cd, colL, rd, rowS := m[1], m[2], m[3], m[4]
			col := colL
			if cd != "$" {
				col = refs.NumToCol(maxInt(1, refs.ColToNum(colL)+dc))
			}
			row := rowS
			if rd != "$" {
				n, _ := strconv.Atoi(rowS)
				row = strconv.Itoa(maxInt(1, n+dr))
			}
			out.WriteString(cd + col + rd + row)
			i += len(m[0])
			continue
		}
		out.WriteByte(ch)
		i++
	}
	return out.String()
}

var refFillRe = regexp.MustCompile(`^(\$?)([A-Z]{1,3})(\$?)(\d{1,7})([A-Za-z0-9_(]?)`)

func isRefBoundary(b byte) bool {
	return (b >= 'A' && b <= 'Z') || (b >= 'a' && b <= 'z') || (b >= '0' && b <= '9') || b == '_' || b == '.'
}

func columnIndex(columns []string, name string) int {
	lower := strings.ToLower(name)
	for i, c := range columns {
		if strings.ToLower(c) == lower {
			return i
		}
	}
	return -1
}

func first(s []string) string {
	if len(s) == 0 {
		return ""
	}
	return s[0]
}

func coalesce(a, b interface{}) interface{} {
	if a != nil {
		return a
	}
	return b
}

func truthy(v interface{}) bool {
	switch val := v.(type) {
	case nil:
		return false
	case bool:
		return val
	case string:
		return val != ""
	case float64:
		return val != 0
	default:
		return true
	}
}

func toRuns(v interface{}) []map[string]interface{} {
	list, ok := v.([]interface{})
	if !ok {
		return nil
	}
	out := make([]map[string]interface{}, 0, len(list))
	for _, item := range list {
		if m, ok := item.(map[string]interface{}); ok {
			out = append(out, m)
		} else {
			out = append(out, map[string]interface{}{})
		}
	}
	return out
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// toStr mirrors JS String(v) for the value types the YAML subset yields.
func toStr(v interface{}) string {
	switch val := v.(type) {
	case nil:
		return ""
	case string:
		return val
	case bool:
		if val {
			return "true"
		}
		return "false"
	case float64:
		return numfmt.Format(val)
	default:
		return ""
	}
}
