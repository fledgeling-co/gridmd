// Package yamlsubset decodes the GridMD YAML safe subset (SPEC §3, §10): maps,
// lists, flow collections, block scalars, and plain/quoted scalars — no
// anchors, aliases, or custom tags. Decoding goes through yaml.Node so map keys
// always come from the key node's raw text (never coerced to bool/int/time
// keys), mirroring the JS reference where object keys are always strings.
package yamlsubset

import (
	"regexp"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

var identKeyRe = regexp.MustCompile(`^(x-)?[a-z][a-z0-9-]*$`)

// implicitTags are the tags yaml.v3 resolves automatically for the core schema
// (plus timestamp). Any other explicit tag is outside the GridMD safe subset.
var implicitTags = map[string]bool{
	"":            true,
	"!!str":       true,
	"!!int":       true,
	"!!float":     true,
	"!!bool":      true,
	"!!null":      true,
	"!!seq":       true,
	"!!map":       true,
	"!!merge":     true,
	"!!timestamp": true,
}

// Decode parses a YAML fragment into a JS-like value (map[string]interface{},
// []interface{}, string, float64, bool, or nil). Any diagnostics (syntax
// errors, aliases, custom tags, duplicate keys) are returned as messages.
func Decode(text string) (interface{}, []string) {
	var errs []string
	if strings.TrimSpace(text) == "" {
		return map[string]interface{}{}, errs
	}
	var doc yaml.Node
	if err := yaml.Unmarshal([]byte(text), &doc); err != nil {
		msg := firstLine(err.Error())
		return map[string]interface{}{}, []string{"YAML: " + msg}
	}
	scanSubset(&doc, &errs)
	if doc.Kind == 0 || len(doc.Content) == 0 {
		return map[string]interface{}{}, errs
	}
	return convert(doc.Content[0], &errs), errs
}

// TryProps trials a fragment as an @-directive flow-map props candidate: it must
// parse cleanly to a mapping whose every top-level key is an identifier and
// whose every value is non-null (SPEC Appendix A, props rule).
func TryProps(text string) (map[string]interface{}, bool) {
	var doc yaml.Node
	if err := yaml.Unmarshal([]byte(text), &doc); err != nil {
		return nil, false
	}
	if doc.Kind == 0 || len(doc.Content) == 0 {
		return nil, false
	}
	var errs []string
	v := convert(doc.Content[0], &errs)
	if len(errs) > 0 {
		return nil, false
	}
	m, ok := v.(map[string]interface{})
	if !ok {
		return nil, false
	}
	for k, val := range m {
		if !identKeyRe.MatchString(k) || val == nil {
			return nil, false
		}
	}
	return m, true
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i != -1 {
		return s[:i]
	}
	return s
}

func scanSubset(n *yaml.Node, errs *[]string) {
	if n.Kind == yaml.AliasNode {
		*errs = append(*errs, "YAML anchors/aliases are outside the GridMD safe subset")
	} else if n.Tag != "" && !implicitTags[n.Tag] {
		*errs = append(*errs, "YAML tags are outside the GridMD safe subset")
	}
	for _, c := range n.Content {
		scanSubset(c, errs)
	}
}

func convert(n *yaml.Node, errs *[]string) interface{} {
	switch n.Kind {
	case yaml.MappingNode:
		m := make(map[string]interface{}, len(n.Content)/2)
		for i := 0; i+1 < len(n.Content); i += 2 {
			key := n.Content[i].Value
			if _, dup := m[key]; dup {
				*errs = append(*errs, "YAML: duplicate key "+key)
			}
			m[key] = convert(n.Content[i+1], errs)
		}
		return m
	case yaml.SequenceNode:
		out := make([]interface{}, 0, len(n.Content))
		for _, c := range n.Content {
			out = append(out, convert(c, errs))
		}
		return out
	case yaml.ScalarNode:
		return convertScalar(n)
	case yaml.AliasNode:
		return convert(n.Alias, errs)
	}
	return nil // unreachable for the document nodes Decode feeds in
}

func convertScalar(n *yaml.Node) interface{} {
	if n.Style&(yaml.SingleQuotedStyle|yaml.DoubleQuotedStyle) != 0 {
		return n.Value
	}
	switch n.Tag {
	case "!!null":
		return nil
	case "!!bool":
		return strings.EqualFold(n.Value, "true")
	case "!!int", "!!float":
		if f, err := strconv.ParseFloat(n.Value, 64); err == nil {
			return f
		}
		return n.Value // non-numeric under a numeric tag: keep raw text
	default:
		return n.Value
	}
}
