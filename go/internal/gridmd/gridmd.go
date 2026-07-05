// Package gridmd ties parsing and validation together (mirrors js/src/index.js
// `lint`) and exposes the canonical dump entry point.
package gridmd

import (
	"sort"

	"github.com/fledgling-co/gridmd/go/internal/dump"
	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/parser"
	"github.com/fledgling-co/gridmd/go/internal/validate"
)

// Result is a lint outcome: the parsed document plus line-sorted diagnostics.
type Result struct {
	Doc      *parser.Document
	Errors   []parser.Diag
	Warnings []parser.Diag
}

// Lint parses and validates source in strict mode.
func Lint(source string) *Result {
	doc := parser.Parse(source, "strict")
	validate.Validate(doc)
	errs := append([]parser.Diag(nil), doc.Errors...)
	sort.SliceStable(errs, func(i, j int) bool { return errs[i].Line < errs[j].Line })
	// Strict-mode validation emits no warnings, so warnings pass through as-is.
	return &Result{Doc: doc, Errors: errs, Warnings: append([]parser.Diag(nil), doc.Warnings...)}
}

// Dump lints source and, when clean, returns its canonical model dump. When the
// document has errors, ok is false and the diagnostics are returned.
func Dump(source string) (out string, diags []parser.Diag, ok bool) {
	res := Lint(source)
	if len(res.Errors) > 0 {
		return "", res.Errors, false
	}
	return dump.Model(model.Build(res.Doc)), nil, true
}
