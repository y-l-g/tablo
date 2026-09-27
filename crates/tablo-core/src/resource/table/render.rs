//! [`Table`](super::Table) HTML rendering: the entry points plus the chrome.
//!
//! Grouping is page-local and interleaved and a page encodes the
//! filter transport once.
//!
//! The entry points and row assembly live in `core`, the search and bulk
//! bars in `toolbar`, the filter bar in `filterbar`, pagination in `pager`,
//! the delete dialog in `dialog`, and the streamed placeholder in `skeleton`.

mod core;
mod dialog;
mod filterbar;
mod pager;
mod skeleton;
mod toolbar;
