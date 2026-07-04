// Package validate performs GridMD strict-mode semantic validation
// (SPEC §9.4, §12–§13). It implements the structural error rules that the
// conformance suite exercises — define-once, sheet/name/table integrity, and
// spill-cache ownership — plus target and frontmatter checks. Feature-body
// validation (cf/validation/chart/… option shapes) and the WARN-level rules of
// the JS reference are intentionally out of scope for this Tier-1 port; see the
// README "Deliberate divergences".
package validate

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/lprhodes/grid-md/go/internal/parser"
	"github.com/lprhodes/grid-md/go/internal/refs"
	"github.com/lprhodes/grid-md/go/internal/scalar"
)

var (
	gridmdRe      = regexp.MustCompile(`^\d+\.\d+$`)
	sheetNameBad  = regexp.MustCompile(`[:\\/?*[\]]`)
	tableNameRe   = regexp.MustCompile(`^[A-Za-z_\\][A-Za-z0-9_.\\]{0,254}$`)
	cellishNameRe = regexp.MustCompile(`^[A-Za-z]{1,3}\d+$`)
)

var contentKeys = []string{"value", "formula", "rich", "entity"}

// Validate appends strict-mode errors to doc.Errors.
func Validate(doc *parser.Document) {
	err := func(line int, msg string) { doc.Errors = append(doc.Errors, parser.Diag{Line: line, Msg: msg}) }
	globalNames := map[string]bool{}

	fm := doc.Frontmatter
	if g, ok := fm["gridmd"].(string); !ok || !gridmdRe.MatchString(g) {
		err(2, `frontmatter requires gridmd: "MAJOR.MINOR" (quoted string)`)
	}
	if names, ok := fm["names"].([]interface{}); ok {
		for _, item := range names {
			n, ok := item.(map[string]interface{})
			if !ok {
				err(2, "names entries require a name")
				continue
			}
			name, ok := n["name"].(string)
			if !ok {
				err(2, "names entries require a name")
				continue
			}
			forms := 0
			for _, k := range []string{"ref", "formula", "value"} {
				if _, present := n[k]; present {
					forms++
				}
			}
			if forms != 1 {
				err(2, fmt.Sprintf("name %s: exactly one of ref | formula | value required", name))
			}
			key := strings.ToLower(name)
			if globalNames[key] {
				err(2, "duplicate defined name: "+name)
			}
			globalNames[key] = true
		}
	}

	if len(doc.Sheets) == 0 {
		err(1, "a workbook requires at least one sheet (a level-1 heading)")
	}
	sheetNames := map[string]bool{}
	for _, sheet := range doc.Sheets {
		nameKey := strings.ToLower(sheet.Name)
		if len([]rune(sheet.Name)) > 31 {
			err(sheet.Line, "sheet name exceeds 31 chars: "+sheet.Name)
		}
		if sheetNameBad.MatchString(sheet.Name) {
			err(sheet.Line, "sheet name contains a forbidden character (: \\ / ? * [ ]): "+sheet.Name)
		}
		if sheetNames[nameKey] {
			err(sheet.Line, "duplicate sheet name: "+sheet.Name)
		}
		sheetNames[nameKey] = true
		validateSheet(doc, sheet, globalNames)
	}
}

type spillRange struct {
	c1, r1, c2, r2 int
}

func validateSheet(doc *parser.Document, sheet *parser.Sheet, globalNames map[string]bool) {
	err := func(line int, msg string) { doc.Errors = append(doc.Errors, parser.Diag{Line: line, Msg: msg}) }
	defs := map[string]int{}
	var spills []spillRange
	var spillCaches []*parser.FenceBlock

	addDef := func(col, row, line int, what string) {
		if col > refs.MaxCol || row > refs.MaxRow {
			err(line, what+": cell out of bounds")
			return
		}
		k := refs.RefKey(col, row)
		if prev, ok := defs[k]; ok {
			err(line, fmt.Sprintf("%s: cell defined more than once (previous definition at line %d)", what, prev))
			return
		}
		defs[k] = line
	}

	for _, b := range sheet.Blocks {
		switch blk := b.(type) {
		case *parser.AtBlock:
			validateAt(doc, sheet, blk, addDef, &spills)
		case *parser.FenceBlock:
			if strings.HasPrefix(blk.Kind, "x-") {
				continue
			}
			if !parser.ReservedKinds[blk.Kind] {
				err(blk.Line, fmt.Sprintf("unknown directive {%s}", blk.Kind))
				continue
			}
			if blk.Kind == "sheet" {
				continue
			}
			if blk.Kind == "spill-cache" {
				spillCaches = append(spillCaches, blk)
				continue
			}
			validateFence(doc, blk, addDef, globalNames)
		}
	}

	for _, sc := range spillCaches {
		anchor := refs.ParseCell(firstArg(sc))
		if anchor == nil {
			err(sc.Line, "{spill-cache} requires a cell anchor")
			continue
		}
		h := len(sc.Rows)
		w := 0
		for _, row := range sc.Rows {
			if len(row.Cells) > w {
				w = len(row.Cells)
			}
		}
		var owner *spillRange
		for i := range spills {
			if spills[i].c1 == anchor.Col && spills[i].r1 == anchor.Row {
				owner = &spills[i]
				break
			}
		}
		if owner == nil {
			err(sc.Line, fmt.Sprintf("{spill-cache} at %s has no owning spill/array formula at that anchor", firstArg(sc)))
			continue
		}
		if anchor.Row+h-1 > owner.r2 || anchor.Col+w-1 > owner.c2 {
			err(sc.Line, "{spill-cache} rectangle exceeds the declared spill/array range")
		}
	}
}

func validateFence(doc *parser.Document, b *parser.FenceBlock, addDef func(int, int, int, string), globalNames map[string]bool) {
	err := func(line int, msg string) { doc.Errors = append(doc.Errors, parser.Diag{Line: line, Msg: msg}) }
	switch b.Kind {
	case "grid":
		anchor := refs.ParseCell(firstArg(b))
		if anchor == nil {
			err(b.Line, "{grid} requires a cell anchor")
			return
		}
		for ri, row := range b.Rows {
			for ci, text := range row.Cells {
				sc := scalar.Parse(text)
				if sc.Problem != "" {
					err(row.Line, "grid cell: "+sc.Problem)
				}
				if sc.Kind != scalar.Blank {
					addDef(anchor.Col+ci, anchor.Row+ri, row.Line, "{grid}")
				}
			}
		}
	case "table":
		validateTable(doc, b, addDef, globalNames)
	default:
		// Other reserved kinds carry feature payloads not validated in this
		// Tier-1 port (see package doc).
	}
}

func validateTable(doc *parser.Document, b *parser.FenceBlock, addDef func(int, int, int, string), globalNames map[string]bool) {
	err := func(line int, msg string) { doc.Errors = append(doc.Errors, parser.Diag{Line: line, Msg: msg}) }
	name := firstArg(b)
	if !(tableNameRe.MatchString(name) && !cellishNameRe.MatchString(name)) {
		err(b.Line, "{table} requires a valid table name")
	}
	anchor := refs.ParseCell(anchorArg(b))
	if anchor == nil {
		err(b.Line, "{table} requires `at <cell>`")
	}
	if name != "" {
		key := strings.ToLower(name)
		if globalNames[key] {
			err(b.Line, "{table} table name collides with an existing name: "+name)
		}
		globalNames[key] = true
	}
	if anchor == nil || len(b.Rows) == 0 {
		if len(b.Rows) == 0 {
			err(b.Line, "{table} requires payload rows")
		}
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
			if sc.Problem != "" {
				err(row.Line, "table cell: "+sc.Problem)
			}
			if header && ri == 0 {
				if sc.Kind != scalar.Text || sc.Str == "" {
					err(row.Line, fmt.Sprintf("table header cells must be non-empty text (column %d)", ci+1))
				} else {
					columns = append(columns, sc.Str)
				}
				addDef(anchor.Col+ci, anchor.Row+ri, row.Line, "{table} header")
				continue
			}
			if sc.Kind != scalar.Blank {
				addDef(anchor.Col+ci, anchor.Row+ri, row.Line, "{table}")
			}
		}
	}
	lower := make([]string, len(columns))
	for i, c := range columns {
		lower[i] = strings.ToLower(c)
	}
	for i := range lower {
		if indexOfStr(lower, lower[i]) != i {
			err(b.Line, "duplicate table column name: "+columns[i])
		}
	}
	if totalMap, ok := tm["total"].(map[string]interface{}); ok {
		totalRow := anchor.Row + len(b.Rows)
		for colName := range totalMap {
			ci := indexOfStr(lower, strings.ToLower(colName))
			if ci != -1 {
				addDef(anchor.Col+ci, totalRow, b.Line, "{table} total")
			}
		}
	}
}

func validateAt(doc *parser.Document, sheet *parser.Sheet, b *parser.AtBlock, addDef func(int, int, int, string), spills *[]spillRange) {
	err := func(line int, msg string) { doc.Errors = append(doc.Errors, parser.Diag{Line: line, Msg: msg}) }
	t := refs.ParseTarget(b.TargetText)
	if t == nil {
		err(b.Line, "invalid @ target: "+b.TargetText)
		return
	}
	if t.HasSheet && strings.ToLower(t.Sheet) != strings.ToLower(sheet.Name) {
		err(b.Line, fmt.Sprintf("@ target qualifier %s! must name the containing sheet", t.Sheet))
	}
	body := b.Body
	if body == nil {
		body = map[string]interface{}{}
	}
	props := map[string]interface{}{}
	for k, v := range b.Props {
		props[k] = v
	}
	for k, v := range body {
		props[k] = v
	}

	var bodyContentKeys []string
	for _, k := range contentKeys {
		if _, ok := body[k]; ok {
			bodyContentKeys = append(bodyContentKeys, k)
		}
	}

	var sc *scalar.Scalar
	if b.ScalarText != nil {
		sc = scalar.Parse(*b.ScalarText)
		if sc.Problem != "" {
			err(b.Line, "scalar: "+sc.Problem)
		}
		if sc.Cached != nil && sc.Cached.Kind == scalar.Invalid {
			err(b.Line, "scalar: "+sc.Cached.Problem)
		}
		cachedOnly := len(bodyContentKeys) == 1 && bodyContentKeys[0] == "value" && sc.Kind == scalar.Formula
		if len(bodyContentKeys) > 0 && !cachedOnly {
			err(b.Line, "inline content and body content keys on the same @ directive")
		}
	}
	_, bodyHasFormula := body["formula"]
	hasFormula := (sc != nil && sc.Kind == scalar.Formula) || bodyHasFormula
	hasContent := (sc != nil && sc.Kind != scalar.Blank) || len(bodyContentKeys) > 0

	if hasContent {
		switch {
		case t.Kind == refs.KindCell:
			addDef(t.C1, t.R1, b.Line, "@")
		case t.Kind == refs.KindRange && hasFormula:
			count := (t.R2 - t.R1 + 1) * (t.C2 - t.C1 + 1)
			if count <= 10000 {
				for r := t.R1; r <= t.R2; r++ {
					for c := t.C1; c <= t.C2; c++ {
						addDef(c, r, b.Line, "@ fill")
					}
				}
			}
		default:
			err(b.Line, "range targets accept formula content only (relative fill, SPEC §8.5/§9.4)")
		}
	}

	for k, v := range props {
		if k == "merge" {
			if t.Kind != refs.KindRange {
				err(b.Line, "merge: requires a range target")
			}
			if v != true {
				err(b.Line, "merge: only `true` is valid")
			}
		}
		if k == "spill" || k == "array" {
			st := refs.ParseTarget(asStr(v))
			if st == nil || st.Kind != refs.KindRange {
				err(b.Line, k+": must be a range")
				continue
			}
			if t.Kind != refs.KindCell || st.C1 != t.C1 || st.R1 != t.R1 {
				err(b.Line, k+": range must start at the anchor cell")
			}
			*spills = append(*spills, spillRange{c1: st.C1, r1: st.R1, c2: st.C2, r2: st.R2})
		}
	}
}

func firstArg(b *parser.FenceBlock) string {
	if len(b.Args.Positional) == 0 {
		return ""
	}
	return b.Args.Positional[0]
}

func anchorArg(b *parser.FenceBlock) string {
	if b.Args.Anchor == nil {
		return ""
	}
	return *b.Args.Anchor
}

func indexOfStr(list []string, v string) int {
	for i, s := range list {
		if s == v {
			return i
		}
	}
	return -1
}

func asStr(v interface{}) string {
	switch val := v.(type) {
	case string:
		return val
	case bool:
		if val {
			return "true"
		}
		return "false"
	default:
		return ""
	}
}
