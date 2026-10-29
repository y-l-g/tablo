//! One integration-test binary for the showcase.
//!
//! Every test file is a module of one binary: one link for the whole suite, and
//! `tests/common` is compiled once.
//!
//! Each module drives the showcase's own resources, one framework feature each; the demo app's
//! public pages are `blog` and `media`.
//!
//! Filter per file with `cargo test --test it <module>::` (the module name is
//! the file's name). Accepted costs: a compile error in any module fails the
//! whole target, and there is no per-file binary isolation.

mod common;

mod action;
mod auth;
mod blog;
mod cell;
mod create;
mod csrf;
mod delete;
mod detail;
mod edit;
mod export;
mod filter;
mod gates;
mod import;
mod inline_edit;
mod layouts;
mod list;
mod many_to_many;
mod media;
mod registration;
mod relations;
mod repeater;
mod rich_text;
mod row_color;
mod search;
mod shell;
mod tenancy;
mod toggleable_columns;
mod widgets;
