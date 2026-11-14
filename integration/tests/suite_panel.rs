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

mod action_face;
mod action_matching;
mod action_policy;
mod area;
mod declaration;
mod extend;
mod header_action;
mod header_action_badge;
mod header_action_preset;
mod layouts;
mod load;
mod navigation;
mod outcome;
mod page_filters;
mod pages;
mod panel;
mod panel_host;
mod record_pages;
mod shell;
mod upload;
mod upload_discard;
mod upload_list;
mod upload_rules;
mod upstream;
mod url;
mod version;
mod widgets;
