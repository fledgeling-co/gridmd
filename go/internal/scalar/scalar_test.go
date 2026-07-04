package scalar

import "testing"

func TestParseKinds(t *testing.T) {
	cases := []struct {
		raw  string
		kind Kind
	}{
		{"", Blank},
		{"Plain text", Text},
		{`"Quoted"`, Text},
		{"'forced", Text},
		{"0.3", Number},
		{"-12.5", Number},
		{"1e3", Number},
		{"TRUE", Boolean},
		{"false", Boolean},
		{"2026-07-04", Date},
		{"2026-07-04T06:00", Date},
		{"12:30", Time},
		{"#DIV/0!", Error},
		{"=A1*2", Formula},
		{"{=TRANSPOSE(A1:B2)}", Formula},
	}
	for _, c := range cases {
		if got := Parse(c.raw); got.Kind != c.kind {
			t.Errorf("Parse(%q).Kind = %q, want %q", c.raw, got.Kind, c.kind)
		}
	}
}

func TestParseDetails(t *testing.T) {
	if s := Parse(`"a""b"`); s.Str != `a"b` || !s.Quoted {
		t.Errorf(`quoted "" escape = %+v`, s)
	}
	if s := Parse("'TRUE"); s.Str != "TRUE" || !s.Forced {
		t.Errorf("forced text = %+v", s)
	}
	if s := Parse("1e3"); s.Num != 1000 {
		t.Errorf("number = %v", s.Num)
	}
	if s := Parse("false"); s.Bool {
		t.Error("false should be Bool=false")
	}
	if s := Parse("#N/A"); s.Str != "#N/A" {
		t.Errorf("error value = %q", s.Str)
	}
	if s := Parse("#n/a"); s.Kind != Error || s.Str != "#N/A" {
		t.Errorf("error value case-insensitive = %+v", s)
	}
}

func TestFormulaAndCached(t *testing.T) {
	s := Parse("=B1*10 :: 3")
	if s.Kind != Formula || s.FValue != "B1*10" || s.Cached == nil || s.Cached.Kind != Number || s.Cached.Num != 3 {
		t.Fatalf("formula+cached = %+v cached=%+v", s, s.Cached)
	}
	s = Parse(`=IF(B1>0,"x :: y","z") :: "x :: y"`)
	if s.FValue != `IF(B1>0,"x :: y","z")` || s.Cached == nil || s.Cached.Str != "x :: y" {
		t.Fatalf("quote-aware split = %q cached=%+v", s.FValue, s.Cached)
	}
	s = Parse("=A1")
	if s.Cached != nil {
		t.Error("no separator should leave cached nil")
	}
	s = Parse("=A1 :: ")
	if s.Cached == nil || s.Cached.Kind != Blank {
		t.Errorf("empty cached should be a blank scalar, got %+v", s.Cached)
	}
	s = Parse("=A1 :: =B1")
	if s.Cached == nil || s.Cached.Kind != Invalid {
		t.Errorf("formula cached should be invalid, got %+v", s.Cached)
	}
}

func TestCSE(t *testing.T) {
	s := Parse("{=SUM(A1:A3)}")
	if s.Kind != Formula || !s.CSE || s.FValue != "SUM(A1:A3)" {
		t.Fatalf("cse = %+v", s)
	}
	s = Parse("{=SUM(A1:A3)")
	if s.Kind != Text || s.Problem == "" {
		t.Errorf("unterminated cse = %+v", s)
	}
}

func TestUnterminatedQuote(t *testing.T) {
	s := Parse(`"open`)
	if s.Kind != Text || s.Problem == "" {
		t.Errorf("unterminated quote = %+v", s)
	}
}

func TestSplitCached(t *testing.T) {
	head, cached, ok := SplitCached("a :: b")
	if !ok || head != "a" || cached != "b" {
		t.Errorf("split = %q %q %v", head, cached, ok)
	}
	if _, _, ok := SplitCached("no separator"); ok {
		t.Error("expected no separator")
	}
}
