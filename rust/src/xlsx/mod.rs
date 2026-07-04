//! XLSX two-way conversion: a deterministic STORE zip layer, a native worksheet
//! writer with a full-source carry part, and a carry-first / native-fallback
//! reader.

pub mod read;
pub mod write;
pub mod zip;

pub use read::xlsx_to_gridmd;
pub use write::write_xlsx;
