//! The framework cases: panels built from test-local models and resources, for behavior the
//! showcase's own resources do not reach (mount errors, extension seams, embedded values, upload
//! edge cases). `common` holds their fixtures.

mod common;

mod after_commit;
mod auth_override;
mod extensions;
mod panels;
mod resource_query_override;
mod sqlite;
mod stream_pool;
mod typed_leaves;
mod uploads;

mod embedded_lens;
mod embedded_value;
mod record_form;
mod relation_table;
