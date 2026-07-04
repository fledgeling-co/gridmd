//! GridMD → XLSX. Emits a genuine worksheet core (cells, formulas + cached
//! values, merges, hidden state) natively, and carries the FULL original GridMD
//! source in a custom package part (`gridmd/source.gmd`) so nothing is ever
//! dropped (SPEC §11) and the round-trip is byte-exact. The importer restores
//! the model from that carry part (see `read.rs`); the native cells make the
//! file a real, openable spreadsheet.

use crate::dump::format_number;
use crate::model::{CellModel, Content, Model, SheetModel};
use crate::refs::num_to_col;
use crate::scalar::Scalar;
use crate::xlsx::zip::{zip_write, ZipEntry};
use crate::yaml::Yaml;

pub const CARRY_PART: &str = "gridmd/source.gmd";

/// ISO date/time → Excel serial per date-system (INTEROP §2). Port of
/// `units.js` `isoToSerial`, including the 1900 phantom-leap-day correction.
pub fn iso_to_serial(iso: &str, date_system: i64) -> f64 {
    let (date_part, time_part): (Option<&str>, Option<&str>) =
        if iso.len() >= 3 && &iso[2..3] == ":" {
            (None, Some(iso))
        } else {
            match iso.split_once('T') {
                Some((d, t)) => (Some(d), Some(t)),
                None => (Some(iso), None),
            }
        };
    let mut frac = 0.0;
    if let Some(t) = time_part {
        let mut it = t.split(':');
        let hh: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
        let mm: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
        let ss: f64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
        frac = (hh * 3600.0 + mm * 60.0 + ss) / 86400.0;
    }
    let date_part = match date_part {
        Some(d) => d,
        None => return frac,
    };
    let mut parts = date_part.split('-');
    let y: i64 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(1900);
    let m: i64 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(1);
    let d: i64 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(1);
    let utc = days_from_civil(y, m, d);
    let mut days = if date_system == 1904 {
        (utc - days_from_civil(1904, 1, 1)) as f64
    } else {
        let dd = (utc - days_from_civil(1899, 12, 30)) as f64;
        if dd < 61.0 {
            dd - 1.0
        } else {
            dd
        }
    };
    days += frac;
    days
}

/// Days since the Unix epoch (proleptic Gregorian) — Howard Hinnant's algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn scalar_display(s: &Scalar) -> String {
    match s {
        Scalar::Number(n) => format_number(*n),
        Scalar::Boolean(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Scalar::Error(e) => e.clone(),
        Scalar::Date(d) | Scalar::Time(d) => d.clone(),
        Scalar::Text { value, .. } => value.clone(),
        Scalar::Blank => String::new(),
        Scalar::Formula { formula, .. } => formula.clone(),
    }
}

struct SharedStrings {
    list: Vec<String>,
    index: std::collections::HashMap<String, usize>,
}

impl SharedStrings {
    fn new() -> Self {
        SharedStrings {
            list: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }
    fn intern(&mut self, s: &str) -> usize {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.list.len();
        self.list.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

fn cell_xml(c: &CellModel, ss: &mut SharedStrings, date_system: i64, out: &mut String) {
    let content = match &c.content {
        Some(c) => c,
        None => return,
    };
    let refr = format!("{}{}", num_to_col(c.col), c.row);
    if let Some(rich) = &content.rich {
        let idx = ss.intern(rich);
        out.push_str(&format!("<c r=\"{refr}\" t=\"s\"><v>{idx}</v></c>"));
        return;
    }
    if let Some(formula) = &content.formula {
        formula_cell_xml(&refr, formula, content, out);
        return;
    }
    match &content.scalar {
        Some(Scalar::Number(n)) => {
            out.push_str(&format!("<c r=\"{refr}\"><v>{}</v></c>", format_number(*n)));
        }
        Some(Scalar::Boolean(b)) => {
            out.push_str(&format!(
                "<c r=\"{refr}\" t=\"b\"><v>{}</v></c>",
                if *b { 1 } else { 0 }
            ));
        }
        Some(Scalar::Error(e)) => {
            out.push_str(&format!(
                "<c r=\"{refr}\" t=\"e\"><v>{}</v></c>",
                xml_escape(e)
            ));
        }
        Some(Scalar::Date(d)) | Some(Scalar::Time(d)) => {
            let serial = iso_to_serial(d, date_system);
            out.push_str(&format!(
                "<c r=\"{refr}\" s=\"1\"><v>{}</v></c>",
                format_number(serial)
            ));
        }
        Some(Scalar::Text { value, .. }) => {
            let idx = ss.intern(value);
            out.push_str(&format!("<c r=\"{refr}\" t=\"s\"><v>{idx}</v></c>"));
        }
        _ => {
            // Blank / defensive: emit an empty inline cell.
            out.push_str(&format!("<c r=\"{refr}\"/>"));
        }
    }
}

fn formula_cell_xml(refr: &str, formula: &str, content: &Content, out: &mut String) {
    let f = xml_escape(formula);
    match &content.cached {
        None => out.push_str(&format!("<c r=\"{refr}\"><f>{f}</f></c>")),
        Some(Scalar::Number(n)) => out.push_str(&format!(
            "<c r=\"{refr}\"><f>{f}</f><v>{}</v></c>",
            format_number(*n)
        )),
        Some(other) => {
            let disp = xml_escape(&scalar_display(other));
            out.push_str(&format!(
                "<c r=\"{refr}\" t=\"str\"><f>{f}</f><v>{disp}</v></c>"
            ));
        }
    }
}

fn worksheet_xml(s: &SheetModel, ss: &mut SharedStrings, date_system: i64) -> String {
    let mut cells: Vec<&CellModel> = s.cells.iter().filter(|c| c.content.is_some()).collect();
    cells.sort_by(|a, b| a.row.cmp(&b.row).then(a.col.cmp(&b.col)));

    let mut body = String::new();
    body.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    // dimension
    if let (Some(first), Some(last)) = (cells.first(), cells.last()) {
        let min_col = cells.iter().map(|c| c.col).min().unwrap_or(1);
        let max_col = cells.iter().map(|c| c.col).max().unwrap_or(1);
        let dim = format!(
            "{}{}:{}{}",
            num_to_col(min_col),
            first.row,
            num_to_col(max_col),
            last.row
        );
        body.push_str(&format!("<dimension ref=\"{dim}\"/>"));
    } else {
        body.push_str("<dimension ref=\"A1\"/>");
    }
    body.push_str("<sheetData>");
    let mut i = 0;
    while i < cells.len() {
        let row = cells[i].row;
        body.push_str(&format!("<row r=\"{row}\">"));
        while i < cells.len() && cells[i].row == row {
            cell_xml(cells[i], ss, date_system, &mut body);
            i += 1;
        }
        body.push_str("</row>");
    }
    body.push_str("</sheetData>");
    if !s.merges.is_empty() {
        body.push_str(&format!("<mergeCells count=\"{}\">", s.merges.len()));
        for m in &s.merges {
            let refr = format!(
                "{}{}:{}{}",
                num_to_col(m.c1),
                m.r1,
                num_to_col(m.c2),
                m.r2
            );
            body.push_str(&format!("<mergeCell ref=\"{refr}\"/>"));
        }
        body.push_str("</mergeCells>");
    }
    body.push_str("</worksheet>");
    body
}

fn shared_strings_xml(ss: &SharedStrings) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{0}\" uniqueCount=\"{0}\">",
        ss.list.len()
    );
    for s in &ss.list {
        out.push_str(&format!("<si><t xml:space=\"preserve\">{}</t></si>", xml_escape(s)));
    }
    out.push_str("</sst>");
    out
}

fn styles_xml() -> String {
    // One date cell format (numFmtId 14) at cellXfs index 1.
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"1\"><fill><patternFill patternType=\"none\"/></fill></fills><borders count=\"1\"><border/></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"14\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>".to_string()
}

fn workbook_xml(model: &Model) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets>",
    );
    for (i, s) in model.sheets.iter().enumerate() {
        let state = match s.meta.get("hidden") {
            Some(Yaml::Bool(true)) => " state=\"hidden\"",
            Some(Yaml::Str(v)) if v == "very" => " state=\"veryHidden\"",
            _ => "",
        };
        out.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"{}/>",
            xml_escape(&s.name),
            i + 1,
            i + 1,
            state
        ));
    }
    out.push_str("</sheets></workbook>");
    out
}

fn workbook_rels_xml(model: &Model) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for i in 0..model.sheets.len() {
        out.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",
            i + 1,
            i + 1
        ));
    }
    let n = model.sheets.len();
    out.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>",
        n + 1
    ));
    out.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        n + 2
    ));
    out.push_str("</Relationships>");
    out
}

fn content_types_xml(model: &Model) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"gmd\" ContentType=\"text/gridmd\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
    );
    for i in 0..model.sheets.len() {
        out.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            i + 1
        ));
    }
    out.push_str("<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>");
    out.push_str("<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    out.push_str("</Types>");
    out
}

fn root_rels_xml() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".to_string()
}

/// The zip parts of the package, optionally including the GridMD carry part.
pub fn build_parts(model: &Model, source: &str, include_carry: bool) -> Vec<ZipEntry> {
    let date_system = if model.fm.get("date-system").and_then(|d| d.as_i64()) == Some(1904) {
        1904
    } else {
        1900
    };
    let mut ss = SharedStrings::new();
    let mut worksheets: Vec<(String, String)> = Vec::new();
    for (i, s) in model.sheets.iter().enumerate() {
        let xml = worksheet_xml(s, &mut ss, date_system);
        worksheets.push((format!("xl/worksheets/sheet{}.xml", i + 1), xml));
    }

    let mut entries = vec![
        ZipEntry {
            name: "[Content_Types].xml".to_string(),
            data: content_types_xml(model).into_bytes(),
        },
        ZipEntry {
            name: "_rels/.rels".to_string(),
            data: root_rels_xml().into_bytes(),
        },
        ZipEntry {
            name: "xl/workbook.xml".to_string(),
            data: workbook_xml(model).into_bytes(),
        },
        ZipEntry {
            name: "xl/_rels/workbook.xml.rels".to_string(),
            data: workbook_rels_xml(model).into_bytes(),
        },
        ZipEntry {
            name: "xl/styles.xml".to_string(),
            data: styles_xml().into_bytes(),
        },
    ];
    for (name, xml) in worksheets {
        entries.push(ZipEntry {
            name,
            data: xml.into_bytes(),
        });
    }
    entries.push(ZipEntry {
        name: "xl/sharedStrings.xml".to_string(),
        data: shared_strings_xml(&ss).into_bytes(),
    });
    if include_carry {
        entries.push(ZipEntry {
            name: CARRY_PART.to_string(),
            data: source.as_bytes().to_vec(),
        });
    }
    entries
}

/// A native-cell count per sheet, for the loud fidelity report.
pub fn native_cell_count(model: &Model) -> usize {
    model
        .sheets
        .iter()
        .map(|s| s.cells.iter().filter(|c| c.content.is_some()).count())
        .sum()
}

/// Export a model + its source to an `.xlsx` byte buffer.
pub fn write_xlsx(model: &Model, source: &str) -> Vec<u8> {
    zip_write(&build_parts(model, source, true))
}
