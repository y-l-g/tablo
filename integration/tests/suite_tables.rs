//! The integration suite's tables binary: what a table lists and reaches: columns, search,
//! relations, export and import.
//!
//! Each module is one suite file; `common` holds the fixtures every binary shares. Filter per file
//! with `cargo test -p tablo-integration <module>::`.

#[allow(
    dead_code,
    unused_imports,
    reason = "each binary uses part of the shared fixtures"
)]
mod common;

mod column;
mod export;
mod import;
mod key_collisions;
mod list;
mod many_to_many;
mod model_columns;
mod relations;
mod relationship_options;
mod search;
mod soft_delete;
mod toggleable_columns;
