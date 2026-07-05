// Command gridmd is the GridMD reference CLI for this Go implementation:
//
//	gridmd dump <file.gmd>                 canonical model dump to stdout
//	gridmd to-xlsx <file.gmd> -o out.xlsx  export to .xlsx (loud fidelity report)
//	gridmd from-xlsx <file.xlsx> -o out.gmd import back to GridMD (self-linted)
package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/fledgling-co/gridmd/go/internal/gridmd"
	"github.com/fledgling-co/gridmd/go/internal/model"
	"github.com/fledgling-co/gridmd/go/internal/xlsxread"
	"github.com/fledgling-co/gridmd/go/internal/xlsxwrite"
)

func main() {
	os.Exit(Run(os.Args[1:], os.Stdout, os.Stderr))
}

type parsedArgs struct {
	files  []string
	output string
	strict bool
}

func parseArgs(args []string) parsedArgs {
	p := parsedArgs{}
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch {
		case a == "-o" || a == "--output":
			if i+1 < len(args) {
				i++
				p.output = args[i]
			}
		case a == "--strict":
			p.strict = true
		case strings.HasPrefix(a, "-"):
			// unknown flag: ignore
		default:
			p.files = append(p.files, a)
		}
	}
	return p
}

// Run executes the CLI and returns the process exit code.
func Run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, "usage: gridmd <dump|to-xlsx|from-xlsx> <file> [-o out]")
		return 2
	}
	verb, rest := args[0], args[1:]
	p := parseArgs(rest)
	switch verb {
	case "dump":
		return runDump(p, stdout, stderr)
	case "to-xlsx":
		return runToXlsx(p, stdout, stderr)
	case "from-xlsx":
		return runFromXlsx(p, stdout, stderr)
	default:
		fmt.Fprintf(stderr, "unknown command %q (expected dump | to-xlsx | from-xlsx)\n", verb)
		return 2
	}
}

func runDump(p parsedArgs, stdout, stderr io.Writer) int {
	if len(p.files) == 0 {
		fmt.Fprintln(stderr, "usage: gridmd dump <file.gmd>")
		return 2
	}
	input := p.files[0]
	src, err := os.ReadFile(input)
	if err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", input, err)
		return 2
	}
	out, diags, ok := gridmd.Dump(string(src))
	if !ok {
		for _, d := range diags {
			fmt.Fprintf(stderr, "%s:%d: error: %s\n", input, d.Line, d.Msg)
		}
		return 1
	}
	io.WriteString(stdout, out)
	return 0
}

func runToXlsx(p parsedArgs, stdout, stderr io.Writer) int {
	if len(p.files) == 0 {
		fmt.Fprintln(stderr, "usage: gridmd to-xlsx <file.gmd> [-o out.xlsx] [--strict]")
		return 2
	}
	input := p.files[0]
	src, err := os.ReadFile(input)
	if err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", input, err)
		return 2
	}
	res := gridmd.Lint(string(src))
	if len(res.Errors) > 0 {
		for _, d := range res.Errors {
			fmt.Fprintf(stderr, "%s:%d: error: %s\n", input, d.Line, d.Msg)
		}
		fmt.Fprintf(stderr, "%s: %d error(s) — fix the document before converting\n", input, len(res.Errors))
		return 1
	}
	data, report, err := xlsxwrite.Write(model.Build(res.Doc), src)
	if err != nil {
		fmt.Fprintf(stderr, "%s: export failed: %v\n", input, err)
		return 1
	}
	out := p.output
	if out == "" {
		out = strings.TrimSuffix(input, ".gmd") + ".xlsx"
	}
	if err := os.WriteFile(out, data, 0o644); err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", out, err)
		return 1
	}
	for _, line := range report {
		fmt.Fprintf(stdout, "%s: %s\n", input, line)
	}
	fmt.Fprintf(stdout, "%s: written (%d bytes)\n", out, len(data))
	return 0
}

func runFromXlsx(p parsedArgs, stdout, stderr io.Writer) int {
	if len(p.files) == 0 {
		fmt.Fprintln(stderr, "usage: gridmd from-xlsx <file.xlsx> [-o out.gmd]")
		return 2
	}
	input := p.files[0]
	data, err := os.ReadFile(input)
	if err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", input, err)
		return 2
	}
	src, report, err := xlsxread.Read(data)
	if err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", input, err)
		return 1
	}
	for _, line := range report {
		fmt.Fprintf(stdout, "%s: %s\n", input, line)
	}
	// The importer's output must itself be valid GridMD — self-check.
	res := gridmd.Lint(src)
	for _, d := range res.Errors {
		fmt.Fprintf(stderr, "self-check:%d: error: %s\n", d.Line, d.Msg)
	}
	out := p.output
	if out == "" {
		out = strings.TrimSuffix(strings.TrimSuffix(input, ".xlsx"), ".xlsm") + ".gmd"
	}
	if err := os.WriteFile(out, []byte(src), 0o644); err != nil {
		fmt.Fprintf(stderr, "%s: %v\n", out, err)
		return 1
	}
	status := "clean"
	if len(res.Errors) > 0 {
		status = fmt.Sprintf("FAILED (%d error(s))", len(res.Errors))
	}
	fmt.Fprintf(stdout, "%s: written — self-check %s\n", out, status)
	if len(res.Errors) > 0 {
		return 1
	}
	return 0
}
