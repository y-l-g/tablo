//! [`ModelColumns`]: the table and detail page a model reads as by default.

use toasty::{schema::Model, stmt::IntoExpr};

use crate::{Detail, Table};

/// The table and detail page a model reads as when its resource declares none: what a
/// [`ResourceDef`](crate::ResourceDef) without a [`table`](crate::ResourceDef::table) lists and
/// without a [`detail`](crate::ResourceDef::detail) shows.
///
/// Derive it with [`ModelColumns`](derive@crate::ModelColumns), which gives a column to each field
/// a person reads, whether a form or Toasty writes it. An impl by hand names the model's tenant
/// columns; one that keeps the defaults here lists no column and turns the detail page off, for a
/// resource that declares both.
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

    /// The default detail page, which is off here.
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
}

#[doc(hidden)]
pub mod pick;

#[cfg(test)]
mod tests;
