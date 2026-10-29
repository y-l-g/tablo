//! The integration suite's records binary: what a submitted form writes: derived forms, embedded
//! values, related forms, commits, uniqueness and action input.
//!
//! Each module is one suite file; `common` holds the fixtures every binary shares. Filter per file
//! with `cargo test -p tablo-integration <module>::`.

#[allow(
    dead_code,
    unused_imports,
    reason = "each binary uses part of the shared fixtures"
)]
mod common;

mod action_input;
mod commit;
mod embedded_lens;
mod embedded_value;
mod record_form;
mod related_form;
mod unique;
mod write;
mod write_error;
