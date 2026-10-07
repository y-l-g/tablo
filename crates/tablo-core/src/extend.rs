//! The traits that add a new kind of building block to a declaration: a [`Column`] a table lists
//! and a detail page shows, a [`Filter`] a table offers, a [`Control`] a
//! [`Field::custom`](crate::Field::custom) field renders, an [`OptionSource`] a relationship
//! choice loads its options from, and a [`TypedValue`] a text field parses. [`Includes`],
//! [`FilterInput`] and [`ControlInput`] are what their methods take and return.
//!
//! The built-in columns and filters and the [`Toggle`](crate::Toggle) control implement these
//! traits too, every [`Resource`](crate::Resource) is an [`OptionSource`], and the integer and
//! float types, `bool`, `Uuid`, [`TenantId`](crate::TenantId) and `jiff::Timestamp` are
//! [`TypedValue`]s.

pub use crate::{
    schema::{
        fields::custom::{Control, ControlInput},
        relationship::OptionSource,
        validation::TypedValue,
    },
    table::{
        column::{Column, Includes},
        filter::{Filter, FilterInput},
    },
};
