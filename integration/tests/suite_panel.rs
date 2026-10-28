//! The integration suite's panel binary: what a panel mounts and serves: declarations, pages, the
//! shell, header actions, extension seams and uploads.
//!
//! Each module is one suite file; `common` holds the fixtures every binary shares. Filter per file
//! with `cargo test -p tablo-integration <module>::`.

#[allow(
    dead_code,
    unused_imports,
    reason = "each binary uses part of the shared fixtures"
)]
mod common;

mod action_policy;
mod declaration;
mod extend;
mod header_action;
mod layouts;
mod navigation;
mod outcome;
mod pages;
mod panel;
mod shell;
mod upload;
mod upload_discard;
mod upload_rules;
mod upstream;
mod version;
mod widgets;
