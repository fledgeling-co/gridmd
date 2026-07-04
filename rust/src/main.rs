//! GridMD CLI: `dump`, `to-xlsx`, `from-xlsx` (conformance/README.md §CLI).

use std::io::Write;
use std::process::ExitCode;

use gridmd::model::build_model;
use gridmd::parser::Mode;
use gridmd::xlsx::write::{native_cell_count, write_xlsx};
use gridmd::xlsx::xlsx_to_gridmd;
use gridmd::{dump_source, lint};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verb = args.first().map(|s| s.as_str());
    match verb {
        Some("dump") => cmd_dump(&args[1..]),
        Some("to-xlsx") => cmd_to_xlsx(&args[1..]),
        Some("from-xlsx") => cmd_from_xlsx(&args[1..]),
        _ => {
            eprintln!("usage: gridmd <dump|to-xlsx|from-xlsx> <file> [-o out]");
            ExitCode::from(2)
        }
    }
}

/// Split `[-o out] positionals` into `(input, output)`.
fn parse_io(args: &[String]) -> (Option<String>, Option<String>) {
    let mut output = None;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                output = args.get(i + 1).cloned();
                i += 2;
            }
            "--strict" => i += 1,
            a if a.starts_with('-') => i += 1,
            a => {
                positionals.push(a.to_string());
                i += 1;
            }
        }
    }
    let input = positionals
        .into_iter()
        .find(|p| Some(p) != output.as_ref());
    (input, output)
}

fn read_text(path: &str) -> Result<String, ExitCode> {
    std::fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read {path}: {e}");
        ExitCode::from(2)
    })
}

fn cmd_dump(args: &[String]) -> ExitCode {
    let (input, _) = parse_io(args);
    let input = match input {
        Some(f) => f,
        None => {
            eprintln!("usage: gridmd dump <file.gmd>");
            return ExitCode::from(2);
        }
    };
    let source = match read_text(&input) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match dump_source(&source) {
        Ok(dump) => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(dump.as_bytes());
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for e in &errors {
                eprintln!("{input}:{}: error: {}", e.line, e.msg);
            }
            ExitCode::from(1)
        }
    }
}

fn cmd_to_xlsx(args: &[String]) -> ExitCode {
    let (input, output) = parse_io(args);
    let input = match input {
        Some(f) => f,
        None => {
            eprintln!("usage: gridmd to-xlsx <file.gmd> [-o out.xlsx]");
            return ExitCode::from(2);
        }
    };
    let source = match read_text(&input) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let res = lint(&source, Mode::Strict);
    if !res.errors.is_empty() {
        for e in &res.errors {
            eprintln!("{input}:{}: error: {}", e.line, e.msg);
        }
        eprintln!(
            "{input}: {} error(s) — fix the document before converting",
            res.errors.len()
        );
        return ExitCode::from(1);
    }
    let model = build_model(&res.doc);
    let buffer = write_xlsx(&model, &source);
    let out = output.unwrap_or_else(|| {
        input
            .strip_suffix(".gmd")
            .map(|b| format!("{b}.xlsx"))
            .unwrap_or_else(|| format!("{input}.xlsx"))
    });
    if let Err(e) = std::fs::write(&out, &buffer) {
        eprintln!("error: cannot write {out}: {e}");
        return ExitCode::from(2);
    }
    let cells = native_cell_count(&model);
    println!(
        "{out}: written ({} bytes) — {} sheet(s), {cells} native cell(s); full-fidelity GridMD source carried in gridmd/source.gmd (nothing dropped)",
        buffer.len(),
        model.sheets.len()
    );
    ExitCode::SUCCESS
}

fn cmd_from_xlsx(args: &[String]) -> ExitCode {
    let (input, output) = parse_io(args);
    let input = match input {
        Some(f) => f,
        None => {
            eprintln!("usage: gridmd from-xlsx <file.xlsx> [-o out.gmd]");
            return ExitCode::from(2);
        }
    };
    let bytes = match std::fs::read(&input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read {input}: {e}");
            return ExitCode::from(2);
        }
    };
    let (gmd, report) = match xlsx_to_gridmd(&bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{input}: error: {e}");
            return ExitCode::from(1);
        }
    };
    for r in &report {
        let note = r.note.as_deref().map(|n| format!(" ({n})")).unwrap_or_default();
        println!("{input}: {}: {}{note}", r.action, r.feature);
    }
    let res = lint(&gmd, Mode::Strict);
    for e in &res.errors {
        eprintln!("self-check:{}: error: {}", e.line, e.msg);
    }
    let out = output.unwrap_or_else(|| {
        input
            .strip_suffix(".xlsx")
            .or_else(|| input.strip_suffix(".xlsm"))
            .map(|b| format!("{b}.gmd"))
            .unwrap_or_else(|| format!("{input}.gmd"))
    });
    if let Err(e) = std::fs::write(&out, &gmd) {
        eprintln!("error: cannot write {out}: {e}");
        return ExitCode::from(2);
    }
    println!(
        "{out}: written — {} sheet(s); self-check {}",
        res.doc.sheets.len(),
        if res.errors.is_empty() {
            "clean".to_string()
        } else {
            format!("FAILED ({} error(s))", res.errors.len())
        }
    );
    if res.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
