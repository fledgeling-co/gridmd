// ECMAScript `Number::toString` (base 10) for `Double`.
//
// The conformance dump requires the shortest round-tripping decimal exactly as
// JavaScript's `String(number)` produces it — `3` not `3.0`, `0.3` not `0.30`,
// `1000` not `1e3`, `100000000000000000000` not `1e+20`, `1e-7` not `1e-07`.
//
// Swift's `Double.description` already yields the *shortest round-tripping*
// significant digits (SwiftDtoa), identical to the digits ECMAScript uses — the
// two differ only in surface formatting (trailing `.0`, exponent thresholds and
// zero-padding). So we parse Swift's shortest form into (sign, significant
// digits, decimal exponent) and re-render it under the ECMAScript Number → String
// algorithm (ECMA-262 §6.1.6.1.20).

enum ESNumber {
    /// Renders `value` exactly as ECMAScript's `String(Number)` would.
    static func string(_ value: Double) -> String {
        if value == 0 { return "0" } // handles +0 and -0 (ES: String(-0) === "0")
        if value.isNaN { return "NaN" }
        if value.isInfinite { return value < 0 ? "-Infinity" : "Infinity" }

        var desc = value.description // shortest round-trip, e.g. "-12.5", "1e-07"
        var negative = false
        if desc.hasPrefix("-") {
            negative = true
            desc.removeFirst()
        }

        // Split an optional exponent.
        var mantissa = desc
        var exponent = 0
        if let eIdx = desc.firstIndex(where: { $0 == "e" || $0 == "E" }) {
            mantissa = String(desc[desc.startIndex..<eIdx])
            exponent = Int(desc[desc.index(after: eIdx)...]) ?? 0
        }

        // Split the mantissa into integer and fractional digit runs.
        var intPart = mantissa
        var fracPart = ""
        if let dot = mantissa.firstIndex(of: ".") {
            intPart = String(mantissa[..<dot])
            fracPart = String(mantissa[mantissa.index(after: dot)...])
        }

        var digits = Array(intPart + fracPart) // all ASCII 0-9
        // `pointPos` (ES `n`): number of digits left of the decimal point.
        var pointPos = intPart.count + exponent

        while digits.count > 1, digits.first == "0" {
            digits.removeFirst()
            pointPos -= 1
        }
        while digits.count > 1, digits.last == "0" {
            digits.removeLast()
        }

        let k = digits.count // significant-digit count
        let n = pointPos
        let ds = String(digits)

        let body: String
        if k <= n, n <= 21 {
            body = ds + String(repeating: "0", count: n - k)
        } else if 0 < n, n <= 21 {
            let idx = ds.index(ds.startIndex, offsetBy: n)
            body = String(ds[..<idx]) + "." + String(ds[idx...])
        } else if -6 < n, n <= 0 {
            body = "0." + String(repeating: "0", count: -n) + ds
        } else {
            let e = n - 1
            let mant = k == 1
                ? String(digits[0])
                : "\(digits[0])." + String(digits[1...])
            body = mant + "e" + (e >= 0 ? "+" : "-") + String(abs(e))
        }

        return negative ? "-" + body : body
    }
}
