//! One integration-test binary for the showcase (GH #179).
//!
//! Every test file is a module of one binary: one link for the whole suite, and
//! `tests/common` is compiled once.
//!
//! The top-level modules drive the showcase's own resources, one framework feature each; the
//! demo app's public pages are `blog` and `media`. `framework` covers what those resources do not
//! reach, with test-local models.
//!
//! Filter per file with `cargo test --test it <module>::` (the module name is
//! the file's name). Accepted costs: a compile error in any module fails the
//! whole target, and there is no per-file binary isolation.

mod common;
mod framework;

mod actions;
mod auth;
mod blog;
mod create;
mod delete;
mod detail;
mod edit;
mod export;
mod filters;
mod gates;
mod list;
mod media;
mod panel;
mod relations;
mod repeaters;
mod tenancy;
