//! GridMD — a plain-text spreadsheet format with a two-way XLSX converter.
//! Rust port of the JS reference implementation. The canonical model dump
//! (`dump`) is the cross-language conformance contract (conformance/README.md).

pub mod diag;
pub mod dump;
pub mod model;
pub mod parser;
pub mod refs;
pub mod scalar;
pub mod validate;
pub mod xlsx;
pub mod xml;
pub mod yaml;

pub use diag::Diag;
use parser::{parse_document, Document, Mode, Stats};
use validate::{validate_document, Validator};

/// Combined parse + validate result, with parse and validate diagnostics merged
/// and stably sorted by line (mirrors `js/src/index.js` `lint`).
pub struct Lint {
    pub doc: Document,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    pub stats: Stats,
}

pub fn lint(source: &str, mode: Mode) -> Lint {
    let doc = parse_document(source, mode);
    let Validator {
        errors: verrors,
        warnings: vwarnings,
        stats,
        ..
    } = validate_document(&doc);
    let mut errors = doc.errors.clone();
    errors.extend(verrors);
    errors.sort_by(|a, b| a.line.cmp(&b.line));
    let mut warnings = doc.warnings.clone();
    warnings.extend(vwarnings);
    warnings.sort_by(|a, b| a.line.cmp(&b.line));
    Lint {
        doc,
        errors,
        warnings,
        stats,
    }
}

/// Strict-mode dump. Returns the canonical dump, or the list of errors.
pub fn dump_source(source: &str) -> Result<String, Vec<Diag>> {
    let res = lint(source, Mode::Strict);
    if !res.errors.is_empty() {
        return Err(res.errors);
    }
    let model = model::build_model(&res.doc);
    Ok(dump::dump_model(&model))
}
