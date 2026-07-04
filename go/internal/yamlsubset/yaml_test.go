package yamlsubset

import (
	"reflect"
	"testing"

	"gopkg.in/yaml.v3"
)

func TestDecodeEmptyAndComment(t *testing.T) {
	for _, in := range []string{"", "   ", "# only a comment"} {
		v, errs := Decode(in)
		if len(errs) != 0 {
			t.Errorf("Decode(%q) errs=%v", in, errs)
		}
		if m, ok := v.(map[string]interface{}); !ok || len(m) != 0 {
			t.Errorf("Decode(%q) = %#v, want empty map", in, v)
		}
	}
}

func TestDecodeScalarsAndCollections(t *testing.T) {
	v, errs := Decode("a: 1\nb: 2.5\nc: true\nd: hello\ne: \"7\"\nf: null\ng: [x, y]\nh: 2026-07-04")
	if len(errs) != 0 {
		t.Fatalf("errs=%v", errs)
	}
	m := v.(map[string]interface{})
	if m["a"] != float64(1) || m["b"] != 2.5 || m["c"] != true || m["d"] != "hello" || m["e"] != "7" {
		t.Errorf("scalars = %#v", m)
	}
	if m["f"] != nil {
		t.Errorf("null = %#v", m["f"])
	}
	if !reflect.DeepEqual(m["g"], []interface{}{"x", "y"}) {
		t.Errorf("seq = %#v", m["g"])
	}
	if m["h"] != "2026-07-04" { // timestamp kept as raw string
		t.Errorf("timestamp = %#v", m["h"])
	}
}

func TestDecodeDiagnostics(t *testing.T) {
	if _, errs := Decode("{ unterminated"); len(errs) == 0 {
		t.Error("expected a syntax error")
	}
	if _, errs := Decode("a: &x 1\nb: *x"); len(errs) == 0 {
		t.Error("expected an alias error")
	}
	if _, errs := Decode("v: !!binary aGk="); len(errs) == 0 {
		t.Error("expected a tag error")
	}
	if _, errs := Decode("a: 1\na: 2"); len(errs) == 0 {
		t.Error("expected a duplicate-key diagnostic")
	}
}

func TestConvertDefaultAndScalarFallback(t *testing.T) {
	var errs []string
	if v := convert(&yaml.Node{}, &errs); v != nil {
		t.Errorf("convert(zero node) = %#v, want nil", v)
	}
	// A non-numeric value under a numeric tag keeps its raw text.
	if v := convertScalar(&yaml.Node{Kind: yaml.ScalarNode, Tag: "!!int", Value: "0xZZ"}); v != "0xZZ" {
		t.Errorf("int fallback = %#v", v)
	}
}

func TestTryProps(t *testing.T) {
	if m, ok := TryProps(`{ merge: true, style: hdr, x-foo: 1 }`); !ok || m["merge"] != true || m["style"] != "hdr" {
		t.Errorf("valid props = %#v ok=%v", m, ok)
	}
	for _, bad := range []string{
		`{ unterminated`, // parse error
		`# comment only`, // no content node
		`{ a: 1, a: 1 }`, // convert diagnostic (duplicate key)
		`[1, 2]`,         // sequence, not a mapping
		`hello`,          // scalar, not a mapping
		`{ "AU", "NZ" }`, // non-ident keys (array constant shape)
		`{ Foo: 1 }`,     // uppercase key
		`{ note: }`,      // null value
	} {
		if _, ok := TryProps(bad); ok {
			t.Errorf("TryProps(%q) accepted, want reject", bad)
		}
	}
}

func TestFirstLine(t *testing.T) {
	if firstLine("one\ntwo") != "one" {
		t.Error("firstLine multi")
	}
	if firstLine("single") != "single" {
		t.Error("firstLine single")
	}
}
