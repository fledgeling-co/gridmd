package refs

import "testing"

func TestColRoundTrip(t *testing.T) {
	cases := map[string]int{"A": 1, "Z": 26, "AA": 27, "AB": 28, "XFD": 16384}
	for letters, num := range cases {
		if got := ColToNum(letters); got != num {
			t.Errorf("ColToNum(%q) = %d, want %d", letters, got, num)
		}
		if got := NumToCol(num); got != letters {
			t.Errorf("NumToCol(%d) = %q, want %q", num, got, letters)
		}
	}
}

func TestParseCell(t *testing.T) {
	if c := ParseCell("B2"); c == nil || c.Col != 2 || c.Row != 2 {
		t.Fatalf("ParseCell(B2) = %+v", c)
	}
	if c := ParseCell("$B$2"); c == nil || c.Col != 2 || c.Row != 2 {
		t.Fatalf("ParseCell($B$2) = %+v", c)
	}
	for _, bad := range []string{"", "B", "2", "B2:C3", "XFE1", "A1048577", "AAAA1", "b2"} {
		if c := ParseCell(bad); c != nil {
			t.Errorf("ParseCell(%q) = %+v, want nil", bad, c)
		}
	}
}

func TestParseTarget(t *testing.T) {
	cases := []struct {
		in    string
		kind  Kind
		sheet string
	}{
		{"B2", KindCell, ""},
		{"B2:D9", KindRange, ""},
		{"D9:B2", KindRange, ""}, // normalized min/max
		{"B:D", KindCols, ""},
		{"2:9", KindRows, ""},
		{"Financials!B2", KindCell, "Financials"},
		{"'Q3 Data'!B2", KindCell, "Q3 Data"},
		{"'It''s'!A1", KindCell, "It's"},
	}
	for _, c := range cases {
		got := ParseTarget(c.in)
		if got == nil || got.Kind != c.kind || got.Sheet != c.sheet {
			t.Errorf("ParseTarget(%q) = %+v, want kind=%v sheet=%q", c.in, got, c.kind, c.sheet)
		}
	}
	if got := ParseTarget("B2:D9"); got.C1 != 2 || got.R1 != 2 || got.C2 != 4 || got.R2 != 9 {
		t.Errorf("range coords = %+v", got)
	}
	for _, bad := range []string{"", "??", "A1:B2:C3", "1:B", "XFE:XFF", "1048577:1048578", "A0"} {
		if got := ParseTarget(bad); got != nil {
			t.Errorf("ParseTarget(%q) = %+v, want nil", bad, got)
		}
	}
}

func TestRefKey(t *testing.T) {
	if RefKey(3, 7) != "3,7" {
		t.Fatal("RefKey mismatch")
	}
}
