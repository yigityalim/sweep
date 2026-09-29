#![forbid(unsafe_code)]

mod git;
mod providers;
mod revalidate;
mod rules;
mod scanner;
mod size;

pub use revalidate::{
    CandidateRevalidation, PlanRevalidation, RevalidationReason, RevalidationStatus,
    revalidate_plan,
};
pub use rules::classify_path;
pub use scanner::{ScanError, ScanOptions, ScanResult, scan, scan_with_diagnostics};
