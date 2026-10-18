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
mod columns;
mod conditions;
mod declaration;
mod dependent;
mod export;
mod extensions;
mod field_basics;
mod field_rules;
mod header_actions;
mod key_collisions;
mod list;
mod model_columns;
mod options_enum;
mod pages;
mod panels;
mod password_reset;
mod profile;
mod shell;
mod sign_in;
mod sign_up;
mod stream_pool;
mod toggleable_columns;
mod typed_leaves;
mod unique;
mod uploads;
mod upstream;
mod variants;
mod writes;

mod embedded_lens;
mod embedded_value;
mod many_to_many;
mod record_form;
mod relation_table;
mod relationship_options;
