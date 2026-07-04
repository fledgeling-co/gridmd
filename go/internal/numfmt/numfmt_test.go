package numfmt

import "testing"

func TestFormat(t *testing.T) {
	cases := []struct {
		in   float64
		want string
	}{
		{0, "0"},
		{-0.0, "0"},
		{3, "3"},
		{84, "84"},
		{1000, "1000"},
		{-12.5, "-12.5"},
		{0.3, "0.3"},
		{0.66, "0.66"},
		{19.5, "19.5"},
		{442.1, "442.1"},
		{987.8, "987.8"},
		{0.3771428571428571, "0.3771428571428571"},
		{37.77777777777778, "37.77777777777778"},
		{0.000001, "0.000001"}, // 1e-6 → -6 < n <= 0
		{1e21, "1e+21"},        // exponential, k==1, positive
		{1.23e21, "1.23e+21"},  // exponential, k>1, positive
		{1e-7, "1e-7"},         // exponential, k==1, negative
		{1.5e-8, "1.5e-8"},     // exponential, k>1, negative
		{-1e21, "-1e+21"},      // sign + exponential
	}
	for _, c := range cases {
		if got := Format(c.in); got != c.want {
			t.Errorf("Format(%v) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestFormatNonFinite(t *testing.T) {
	inf := 1.0
	for _, c := range []struct {
		in   float64
		want string
	}{
		{inf / 0, "Infinity"},
		{-inf / 0, "-Infinity"},
		{(inf / 0) - (inf / 0), "NaN"},
	} {
		if got := Format(c.in); got != c.want {
			t.Errorf("Format non-finite = %q, want %q", got, c.want)
		}
	}
}
