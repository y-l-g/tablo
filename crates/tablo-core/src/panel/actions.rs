//! The delete, custom action and header action handlers, the CSV export, and
//! the relationship option search.
//!
//! Fetch, policy checks, and writes share one framework transaction:
//! a mid-loop failure deletes zero rows.
//!
//! The record fetchers live in `fetch`, the one pipeline the deletes and the
//! custom actions share in `mutation`, the CSV export and its chunk walker in
//! `export`, and the relationship option search in `options`.

mod export;
mod fetch;
mod mutation;
mod options;

pub(crate) use self::{
    export::resource_export,
    fetch::{load_detail, load_viewable},
    mutation::{
        resource_bulk_delete, resource_delete, resource_list_action, resource_record_action,
        run_header,
    },
    options::resource_options,
};
