// Package dump produces the canonical model dump — the cross-language
// conformance contract (conformance/README.md). Output matches
// JSON.stringify(value, null, 1): 1-space indent, fixed key order, shortest
// round-trip numbers, a trailing newline, UTF-8.
package dump

import (
	"sort"
	"strconv"
	"strings"

	"github.com/lprhodes/grid-md/go/internal/model"
	"github.com/lprhodes/grid-md/go/internal/numfmt"
	"github.com/lprhodes/grid-md/go/internal/refs"
	"github.com/lprhodes/grid-md/go/internal/scalar"
)

// omap is an insertion-ordered object for the dump tree.
type omap struct {
	keys []string
	vals []interface{}
}

func newOMap() *omap { return &omap{} }

func (m *omap) set(k string, v interface{}) {
	m.keys = append(m.keys, k)
	m.vals = append(m.vals, v)
}

// Model renders the canonical dump for a materialized workbook.
func Model(wb *model.Workbook) string {
	fm := wb.FM
	out := newOMap()
	out.set("gridmd", scalarLike(fm["gridmd"]))
	out.set("title", scalarLike(fm["title"]))
	dateSystem := 1900.0
	if ds, ok := fm["date-system"].(float64); ok && ds == 1904 {
		dateSystem = 1904
	}
	out.set("dateSystem", dateSystem)
	out.set("names", dumpNames(fm))

	sheets := make([]interface{}, 0, len(wb.Sheets))
	for _, s := range wb.Sheets {
		sheets = append(sheets, dumpSheet(s))
	}
	out.set("sheets", sheets)

	var b strings.Builder
	serialize(&b, out, 0)
	b.WriteByte('\n')
	return b.String()
}

func dumpNames(fm map[string]interface{}) []interface{} {
	raw, _ := fm["names"].([]interface{})
	type nameEntry struct {
		name string
		om   *omap
	}
	var entries []nameEntry
	for _, item := range raw {
		n, ok := item.(map[string]interface{})
		if !ok {
			continue
		}
		name := asString(n["name"])
		om := newOMap()
		om.set("name", name)
		om.set("ref", refOrNil(n, "ref"))
		om.set("formula", refOrNil(n, "formula"))
		if v, ok := n["value"]; ok {
			om.set("value", stringify(v))
		} else {
			om.set("value", nil)
		}
		entries = append(entries, nameEntry{name: name, om: om})
	}
	sort.SliceStable(entries, func(i, j int) bool { return entries[i].name < entries[j].name })
	out := make([]interface{}, 0, len(entries))
	for _, e := range entries {
		out = append(out, e.om)
	}
	return out
}

func refOrNil(m map[string]interface{}, key string) interface{} {
	v, ok := m[key]
	if !ok || v == nil {
		return nil
	}
	return scalarLike(v)
}

func dumpSheet(s *model.Sheet) *omap {
	om := newOMap()
	om.set("name", s.Name)
	om.set("kind", s.Kind)
	om.set("hidden", hiddenValue(s.Meta["hidden"]))
	if fz, ok := s.Meta["freeze"].(string); ok {
		om.set("freeze", fz)
	} else {
		om.set("freeze", nil)
	}
	protected := false
	if pm, ok := s.Meta["protect"].(map[string]interface{}); ok {
		protected = boolish(pm["enabled"])
	}
	om.set("protected", protected)
	om.set("cells", dumpCells(s))
	om.set("merges", dumpMerges(s))
	om.set("tables", dumpTables(s))

	counts := newOMap()
	counts.set("cf", float64(s.CFRules))
	counts.set("validations", float64(s.Validations))
	counts.set("notes", float64(s.Notes))
	counts.set("threads", float64(s.Threads))
	counts.set("scenarios", float64(s.Scenarios))
	counts.set("sparklines", float64(s.Sparklines))
	counts.set("charts", float64(s.Charts))
	counts.set("pivots", float64(s.Pivots))
	counts.set("slicers", float64(s.Slicers))
	counts.set("images", float64(s.Images))
	counts.set("shapes", float64(s.Shapes))
	counts.set("hyperlinks", float64(s.Hyperlinks))
	om.set("counts", counts)
	return om
}

func dumpCells(s *model.Sheet) *omap {
	cells := s.Cells()
	sorted := make([]*model.Cell, len(cells))
	copy(sorted, cells)
	sort.SliceStable(sorted, func(i, j int) bool {
		if sorted[i].Row != sorted[j].Row {
			return sorted[i].Row < sorted[j].Row
		}
		return sorted[i].Col < sorted[j].Col
	})
	om := newOMap()
	for _, c := range sorted {
		// Every materialized cell is content-bearing (model.setContent), so no
		// nil-content filter is needed here.
		ref := refs.NumToCol(c.Col) + strconv.Itoa(c.Row)
		om.set(ref, dumpContent(c.Content))
	}
	return om
}

func dumpContent(ct *model.Content) interface{} {
	if ct.Rich != nil {
		var b strings.Builder
		for _, run := range ct.Rich {
			b.WriteString(asString(run["text"]))
		}
		e := newOMap()
		e.set("t", "rich")
		e.set("v", b.String())
		return e
	}
	if ct.Formula != nil {
		e := newOMap()
		e.set("t", "f")
		e.set("f", *ct.Formula)
		e.set("cached", scalarDump(ct.Cached))
		if ct.ArrayRef != nil {
			e.set("array", *ct.ArrayRef)
		} else {
			e.set("array", nil)
		}
		return e
	}
	return scalarDump(ct.Scalar)
}

func scalarDump(s *scalar.Scalar) interface{} {
	if s == nil {
		return nil
	}
	e := newOMap()
	switch s.Kind {
	case scalar.Number:
		e.set("t", "n")
		e.set("v", s.Num)
	case scalar.Boolean:
		e.set("t", "b")
		e.set("v", s.Bool)
	case scalar.Error:
		e.set("t", "e")
		e.set("v", s.Str)
	case scalar.Date, scalar.Time:
		e.set("t", "d")
		e.set("v", s.Str)
	default:
		e.set("t", "s")
		e.set("v", s.Str)
	}
	return e
}

func dumpMerges(s *model.Sheet) []interface{} {
	refsList := make([]string, 0, len(s.Merges))
	for _, m := range s.Merges {
		refsList = append(refsList, refs.NumToCol(m.C1)+strconv.Itoa(m.R1)+":"+refs.NumToCol(m.C2)+strconv.Itoa(m.R2))
	}
	sort.Strings(refsList)
	out := make([]interface{}, 0, len(refsList))
	for _, r := range refsList {
		out = append(out, r)
	}
	return out
}

func dumpTables(s *model.Sheet) []interface{} {
	sorted := make([]model.Table, len(s.Tables))
	copy(sorted, s.Tables)
	sort.SliceStable(sorted, func(i, j int) bool { return sorted[i].Name < sorted[j].Name })
	out := make([]interface{}, 0, len(sorted))
	for _, t := range sorted {
		om := newOMap()
		om.set("name", t.Name)
		om.set("anchor", refs.NumToCol(t.Anchor.Col)+strconv.Itoa(t.Anchor.Row))
		cols := make([]interface{}, 0, len(t.Columns))
		for _, c := range t.Columns {
			cols = append(cols, c)
		}
		om.set("columns", cols)
		om.set("bodyRows", float64(t.BodyRows))
		om.set("hasTotals", t.HasTotals)
		out = append(out, om)
	}
	return out
}

func hiddenValue(v interface{}) interface{} {
	if v == true {
		return true
	}
	if v == "very" {
		return "very"
	}
	return false
}

func boolish(v interface{}) bool {
	b, ok := v.(bool)
	return ok && b
}

// scalarLike passes through the primitive JSON types; anything else becomes nil.
func scalarLike(v interface{}) interface{} {
	switch v.(type) {
	case nil, string, bool, float64:
		return v
	default:
		return nil
	}
}

func asString(v interface{}) string {
	if s, ok := v.(string); ok {
		return s
	}
	return ""
}

// stringify mirrors JS String(v) for the YAML subset value types.
func stringify(v interface{}) interface{} {
	switch val := v.(type) {
	case string:
		return val
	case bool:
		if val {
			return "true"
		}
		return "false"
	case float64:
		return numfmt.Format(val)
	case nil:
		return ""
	default:
		return ""
	}
}

// ---- JSON serialization (JSON.stringify(v, null, 1)) ----

func serialize(b *strings.Builder, v interface{}, depth int) {
	switch val := v.(type) {
	case *omap:
		if len(val.keys) == 0 {
			b.WriteString("{}")
			return
		}
		b.WriteString("{\n")
		inner := depth + 1
		pad := strings.Repeat(" ", inner)
		for i, k := range val.keys {
			if i > 0 {
				b.WriteString(",\n")
			}
			b.WriteString(pad)
			writeJSONString(b, k)
			b.WriteString(": ")
			serialize(b, val.vals[i], inner)
		}
		b.WriteByte('\n')
		b.WriteString(strings.Repeat(" ", depth))
		b.WriteByte('}')
	case []interface{}:
		if len(val) == 0 {
			b.WriteString("[]")
			return
		}
		b.WriteString("[\n")
		inner := depth + 1
		pad := strings.Repeat(" ", inner)
		for i, item := range val {
			if i > 0 {
				b.WriteString(",\n")
			}
			b.WriteString(pad)
			serialize(b, item, inner)
		}
		b.WriteByte('\n')
		b.WriteString(strings.Repeat(" ", depth))
		b.WriteByte(']')
	case string:
		writeJSONString(b, val)
	case bool:
		if val {
			b.WriteString("true")
		} else {
			b.WriteString("false")
		}
	case float64:
		b.WriteString(numfmt.Format(val))
	case nil:
		b.WriteString("null")
	default:
		b.WriteString("null")
	}
}

const hexDigits = "0123456789abcdef"

func writeJSONString(b *strings.Builder, s string) {
	b.WriteByte('"')
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch c {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\n':
			b.WriteString(`\n`)
		case '\r':
			b.WriteString(`\r`)
		case '\t':
			b.WriteString(`\t`)
		case '\b':
			b.WriteString(`\b`)
		case '\f':
			b.WriteString(`\f`)
		default:
			if c < 0x20 {
				b.WriteString(`\u00`)
				b.WriteByte(hexDigits[c>>4])
				b.WriteByte(hexDigits[c&0xf])
			} else {
				b.WriteByte(c)
			}
		}
	}
	b.WriteByte('"')
}
