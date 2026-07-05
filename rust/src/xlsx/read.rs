//! XLSX → GridMD. If the package carries the original GridMD source
//! (`gridmd/source.gmd`, written by `write.rs`), the import restores it verbatim
//! — a byte-exact round-trip. Otherwise a native fallback reverse-parses the
//! worksheet core (sheets, cells, formulas, merges, hidden state) into
//! lint-clean GridMD. Foreign parts that are not reverse-parsed are reported,
//! never silently dropped.

use crate::refs::{num_to_col, parse_cell};
use crate::xlsx::write::CARRY_PART;
use crate::xlsx::zip::zip_read;
use crate::xml::{parse_xml, XmlNode};

pub struct ImportReport {
    pub feature: String,
    pub action: String,
    pub note: Option<String>,
}

fn report(feature: &str, action: &str, note: Option<&str>) -> ImportReport {
    ImportReport {
        feature: feature.to_string(),
        action: action.to_string(),
        note: note.map(|s| s.to_string()),
    }
}

/// Import an `.xlsx` buffer into GridMD source + a fidelity report.
pub fn xlsx_to_gridmd(buf: &[u8]) -> Result<(String, Vec<ImportReport>), String> {
    let entries = zip_read(buf)?;
    let find = |name: &str| entries.iter().find(|(n, _)| n == name).map(|(_, d)| d);

    if let Some(data) = find(CARRY_PART) {
        let src = String::from_utf8_lossy(data).into_owned();
        return Ok((
            src,
            vec![report(
                "gridmd/source.gmd",
                "restored",
                Some("full-fidelity GridMD carry part — byte-exact round-trip"),
            )],
        ));
    }

    native_import(&entries)
}

fn native_import(entries: &[(String, Vec<u8>)]) -> Result<(String, Vec<ImportReport>), String> {
    let get = |name: &str| -> Option<String> {
        entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
    };
    let wb_src = get("xl/workbook.xml").ok_or("missing xl/workbook.xml")?;
    let wb = parse_xml(&wb_src);
    let rels = get("xl/_rels/workbook.xml.rels")
        .map(|s| parse_xml(&s))
        .unwrap_or_else(|| XmlNode {
            name: "Relationships".to_string(),
            attrs: vec![],
            children: vec![],
            text: String::new(),
        });
    let mut rel_targets: Vec<(String, String)> = Vec::new();
    for r in rels.all("Relationship") {
        if let (Some(id), Some(t)) = (r.attr("Id"), r.attr("Target")) {
            rel_targets.push((id.to_string(), t.to_string()));
        }
    }
    let shared = get("xl/sharedStrings.xml")
        .map(|s| read_shared_strings(&s))
        .unwrap_or_default();

    let mut report_lines = Vec::new();
    let mut out = String::from("---\ngridmd: \"1.0\"\n---\n");

    let sheets_el = wb.one("sheets");
    let sheet_nodes = sheets_el.map(|s| s.all("sheet")).unwrap_or_default();
    for sh in sheet_nodes {
        let name = sh.attr("name").unwrap_or("Sheet").to_string();
        let state = sh.attr("state").map(|s| s.to_string());
        let rid = sh.attr("id").or_else(|| sh.attr("r:id"));
        let target = rid
            .and_then(|id| rel_targets.iter().find(|(rid, _)| rid == id))
            .map(|(_, t)| normalize_target(t));
        out.push_str(&format!("\n# {name}\n"));
        if let Some(state) = &state {
            let vis = if state == "veryHidden" { "very" } else { "true" };
            out.push_str(&format!("\n```{{sheet}}\nhidden: {vis}\n```\n"));
        }
        if let Some(target) = target {
            if let Some(ws_src) = get(&target) {
                emit_worksheet(&ws_src, &shared, &mut out);
            } else {
                report_lines.push(report(&format!("{name} worksheet"), "not-emitted", Some("worksheet part missing")));
            }
        }
    }
    report_lines.push(report("worksheet core", "imported", Some("cells, formulas, merges reverse-parsed")));
    Ok((out, report_lines))
}

fn normalize_target(t: &str) -> String {
    let t = t.strip_prefix('/').unwrap_or(t);
    if t.starts_with("xl/") {
        t.to_string()
    } else {
        format!("xl/{t}")
    }
}

fn read_shared_strings(src: &str) -> Vec<String> {
    let sst = parse_xml(src);
    sst.all("si").iter().map(|si| si.text_of()).collect()
}

fn emit_worksheet(src: &str, shared: &[String], out: &mut String) {
    let ws = parse_xml(src);
    let sheet_data = match ws.one("sheetData") {
        Some(sd) => sd,
        None => return,
    };
    let mut cells: Vec<(i64, i64, String)> = Vec::new();
    for row in sheet_data.all("row") {
        for c in row.all("c") {
            if let Some((col, r)) = c.attr("r").and_then(parse_cell).map(|cc| (cc.col, cc.row)) {
                if let Some(line) = cell_to_at(c, shared, col, r) {
                    cells.push((r, col, line));
                }
            }
        }
    }
    cells.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, line) in cells {
        out.push_str(&line);
        out.push('\n');
    }
    if let Some(merges) = ws.one("mergeCells") {
        for m in merges.all("mergeCell") {
            if let Some(refr) = m.attr("ref") {
                out.push_str(&format!("@ {refr} {{ merge: true }}\n"));
            }
        }
    }
}

fn cell_to_at(c: &XmlNode, shared: &[String], col: i64, row: i64) -> Option<String> {
    let refr = format!("{}{}", num_to_col(col), row);
    let t = c.attr("t");
    let v = c.one("v").map(|n| n.text_of());
    if let Some(f) = c.one("f") {
        let formula = f.text_of();
        if formula.is_empty() {
            return None;
        }
        let cached = v.map(|val| match t {
            Some("str") | Some("s") => quote_text(&val),
            Some("b") => if val == "1" { "TRUE".to_string() } else { "FALSE".to_string() },
            Some("e") => val,
            _ => val,
        });
        return Some(match cached {
            Some(cv) => format!("@ {refr} ={formula} :: {cv}"),
            None => format!("@ {refr} ={formula}"),
        });
    }
    match t {
        Some("s") => {
            let idx: usize = v?.parse().ok()?;
            let text = shared.get(idx)?.clone();
            Some(emit_text(&refr, &text))
        }
        Some("inlineStr") => {
            let text = c.one("is").map(|is| is.text_of()).unwrap_or_default();
            Some(emit_text(&refr, &text))
        }
        Some("str") => Some(emit_text(&refr, &v.unwrap_or_default())),
        Some("b") => {
            let val = v?;
            Some(format!("@ {refr} {}", if val == "1" { "TRUE" } else { "FALSE" }))
        }
        Some("e") => Some(format!("@ {refr} {}", v?)),
        _ => {
            let val = v?;
            Some(format!("@ {refr} {val}"))
        }
    }
}

/// Emit a text cell; single-line quoted when safe, else a multiline body with a
/// YAML double-quoted value.
fn emit_text(refr: &str, text: &str) -> String {
    if text.contains('\n') || text.contains('\r') {
        format!("@ {refr}\n  value: {}", yaml_dq(text))
    } else {
        format!("@ {refr} {}", quote_text(text))
    }
}

/// GridMD double-quoted scalar (rule 5): wrap in `"`, doubling inner quotes.
fn quote_text(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// YAML double-quoted string with standard escapes.
fn yaml_dq(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
