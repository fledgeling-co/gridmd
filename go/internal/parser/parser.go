// Package parser implements the GridMD document parser (SPEC §2–§10, Appendix
// A). It produces a block tree; semantic checks live in package validate.
package parser

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"github.com/lprhodes/grid-md/go/internal/yamlsubset"
)

// ReservedKinds is the closed set of directive kinds in 0.1.
var ReservedKinds = map[string]bool{
	"sheet": true, "grid": true, "spill-cache": true, "table": true, "cf": true,
	"validation": true, "filter": true, "chart": true, "sparklines": true,
	"pivot": true, "slicer": true, "image": true, "shape": true, "textbox": true,
	"checkbox": true, "comments": true, "outline": true, "page": true,
	"query": true, "script": true, "scenario": true, "raw": true,
}

// Diag is a line-tagged diagnostic message.
type Diag struct {
	Line int
	Msg  string
}

// Size is a directive `size WxH` argument in pixels.
type Size struct {
	W, H int
}

// InfoArgs holds a fence info-string's parsed arguments.
type InfoArgs struct {
	Positional []string
	Flags      map[string]string
	Anchor     *string
	Size       *Size
}

// Row is one payload pipe row of a grid/table/spill-cache block.
type Row struct {
	Cells []string
	Line  int
}

// Block is either an @-directive or a fenced directive.
type Block interface{ isBlock() }

// AtBlock is a parsed @ directive.
type AtBlock struct {
	Line       int
	TargetText string
	ScalarText *string
	Props      map[string]interface{}
	Body       map[string]interface{}
}

func (b *AtBlock) isBlock() { _ = b }

// FenceBlock is a parsed fenced directive.
type FenceBlock struct {
	Line    int
	Kind    string
	Args    InfoArgs
	Body    []string
	Rows    []Row
	Meta    interface{}
	Code    string
	Payload string
}

func (b *FenceBlock) isBlock() { _ = b }

// Sheet is a level-1 heading and its blocks.
type Sheet struct {
	Name   string
	Line   int
	Blocks []Block
}

// Document is the full parsed tree.
type Document struct {
	Frontmatter    map[string]interface{}
	WorkbookBlocks []Block
	Sheets         []*Sheet
	Errors         []Diag
	Warnings       []Diag
	Mode           string

	// Stats populated by validate.
	StatsDefs   int
	StatsBlocks int
}

// ParseYaml parses a YAML fragment, appending any diagnostics at the given line
// to errors. An empty fragment yields an empty map.
func ParseYaml(text string, line int, errors *[]Diag) interface{} {
	v, errs := yamlsubset.Decode(text)
	for _, e := range errs {
		*errors = append(*errors, Diag{Line: line, Msg: e})
	}
	return v
}

// TryProps returns the flow-map props for an @ directive when text parses to a
// mapping of identifier keys with non-null values (SPEC Appendix A).
func TryProps(text string) (map[string]interface{}, bool) {
	return yamlsubset.TryProps(text)
}

// FindPropsSplit performs the right-edge props split (SPEC §9.1 / Appendix A):
// the last brace-balanced {…} group (double quotes respected) that runs to
// end-of-line and is preceded by whitespace.
func FindPropsSplit(text string) (scalarText string, propsText string, ok bool) {
	if !strings.HasSuffix(text, "}") {
		return text, "", false
	}
	inQ := false
	depth := 0
	start := -1
	lastStart, lastEnd := -1, -1
	for i := 0; i < len(text); i++ {
		ch := text[i]
		if ch == '"' {
			inQ = !inQ
			continue
		}
		if inQ {
			continue
		}
		if ch == '{' {
			if depth == 0 {
				start = i
			}
			depth++
		} else if ch == '}' {
			depth--
			if depth == 0 && start != -1 {
				lastStart, lastEnd = start, i
			}
			if depth < 0 {
				return text, "", false
			}
		}
	}
	if lastStart == -1 {
		return text, "", false
	}
	s, e := lastStart, lastEnd
	if e != len(text)-1 || s == 0 || text[s-1] != ' ' {
		return text, "", false
	}
	return strings.TrimRight(text[:s], " \t"), text[s:], true
}

// SplitPipeRow splits a pipe row into trimmed cell strings; a backslash escapes
// the next character. Returns ok=false for a malformed row.
func SplitPipeRow(rawLine string) ([]string, bool) {
	line := strings.TrimRight(rawLine, " \t\r\n\v\f")
	if !strings.HasPrefix(line, "|") || len(line) < 2 {
		return nil, false
	}
	var cells []string
	var cell strings.Builder
	opened := false
	for i := 0; i < len(line); i++ {
		ch := line[i]
		if ch == '\\' && i+1 < len(line) {
			cell.WriteByte(line[i+1])
			i++
			continue
		}
		if ch == '|' {
			if !opened {
				opened = true
				continue
			}
			cells = append(cells, strings.TrimSpace(cell.String()))
			cell.Reset()
			continue
		}
		cell.WriteByte(ch)
	}
	if strings.TrimSpace(cell.String()) != "" {
		return nil, false // no unescaped closing pipe
	}
	return cells, true
}

var infoTokenRe = regexp.MustCompile(`"((?:[^"]|"")*)"|\S+`)
var flagRe = regexp.MustCompile(`^([A-Za-z][A-Za-z0-9-]*)=(.*)$`)
var sizeRe = regexp.MustCompile(`^(\d+)x(\d+)$`)
var flagQuoteRe = regexp.MustCompile(`(?s)^"(.*)"$`)

type infoToken struct {
	v string
	q bool
}

// ParseInfoArgs parses a fence info string into positional args, flags, an
// anchor, and a size.
func ParseInfoArgs(rest string, line int, errors *[]Diag) InfoArgs {
	out := InfoArgs{Flags: map[string]string{}}
	var tokens []infoToken
	for _, m := range infoTokenRe.FindAllStringSubmatchIndex(rest, -1) {
		if m[2] != -1 {
			inner := rest[m[2]:m[3]]
			tokens = append(tokens, infoToken{v: strings.ReplaceAll(inner, `""`, `"`), q: true})
		} else {
			tokens = append(tokens, infoToken{v: rest[m[0]:m[1]], q: false})
		}
	}
	for k := 0; k < len(tokens); k++ {
		tok := tokens[k]
		if !tok.q && tok.v == "at" {
			if k+1 >= len(tokens) {
				*errors = append(*errors, Diag{Line: line, Msg: "`at` requires an anchor"})
				break
			}
			k++
			a := tokens[k].v
			out.Anchor = &a
			continue
		}
		if !tok.q && tok.v == "size" {
			if k+1 >= len(tokens) {
				*errors = append(*errors, Diag{Line: line, Msg: "`size` requires WxH (e.g. 480x320)"})
				continue
			}
			k++
			sm := sizeRe.FindStringSubmatch(tokens[k].v)
			if sm == nil {
				*errors = append(*errors, Diag{Line: line, Msg: "`size` requires WxH (e.g. 480x320)"})
				continue
			}
			w, _ := strconv.Atoi(sm[1])
			h, _ := strconv.Atoi(sm[2])
			out.Size = &Size{W: w, H: h}
			continue
		}
		if !tok.q {
			if fm := flagRe.FindStringSubmatch(tok.v); fm != nil {
				val := flagQuoteRe.ReplaceAllString(fm[2], "$1")
				val = strings.ReplaceAll(val, `""`, `"`)
				out.Flags[fm[1]] = val
				continue
			}
		}
		out.Positional = append(out.Positional, tok.v)
	}
	return out
}

var fenceOpenRe = regexp.MustCompile("^(`{3,})\\{([A-Za-z][A-Za-z0-9-]*)\\}(.*)$")

func parseFence(lines []string, i int, m []string, errors *[]Diag) (*FenceBlock, int) {
	open := len(m[1])
	kind := m[2]
	args := ParseInfoArgs(m[3], i+1, errors)
	var body []string
	j := i + 1
	closed := false
	closeRe := regexp.MustCompile(fmt.Sprintf("^`{%d,}\\s*$", open))
	for j < len(lines) {
		if closeRe.MatchString(lines[j]) {
			closed = true
			j++
			break
		}
		body = append(body, lines[j])
		j++
	}
	if !closed {
		*errors = append(*errors, Diag{Line: i + 1, Msg: fmt.Sprintf("unclosed {%s} fence", kind)})
	}
	block := &FenceBlock{Line: i + 1, Kind: kind, Args: args, Body: body}
	refineFence(block, errors)
	return block, j
}

func parseRows(bodyLines []string, baseLine int, errors *[]Diag) []Row {
	var rows []Row
	for k, l := range bodyLines {
		if strings.TrimSpace(l) == "" {
			continue
		}
		cells, ok := SplitPipeRow(l)
		if !ok {
			snippet := l
			if len(snippet) > 50 {
				snippet = snippet[:50]
			}
			*errors = append(*errors, Diag{Line: baseLine + k + 1, Msg: "expected a pipe row, got: " + snippet})
			continue
		}
		rows = append(rows, Row{Cells: cells, Line: baseLine + k + 1})
	}
	return rows
}

func indexOfSeparator(body []string) int {
	for i, l := range body {
		if l == "---" {
			return i
		}
	}
	return -1
}

func refineFence(block *FenceBlock, errors *[]Diag) {
	kind := block.Kind
	body := block.Body
	line := block.Line
	meta := func(arr []string, off int) interface{} {
		return ParseYaml(strings.Join(arr, "\n"), line+off, errors)
	}
	switch {
	case kind == "grid" || kind == "spill-cache":
		block.Rows = parseRows(body, line, errors)
	case kind == "table":
		d := indexOfSeparator(body)
		if d == -1 {
			*errors = append(*errors, Diag{Line: line, Msg: "{table} requires a `---`-separated payload of pipe rows"})
			block.Meta = meta(body, 1)
			block.Rows = nil
		} else {
			block.Meta = meta(body[:d], 1)
			block.Rows = parseRows(body[d+1:], line+d+1, errors)
		}
	case kind == "script":
		d := indexOfSeparator(body)
		if d == -1 {
			block.Meta = map[string]interface{}{}
			block.Code = strings.Join(body, "\n")
		} else {
			block.Meta = meta(body[:d], 1)
			block.Code = strings.Join(body[d+1:], "\n")
		}
	case kind == "raw" || strings.HasPrefix(kind, "x-"):
		block.Payload = strings.Join(body, "\n")
	default:
		block.Meta = meta(body, 1)
	}
}

func parseAt(lines []string, i int, errors *[]Diag) (*AtBlock, int) {
	line := lines[i]
	rest := line[2:]
	sp := strings.IndexByte(rest, ' ')
	targetText := rest
	inline := ""
	if sp != -1 {
		targetText = rest[:sp]
		inline = strings.TrimSpace(rest[sp+1:])
	}

	// Multiline body (Appendix A dedent rule): maximal run of blank-or-2-space
	// lines, trailing blanks excluded.
	j := i + 1
	taken := 0
	lastTake := 0
	var acc []string
	for j < len(lines) {
		l := lines[j]
		if strings.TrimSpace(l) == "" {
			acc = append(acc, "")
			j++
			taken++
			continue
		}
		if strings.HasPrefix(l, "  ") {
			acc = append(acc, l[2:])
			j++
			taken++
			lastTake = taken
			continue
		}
		break
	}
	var bodyLines []string
	if lastTake > 0 {
		bodyLines = acc[:lastTake]
	}
	next := i + 1 + lastTake

	block := &AtBlock{Line: i + 1, TargetText: targetText}
	if bodyLines != nil {
		parsed := ParseYaml(strings.Join(bodyLines, "\n"), i+2, errors)
		if m, ok := parsed.(map[string]interface{}); ok {
			block.Body = m
		} else {
			*errors = append(*errors, Diag{Line: i + 2, Msg: "@ directive body must be a YAML mapping"})
		}
	}
	if inline != "" {
		if strings.HasPrefix(inline, "{") && !strings.HasPrefix(inline, "{=") {
			if props, ok := TryProps(inline); ok {
				block.Props = props
				return block, next
			}
		}
		scalarText, propsText, ok := FindPropsSplit(inline)
		if ok {
			if props, pok := TryProps(propsText); pok {
				block.Props = props
				if scalarText != "" {
					st := scalarText
					block.ScalarText = &st
				}
				return block, next
			}
		}
		st := inline
		block.ScalarText = &st
	}
	return block, next
}

var headingRe = regexp.MustCompile(`^# (.+)$`)
var docCommentRe = regexp.MustCompile(`^#{2,}(\s|$)`)

// Parse parses source into a Document. mode is "strict" (default) or "lenient".
func Parse(source string, mode string) *Document {
	if mode == "" {
		mode = "strict"
	}
	doc := &Document{Frontmatter: map[string]interface{}{}, Mode: mode}
	lines := strings.Split(strings.ReplaceAll(source, "\r\n", "\n"), "\n")

	if len(lines) == 0 || lines[0] != "---" {
		doc.Errors = append(doc.Errors, Diag{Line: 1, Msg: "document must begin with `---` YAML frontmatter"})
		return doc
	}
	fmEnd := -1
	for k := 1; k < len(lines); k++ {
		if lines[k] == "---" {
			fmEnd = k
			break
		}
	}
	if fmEnd == -1 {
		doc.Errors = append(doc.Errors, Diag{Line: 1, Msg: "unterminated frontmatter (missing closing `---`)"})
		return doc
	}
	if fm, ok := ParseYaml(strings.Join(lines[1:fmEnd], "\n"), 2, &doc.Errors).(map[string]interface{}); ok {
		doc.Frontmatter = fm
	}

	i := fmEnd + 1
	var cur *Sheet
	push := func(b Block) {
		if cur != nil {
			cur.Blocks = append(cur.Blocks, b)
		} else {
			doc.WorkbookBlocks = append(doc.WorkbookBlocks, b)
		}
	}

	for i < len(lines) {
		line := lines[i]
		if strings.TrimSpace(line) == "" || strings.HasPrefix(line, ">") || docCommentRe.MatchString(line) {
			i++
			continue
		}
		if m := headingRe.FindStringSubmatch(line); m != nil {
			cur = &Sheet{Name: strings.TrimSpace(m[1]), Line: i + 1}
			doc.Sheets = append(doc.Sheets, cur)
			i++
			continue
		}
		if m := fenceOpenRe.FindStringSubmatch(line); m != nil {
			block, next := parseFence(lines, i, m, &doc.Errors)
			push(block)
			i = next
			continue
		}
		if strings.HasPrefix(line, "@ ") {
			block, next := parseAt(lines, i, &doc.Errors)
			push(block)
			i = next
			continue
		}
		snippet := line
		if len(snippet) > 60 {
			snippet = snippet[:60]
		}
		diag := Diag{Line: i + 1, Msg: "unrecognized line: " + snippet}
		if mode == "strict" {
			doc.Errors = append(doc.Errors, diag)
		} else {
			doc.Warnings = append(doc.Warnings, diag)
		}
		i++
	}
	return doc
}
