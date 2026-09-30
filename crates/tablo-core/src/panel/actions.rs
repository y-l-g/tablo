//! Delete / bulk-delete / CSV export handlers plus their caps and parsers.
//!
//! Fetch, policy checks, and writes share one framework transaction:
//! a mid-loop failure deletes zero rows.
//!
//! The record fetchers live in `fetch`, the row delete in `delete`, the bulk
//! delete and its `ids` parser in `bulk`, the CSV export and its chunk walker
//! in `export`, and the relationship option search in `options`.

mod bulk;
mod delete;
mod export;
mod fetch;
mod options;

pub(crate) use self::{
    bulk::resource_bulk_delete,
    delete::resource_delete,
    export::resource_export,
    fetch::{find_by_key, load_detail, load_viewable},
    options::resource_options,
};
