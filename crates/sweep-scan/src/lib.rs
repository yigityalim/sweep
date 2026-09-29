#![forbid(unsafe_code)]

mod git;
mod rules;
mod scanner;
mod size;

pub use rules::classify_path;
pub use scanner::{ScanError, ScanOptions, scan};
