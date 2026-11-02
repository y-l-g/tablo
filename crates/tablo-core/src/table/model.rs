//! [`ModelColumns`]: the table and detail page a model reads as by default.

use toasty::{schema::Model, stmt::IntoExpr};

use crate::{Detail, Table};

/// The table and detail page a model reads as when its resource declares none: what a
/// [`ResourceDef`](crate::ResourceDef) without a [`table`](crate::ResourceDef::table) lists and
/// without a [`detail`](crate::ResourceDef::detail) shows.
///
/// Derive it with [`ModelColumns`](derive@crate::ModelColumns), which gives a column to each field
/// a person reads, whether a form or Toasty writes it. An impl by hand names the model's tenant
/// columns; one that keeps the defaults here lists no column, for a resource that declares its own
/// table and detail page, or turns the view page off.
#[diagnostic::on_unimplemented(
    message = "`{Self}` declares no default columns",
    label = "a resource's model implements `ModelColumns`",
    note = "add `#[derive(tablo::ModelColumns)]` to `{Self}`, or `impl tablo::ModelColumns for \
            {Self} {{ const TENANT_COLUMNS: &[&str] = &[..]; }}`, naming its `TenantId` fields, \
            when its resources declare their own table and detail page"
)]
pub trait ModelColumns: Model + IntoExpr<Self> + Sized {
    /// The model's [`TenantId`](crate::TenantId) fields, by name: the derive lists each field
    /// typed `TenantId` or `Option<TenantId>`. Mounting a panel refuses a resource over a model
    /// that lists one and declares no [`tenancy`](crate::ResourceDef::tenancy), which would serve
    /// every tenant's rows. It has no default, so an impl by hand states it.
    const TENANT_COLUMNS: &'static [&'static str];

    /// The default table, which lists no column here.
    fn table() -> Table<Self> {
        Table::new(())
    }

    /// The default detail page, which lists no column here: a resource serving the view page then
    /// declares its own, else mounting refuses it.
    fn detail() -> Detail<Self> {
        Detail::empty()
    }

    /// The record's `#[version]` counter, or `None` for a model without one.
    ///
    /// The edit form carries it, so a save refuses a record another save changed since the form
    /// opened. The derive reads the model's `#[version]` field; an impl written by hand keeps
    /// this default unless it returns the counter itself.
    fn version(&self) -> Option<u64> {
        None
    }

    /// Whether [`bare`](Self::bare) leaves out every relation the model holds. The derive
    /// answers `true` when each relation is a `Deferred` one; an impl by hand keeps this default
    /// unless its `bare` leaves them out.
    ///
    /// A panel then hands a closure over the row that declares it reads none, such as a
    /// [`row_color`](crate::Table::row_color), a copy of the row without them, so the closure
    /// mounts beside a column loading a record of a model a resource restricts a field of. Such a
    /// closure reaching a relation anyway panics at render, where it read the relation another
    /// column loaded before: declare the relations it reads.
    ///
    /// An eager relation, typed without `Deferred`, holds its records on every query, so a model
    /// with one answers `false` and keeps the refusal.
    const UNLOADS_RELATIONS: bool = false;

    /// A copy of the record without its relations, holding its own fields: the derive builds it
    /// field by field, leaving each `Deferred` relation unloaded rather than copying its records.
    #[must_use]
    fn bare(&self) -> Self
    where
        Self: Clone,
    {
        self.clone()
    }
}

/// A copy of `row` without its relations: see [`ModelColumns::UNLOADS_RELATIONS`].
pub(crate) fn bare<M: ModelColumns + Clone>(row: &M) -> M {
    row.bare()
}

#[doc(hidden)]
pub mod pick;

#[cfg(test)]
mod tests;
