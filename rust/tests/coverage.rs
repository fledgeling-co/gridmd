//! Targeted tests filling coverage gaps the fixtures + main unit suite miss:
//! the native XLSX reader's cell-type branches, writer edge scalars, YAML
//! numeric resolution, model coercions, and zip error paths.

use gridmd::model::build_model;
use gridmd::parser::Mode;
use gridmd::scalar::Scalar;
use gridmd::validate::is_valid_part_path;
use gridmd::xlsx::write::write_xlsx;
use gridmd::xlsx::xlsx_to_gridmd;
use gridmd::xlsx::zip::{zip_read, zip_write, ZipEntry};
use gridmd::yaml::{load, Yaml};
use gridmd::{dump_source, lint};

fn s(body: &str) -> String {
    format!("---\ngridmd: \"1.0\"\n---\n\n# S\n{body}\n")
}
fn model_of(src: &str) -> gridmd::model::Model {
    build_model(&lint(src, Mode::Strict).doc)
}
fn cell<'a>(m: &'a gridmd::model::Model, refr: &str) -> Option<&'a gridmd::model::Content> {
    let c = gridmd::refs::parse_cell(refr).unwrap();
    m.sheets[0]
        .cells
        .iter()
        .find(|x| x.col == c.col && x.row == c.row)
        .and_then(|x| x.content.as_ref())
}

fn entry(name: &str, xml: &str) -> ZipEntry {
    ZipEntry { name: name.to_string(), data: xml.as_bytes().to_vec() }
}

/// A synthetic package exercising every native-reader cell branch.
fn synthetic_xlsx() -> Vec<u8> {
    let workbook = r#"<workbook xmlns:r="r"><sheets><sheet name="One" sheetId="1" r:id="rId1"/><sheet name="Two" sheetId="2" state="veryHidden" r:id="rId2"/></sheets></workbook>"#;
    let rels = r#"<Relationships><Relationship Id="rId1" Target="/xl/worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/missing.xml"/></Relationships>"#;
    let sst = "<sst><si><t>hello</t></si><si><t xml:space=\"preserve\">multi\nline</t></si></sst>";
    let sheet1 = r#"<worksheet><sheetData><row r="1"><c r="A1"><v>42</v></c><c r="B1" t="s"><v>0</v></c><c r="C1" t="s"><v>1</v></c><c r="D1" t="inlineStr"><is><t>inl</t></is></c><c r="E1" t="b"><v>1</v></c><c r="F1" t="b"><v>0</v></c><c r="G1" t="e"><v>#N/A</v></c><c r="H1" t="str"><f>A1&amp;B1</f><v>res</v></c><c r="I1"><f>SUM(A1:A1)</f><v>42</v></c><c r="J1"><f>NOW()</f></c><c r="K1" t="b"><f>ISBLANK(A1)</f><v>0</v></c><c r="L1" t="s"><f>X</f><v>0</v></c><c r="M1"/><c><v>99</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A1:C1"/></mergeCells></worksheet>"#;
    zip_write(&[
        entry("xl/workbook.xml", workbook),
        entry("xl/_rels/workbook.xml.rels", rels),
        entry("xl/sharedStrings.xml", sst),
        entry("xl/worksheets/sheet1.xml", sheet1),
    ])
}

#[test]
fn native_reader_all_cell_types() {
    let (gmd, report) = xlsx_to_gridmd(&synthetic_xlsx()).unwrap();
    // veryHidden sheet + missing worksheet part are reported/handled.
    assert!(gmd.contains("hidden: very"));
    assert!(report.iter().any(|r| r.feature.contains("worksheet") && r.action == "not-emitted"));
    // multiline shared string routed through the YAML block value form.
    assert!(gmd.contains("value: \"multi\\nline\""));
    assert!(gmd.contains("@ E1 TRUE"));
    assert!(gmd.contains("@ F1 FALSE"));
    assert!(gmd.contains("@ G1 #N/A"));
    assert!(gmd.contains("@ H1 =A1&B1 :: \"res\""));
    assert!(gmd.contains("@ I1 =SUM(A1:A1) :: 42"));
    assert!(gmd.contains("@ J1 =NOW()"));
    assert!(gmd.contains("@ K1 =ISBLANK(A1) :: FALSE"));
    assert!(gmd.contains("@ D1 \"inl\""));
    // the reverse-parsed document must itself lint clean.
    assert!(lint(&gmd, Mode::Strict).errors.is_empty(), "{:?}", lint(&gmd, Mode::Strict).errors);
}

#[test]
fn native_reader_error_and_missing_paths() {
    // Missing workbook.xml → Err.
    let no_wb = zip_write(&[entry("junk.xml", "<x/>")]);
    assert!(xlsx_to_gridmd(&no_wb).is_err());
    // Missing workbook rels → sheets resolve to headers only, still ok.
    let wb = r#"<workbook xmlns:r="r"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let no_rels = zip_write(&[entry("xl/workbook.xml", wb)]);
    let (gmd, _) = xlsx_to_gridmd(&no_rels).unwrap();
    assert!(gmd.contains("# S"));
}

#[test]
fn writer_non_numeric_cached_and_very_hidden() {
    // Formula cached bool / error / date → scalar_display str path; very-hidden
    // sheet → workbook state; a genuine round-trip via the carry part.
    let src = format!(
        "---\ngridmd: \"1.0\"\n---\n\n# S\n```{{sheet}}\nhidden: very\n```\n@ A1 =B1 :: TRUE\n@ A2 =B2 :: #N/A\n@ A3 =B3 :: 2026-01-01\n"
    );
    let m = model_of(&src);
    let xlsx = write_xlsx(&m, &src);
    // The package is a valid zip and restores byte-exactly via the carry.
    let (back, _) = xlsx_to_gridmd(&xlsx).unwrap();
    assert_eq!(dump_source(&src).unwrap(), dump_source(&back).unwrap());
    // The native worksheet XML carries the string-typed cached values.
    let parts = zip_read(&xlsx).unwrap();
    let sheet = parts.iter().find(|(n, _)| n == "xl/worksheets/sheet1.xml").unwrap();
    let xml = String::from_utf8_lossy(&sheet.1);
    assert!(xml.contains("t=\"str\""));
    assert!(xml.contains("<v>TRUE</v>") && xml.contains("<v>#N/A</v>"));
}

#[test]
fn yaml_numeric_resolution() {
    let l = load("a: .5\nb: 1e3\nc: +7\nd: -2.5E+3\ne: 1.\nf: not-a-number\n").unwrap();
    let v = l.value;
    assert!(matches!(v.get("a"), Some(Yaml::Real(_))));
    assert!(matches!(v.get("b"), Some(Yaml::Real(_))));
    assert!(matches!(v.get("c"), Some(Yaml::Int(7))));
    assert!(matches!(v.get("d"), Some(Yaml::Real(_))));
    assert!(matches!(v.get("e"), Some(Yaml::Real(_))));
    assert!(matches!(v.get("f"), Some(Yaml::Str(_))));
}

#[test]
fn model_coercions_and_truthy() {
    // yaml_scalar Real + plain-text value branches
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: 0.5")), "A1").unwrap().scalar, Some(Scalar::Number(n)) if n == 0.5));
    assert!(matches!(cell(&model_of(&s("@ A1\n  value: hello")), "A1").unwrap().scalar, Some(Scalar::Text { .. })));
    // truthy on numeric / list / map / false note values
    assert_eq!(model_of(&s("@ A1 1\n  note: 7")).sheets[0].counts.notes, 1);
    assert_eq!(model_of(&s("@ A1 1\n  note: 1.5")).sheets[0].counts.notes, 1);
    assert_eq!(model_of(&s("@ A1 1\n  note: [x]")).sheets[0].counts.notes, 1);
    assert_eq!(model_of(&s("@ A1 1\n  note: { a: b }")).sheets[0].counts.notes, 1);
    assert_eq!(model_of(&s("@ A1 1 { note: false }")).sheets[0].counts.notes, 0);
    assert_eq!(model_of(&s("@ A1 1 { note: \"\" }")).sheets[0].counts.notes, 0);
}

#[test]
fn validate_extra_branches() {
    // anchor sheet-qualifier mismatch through ctx.target
    let src = s("```{cf} Wrong!B2\n- when: \"> 1\"\n```");
    assert!(lint(&src, Mode::Strict).errors.iter().any(|e| e.msg.contains("must name the containing sheet")));
    // is_valid_part_path space + control-char rejection
    assert!(!is_valid_part_path("a b.xml"));
    assert!(!is_valid_part_path("a\tb.xml"));
    assert!(is_valid_part_path("customXml/item1.xml"));
}

#[test]
fn dump_escapes_control_characters() {
    // A cell text carrying control chars exercises every JSON string escape.
    let src = s("@ A1\n  value: \"x\\ty\\nz\\rw\\bv\\fu\\u0001t\"");
    let dump = dump_source(&src).unwrap();
    assert!(dump.contains("x\\ty\\nz\\rw\\bv\\fu\\u0001t"), "{dump}");
}

#[test]
fn writer_escapes_and_false_cached() {
    // Formula text with XML metacharacters + apostrophes → xml_escape branches;
    // a FALSE cached value → scalar_display boolean-false branch.
    let src = format!(
        "---\ngridmd: \"1.0\"\n---\n\n# S\n@ A1 =IF(B1>2,\"<a>\",C1&\"y\") :: FALSE\n@ A2 ='Other Sheet'!D4 :: 1\n"
    );
    let m = model_of(&src);
    let xlsx = write_xlsx(&m, &src);
    let parts = zip_read(&xlsx).unwrap();
    let ws = parts.iter().find(|(n, _)| n == "xl/worksheets/sheet1.xml").unwrap();
    let xml = String::from_utf8_lossy(&ws.1);
    assert!(xml.contains("&lt;a&gt;") && xml.contains("&amp;") && xml.contains("&quot;"));
    assert!(xml.contains("&apos;Other Sheet&apos;"));
    assert!(xml.contains("<v>FALSE</v>"));
}

#[test]
fn model_null_note_and_body_spill() {
    // truthy(Null) branch for an explicit null note.
    assert_eq!(model_of(&s("@ A1\n  note: ~")).sheets[0].counts.notes, 0);
    // body-form formula carrying a spill range → body_content array_ref branch.
    let m = model_of(&s("@ A1\n  formula: =SORT(B1:B5)\n  spill: A1:A5"));
    assert_eq!(cell(&m, "A1").unwrap().array_ref.as_deref(), Some("A1:A5"));
}

#[test]
fn parser_non_pipe_grid_row() {
    // A body line that is not a pipe row → parse_rows diagnostic.
    let src = s("```{grid} A1\nnot a pipe row\n```");
    assert!(lint(&src, Mode::Strict).errors.iter().any(|e| e.msg.contains("expected a pipe row")));
    // A props-only inline that fails the props check falls back to a scalar.
    let m = model_of(&s("@ A1 =B1{notprops"));
    assert!(cell(&m, "A1").is_some());
}

#[test]
fn yaml_non_numeric_plain_scalars() {
    let l = load("a: .\nb: 1e\nc: 1.2.3\n").unwrap();
    assert!(matches!(l.value.get("a"), Some(Yaml::Str(_))));
    assert!(matches!(l.value.get("b"), Some(Yaml::Str(_))));
    assert!(matches!(l.value.get("c"), Some(Yaml::Str(_))));
}

#[test]
fn zip_error_paths() {
    // Valid zip whose stored data is then corrupted → CRC mismatch.
    let mut buf = zip_write(&[ZipEntry { name: "a".into(), data: b"payload".to_vec() }]);
    // The local-header data for a 1-char name sits at offset 30 + 1 = 31.
    buf[31] ^= 0xff;
    assert!(zip_read(&buf).unwrap_err().contains("crc"));
    // A buffer with an EOCD signature but a bogus central directory.
    let mut bogus = vec![0u8; 40];
    let eocd = 40 - 22;
    bogus[eocd..eocd + 4].copy_from_slice(&0x06054b50u32.to_le_bytes());
    bogus[eocd + 10..eocd + 12].copy_from_slice(&1u16.to_le_bytes()); // count = 1
    bogus[eocd + 16..eocd + 20].copy_from_slice(&0u32.to_le_bytes()); // cd offset = 0
    assert!(zip_read(&bogus).is_err());
}
