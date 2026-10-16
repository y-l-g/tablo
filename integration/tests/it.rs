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
mod conditions;
mod dependent;
mod extensions;
mod field_basics;
mod header_actions;
mod key_collisions;
mod model_columns;
mod options_enum;
mod panels;
mod profile;
mod resource_query_override;
mod sign_up;
mod sqlite;
mod stream_pool;
mod typed_leaves;
mod uploads;
mod upstream;
mod variants;
mod writes;

mod embedded_lens;
mod embedded_value;
mod many_to_many;
mod record_form;
mod relation_table;
