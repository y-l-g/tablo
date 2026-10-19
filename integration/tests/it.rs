//! Tablo's integration suite: panels built from test-local models and resources, served over
//! HTTP (mount errors, extension seams, embedded values, upload edge cases). `common` holds their
//! fixtures.
//!
//! Every test file is a module of one binary: one link for the whole suite. Filter per file with
//! `cargo test -p tablo-integration --test it <module>::`.

mod common;

mod action_input;
mod action_policy;
mod after_commit;
mod auth_override;
mod column;
mod condition;
mod declaration;
mod dependent;
mod email_change;
mod embedded_lens;
mod embedded_value;
mod export;
mod extend;
mod field_basics;
mod field_rules;
mod header_action;
mod key_collisions;
mod list;
mod login;
mod mail;
mod many_to_many;
mod model_columns;
mod options_enum;
mod pages;
mod panel;
mod password_reset;
mod profile;
mod record_form;
mod registration;
mod relations;
mod relationship_options;
mod shell;
mod stream_pool;
mod tenant_registration;
mod toggleable_columns;
mod typed_leaves;
mod unique;
mod upload;
mod upstream;
mod variants;
mod verification;
mod write;
