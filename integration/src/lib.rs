//! Tablo's integration suite lives in `tests/it.rs`; this crate has no library of its own.
//!
//! It is a workspace member rather than a published crate's `tests/` so that no published crate
//! dev-depends on `tablo-test`, which would make a publish cycle.
