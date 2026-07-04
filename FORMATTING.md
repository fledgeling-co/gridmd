# GridMD Formatting Reference

**Version 0.1 (draft).** Normative companion to [SPEC.md](SPEC.md) §9.3.
Defines property value syntax, number formats, colors and themes, and the
built-in style catalogs.

---

## 1. Font, fill, border and alignment values

### Font

```yaml
font: "Inter"                # family name; "major"/"minor" reference the theme fonts
size: 11                     # points
bold: true
italic: true
underline: true              # true(=single) | double | single-accounting | double-accounting
strike: true
sub: true                    # subscript (sub and super are mutually exclusive)
super: true
color: "#0F1A2E"             # see §3
```

### Fill

```yaml
fill: "#EEF1F6"              # solid fill
pattern: gray-125            # optional: none|solid|gray-750|gray-500|gray-250|gray-125|
fill2: "#FFFFFF"             #   gray-0625|dark-horizontal|dark-vertical|dark-down|dark-up|
                             #   dark-grid|dark-trellis|light-* variants; fill2 = pattern bg
```

Gradient cell fills ride the escape hatch (rare in practice).

### Borders

Edge value = `"<style> <color>"` shorthand or a map:

```yaml
border: "thin #D6D9E0"                 # all four edges
border-top: { style: double, color: accent1 }
border-bottom: "medium #0F1A2E"
border-diag-down: "thin #C0392B"       # and border-diag-up
```

On a **range** target, `border:` draws the outline and `border-inner:` the
inside grid (`border-inner-h`/`border-inner-v` for one direction).

Styles: `hair thin medium thick double dotted dashed dash-dot dash-dot-dot
medium-dashed medium-dash-dot medium-dash-dot-dot slant-dash-dot none`.

### Alignment

```yaml
align: center        # left | center | right | justify | fill | center-across | distributed
valign: middle       # top | middle | bottom | justify | distributed
rotation: 45         # -90..90 degrees, or `vertical` (stacked letters)
indent: 2            # indent units (Excel indent steps)
wrap: true
shrink: true         # shrink-to-fit (mutually exclusive with wrap)
```

## 2. Number formats

`numfmt` takes either a raw **Excel format code** (the portable ground truth)
or a built-in **alias**:

| Alias | Code |
|---|---|
| `general` | `General` |
| `number` | `0.00` |
| `comma` | `#,##0.00` |
| `comma-0` | `#,##0` |
| `currency` | `$#,##0.00` |
| `currency-0` | `$#,##0` |
| `accounting` | `_($* #,##0.00_);_($* (#,##0.00);_($* "-"??_);_(@_)` |
| `percent` | `0.00%` |
| `percent-0` | `0%` |
| `scientific` | `0.00E+00` |
| `fraction` | `# ?/?` |
| `short-date` | `m/d/yyyy` |
| `long-date` | `dddd, mmmm d, yyyy` |
| `time` | `h:mm:ss AM/PM` |
| `text` | `@` |

- Currency symbols other than `$` are written literally in the code
  (`[$£-en-GB]#,##0.00`, `[$€-x-euro2]#,##0.00 `) — exactly Excel's locale-tagged
  code syntax. Aliases exist for authoring comfort; the code is what round-trips.
- `numfmt: text` (`@`) suppresses numeric coercion: `@ A2 '00042 { numfmt: text }`
  keeps the leading zeros (the `'` prefix already forces text; the format keeps
  Excel honest on re-entry).
- Format codes are stored verbatim, including sections
  (`positive;negative;zero;text`), color tags (`[Red]`), and conditions
  (`[>=1000]`).

## 3. Colors

Three forms, usable anywhere a color is accepted:

| Form | Example | Meaning |
|---|---|---|
| Hex | `"#1F3FA6"` | sRGB. Optional alpha: `"#1F3FA680"` (objects/charts only) |
| Theme slot | `accent1` | One of the 12 theme slots (§4) |
| Theme + tint/shade | `accent1@40`, `accent1@-25` | `@N` = N % lighter (tint); `@-N` = N % darker (shade) |

`auto` is accepted for border/font colors (application default).

## 4. Theme

Frontmatter `theme:` fills any of the 12 OOXML color slots and 2 font slots;
unlisted slots inherit the Office defaults:

```yaml
theme:
  colors:
    dk1: "#000000"      # text 1        lt1: "#FFFFFF"   # background 1
    dk2: "#0F1A2E"      # text 2        lt2: "#EEF1F6"   # background 2
    accent1: "#1F3FA6"
    accent2: "#63BE7B"
    accent3: "#FFB547"
    accent4: "#C0392B"
    accent5: "#5E6A82"
    accent6: "#8E7CC3"
    hlink: "#1F3FA6"
    folHlink: "#8E7CC3"
  fonts: { major: Inter, minor: Inter }
```

## 5. Named styles & the built-in cell-style catalog

Frontmatter `styles:` defines named styles (any cell/range props except
content-related ones). A cell's `style:` applies the named style first, then
explicit props override. Styles may `extend:` another style:

```yaml
styles:
  money:      { numfmt: "$#,##0.00" }
  money-bold: { extend: money, bold: true }
```

The **built-in catalog** (Excel's Cell Styles gallery) is available without
declaration. Applying one is `style: <name>`; converters map them to the
equivalent built-in style so themes restyle them:

| Group | Names |
|---|---|
| Good/Bad/Neutral | `normal` `good` `bad` `neutral` |
| Data & Model | `calculation` `check` `explanatory` `input` `linked` `note` `output` `warning` `followed-hyperlink` `hyperlink` |
| Titles & Headings | `title` `heading-1` `heading-2` `heading-3` `heading-4` `total` |
| Themed | `accent1` … `accent6`, `accent1@20` `accent1@40` `accent1@60` (… per accent) |
| Number format | `comma` `comma-0` `currency` `currency-0` `percent` |

(Themed cell styles reuse the color tint syntax: `style: accent3@40` = "40 %
Accent 3".)

Namespace notes: built-in names occupy the `style:` **value** position only,
so they never collide with property keys like `note:` — `style: note` and
`note: "text"` coexist on one cell. `comma`/`currency`/`percent` also exist as
`numfmt:` aliases (§2); again a different key, no conflict. A user-defined
frontmatter style MAY shadow a built-in name — the user definition wins.

## 6. Table styles

`{table}` `style:` accepts the built-in banded catalog — `light-1` … `light-21`,
`medium-1` … `medium-28`, `dark-1` … `dark-11` (mapping to Excel's
`TableStyleLight1`…) — or a custom table style declared in frontmatter:

```yaml
table-styles:
  diolog:
    whole:   { border: "thin #D6D9E0" }
    header:  { bold: true, color: "#FFFFFF", fill: accent1 }
    band1:   { fill: "#F7F8FB" }        # odd body band
    band2:   { fill: "#FFFFFF" }
    total:   { bold: true, border-top: "double #0F1A2E" }
    first-col: { bold: true }
    last-col:  { bold: true }
```

## 7. Icon-set catalog (conditional formatting)

`icons:` in `{cf}` accepts:

`3-arrows` `3-arrows-gray` `3-flags` `3-traffic-lights` `3-traffic-lights-rimmed`
`3-signs` `3-symbols` `3-symbols-circled` `3-stars` `3-triangles`
`4-arrows` `4-arrows-gray` `4-red-to-black` `4-ratings` `4-traffic-lights`
`5-arrows` `5-arrows-gray` `5-ratings` `5-quarters` `5-boxes`

Default thresholds split the range into equal percent bands; override with
`steps:` (DIRECTIVES.md §2). Per-icon substitution (mixed sets) rides
`fallback:`.

## 8. Property precedence

For any visual attribute of a cell, later wins:

1. Sheet defaults (`{sheet}` `default-*`, column `style:`)
2. Row/column props (`rows:`/`cols:` maps)
3. Table style layers (whole → band → header/total/first/last)
4. Named style on the cell (`style:`, after `extend:` resolution)
5. Explicit cell/range props (`@` directives; among overlapping `@` range
   directives, document order — last write wins)
6. Conditional formatting (visual override only; never mutates stored props)

This matches Excel's effective-format model: CF paints over manual fills
without erasing them.
