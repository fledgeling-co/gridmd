// Package numfmt formats an IEEE-754 double exactly as ECMAScript's
// Number::toString does (SPEC §6; conformance/README.md): shortest round-trip
// decimal, integer-valued doubles without a decimal point, fixed notation
// inside the standard range and exponential outside it.
package numfmt

import (
	"math"
	"strconv"
	"strings"
)

// Format renders f using ECMAScript Number-to-String semantics.
func Format(f float64) string {
	if f == 0 {
		return "0" // covers +0 and -0
	}
	if math.IsNaN(f) {
		return "NaN"
	}
	if math.IsInf(f, 1) {
		return "Infinity"
	}
	if math.IsInf(f, -1) {
		return "-Infinity"
	}

	sign := ""
	if f < 0 {
		sign = "-"
		f = -f
	}

	// Shortest round-trip digits + decimal exponent via the 'e' form.
	e := strconv.FormatFloat(f, 'e', -1, 64) // e.g. "3.771e-01"
	mantissa, expStr, _ := strings.Cut(e, "e")
	digits := strings.Replace(mantissa, ".", "", 1)
	exp, _ := strconv.Atoi(expStr) // power of ten of the first digit
	k := len(digits)               // significant digit count
	n := exp + 1                   // decimal point position (ES §Number::toString)

	switch {
	case k <= n && n <= 21:
		return sign + digits + strings.Repeat("0", n-k)
	case 0 < n && n <= 21:
		return sign + digits[:n] + "." + digits[n:]
	case -6 < n && n <= 0:
		return sign + "0." + strings.Repeat("0", -n) + digits
	default:
		return sign + exponential(digits, k, n)
	}
}

func exponential(digits string, k, n int) string {
	head := digits[:1]
	if k > 1 {
		head += "." + digits[1:]
	}
	e := n - 1
	if e >= 0 {
		return head + "e+" + strconv.Itoa(e)
	}
	return head + "e-" + strconv.Itoa(-e)
}
