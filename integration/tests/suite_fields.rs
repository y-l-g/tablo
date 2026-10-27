//! The integration suite's fields binary: what a form field shows, accepts and refuses: access,
//! rules, comparisons, conditions and dependent options.
//!
//! Each module is one suite file; `common` holds the fixtures every binary shares. Filter per file
//! with `cargo test -p tablo-integration <module>::`.

#[allow(
    dead_code,
    unused_imports,
    reason = "each binary uses part of the shared fixtures"
)]
mod common;

mod condition;
mod dependent;
mod field_access;
mod field_access_related;
mod field_basics;
mod field_comparisons;
mod field_rules;
mod options_enum;
mod typed_leaves;
mod variants;
