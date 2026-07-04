// Package scalar implements the cell scalar micro-grammar (SPEC.md §6).
package scalar

import (
	"regexp"
	"strconv"
	"strings"
)

// ErrorValues is the closed set of spreadsheet error literals (SPEC §6 rule 9).
var ErrorValues = map[string]bool{
	"#NULL!": true, "#DIV/0!": true, "#VALUE!": true, "#REF!": true,
	"#NAME?": true, "#NUM!": true, "#N/A": true, "#GETTING_DATA": true,
	"#SPILL!": true, "#CALC!": true, "#FIELD!": true, "#BLOCKED!": true,
}

var (
	numberRe = regexp.MustCompile(`^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?$`)
	dateRe   = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?)?$`)
	timeRe   = regexp.MustCompile(`^\d{2}:\d{2}(:\d{2})?$`)
	dqRe     = regexp.MustCompile(`^"((?:[^"]|"")*)"$`)
)

// Kind discriminates a parsed scalar.
type Kind string

// Scalar kinds.
const (
	Blank   Kind = "blank"
	Number  Kind = "number"
	Boolean Kind = "boolean"
	Error   Kind = "error"
	Date    Kind = "date"
	Time    Kind = "time"
	Text    Kind = "text"
	Formula Kind = "formula"
	Invalid Kind = "invalid"
)

// Scalar is a parsed cell value.
type Scalar struct {
	Kind Kind

	// Number
	Num float64
	// Boolean
	Bool bool
	// Text / Error / Date / Time: the string value.
	Str string

	// Formula
	CSE    bool
	FValue string  // formula text (without the leading = or {= }=)
	Cached *Scalar // cached side of a formula (may be nil)

	// Text flags
	Forced bool
	Quoted bool

	// Diagnostics
	Problem string
}

// SplitCached splits "formula :: cached" at the LAST " :: " outside
// double-quoted string literals (SPEC §6). The second return is "" and ok=false
// when there is no separator.
func SplitCached(text string) (head, cached string, ok bool) {
	inQ := false
	idx := -1
	for i := 0; i < len(text); i++ {
		ch := text[i]
		if ch == '"' {
			inQ = !inQ
			continue
		}
		if !inQ && ch == ' ' && strings.HasPrefix(text[i:], " :: ") {
			idx = i
		}
	}
	if idx == -1 {
		return text, "", false
	}
	return text[:idx], strings.TrimSpace(text[idx+4:]), true
}

// Parse parses one cell scalar. raw must already be trimmed.
func Parse(raw string) *Scalar {
	if raw == "" {
		return &Scalar{Kind: Blank}
	}
	if strings.HasPrefix(raw, "{=") {
		head, cached, ok := SplitCached(raw)
		if !strings.HasSuffix(head, "}") {
			return &Scalar{Kind: Text, Str: raw, Problem: "unterminated CSE array formula"}
		}
		return &Scalar{Kind: Formula, CSE: true, FValue: head[2 : len(head)-1], Cached: parseCached(cached, ok)}
	}
	if strings.HasPrefix(raw, "=") {
		head, cached, ok := SplitCached(raw)
		return &Scalar{Kind: Formula, CSE: false, FValue: head[1:], Cached: parseCached(cached, ok)}
	}
	if strings.HasPrefix(raw, "'") {
		return &Scalar{Kind: Text, Str: raw[1:], Forced: true}
	}
	if strings.HasPrefix(raw, `"`) {
		if m := dqRe.FindStringSubmatch(raw); m != nil {
			return &Scalar{Kind: Text, Str: strings.ReplaceAll(m[1], `""`, `"`), Quoted: true}
		}
		return &Scalar{Kind: Text, Str: raw, Problem: "unterminated quoted text"}
	}
	if numberRe.MatchString(raw) {
		f, _ := strconv.ParseFloat(raw, 64)
		return &Scalar{Kind: Number, Num: f}
	}
	up := strings.ToUpper(raw)
	if up == "TRUE" || up == "FALSE" {
		return &Scalar{Kind: Boolean, Bool: up == "TRUE"}
	}
	if dateRe.MatchString(raw) {
		return &Scalar{Kind: Date, Str: raw}
	}
	if timeRe.MatchString(raw) {
		return &Scalar{Kind: Time, Str: raw}
	}
	if ErrorValues[up] {
		return &Scalar{Kind: Error, Str: up}
	}
	return &Scalar{Kind: Text, Str: raw}
}

// parseCached parses the cached side; a formula there is rejected as invalid.
// present is false when the source carried no " :: " separator at all, in which
// case there is no cached scalar (nil), mirroring the JS null return.
func parseCached(cachedText string, present bool) *Scalar {
	if !present {
		return nil
	}
	v := Parse(cachedText)
	if v.Kind == Formula {
		return &Scalar{Kind: Invalid, Problem: "cached value must not be a formula"}
	}
	return v
}
