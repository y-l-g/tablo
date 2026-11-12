//! [`PanelModel`]: what a panel needs from a model a resource serves.

use toasty::{schema::Model, stmt::IntoExpr};

use crate::{Detail, Table};

/// What a panel needs from a model a resource serves: its tenant columns, its version counter,
/// how to copy a record without its relations, and the table and detail page it reads as when its
/// resource declares none, which a [`ResourceDef`](crate::ResourceDef) without a
/// [`table`](crate::ResourceDef::table) lists and without a [`detail`](crate::ResourceDef::detail)
/// shows.
///
/// Derive it with [`PanelModel`](derive@crate::PanelModel), which gives a column to each field
/// a person reads, whether a form or Toasty writes it. An impl by hand names the model's tenant
/// columns; one that keeps the defaults here lists no column, for a resource that declares its own
/// table and detail page, or turns the view page off.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a panel model",
    label = "a resource's model implements `PanelModel`",
    note = "add `#[derive(tablo::PanelModel)]` to `{Self}`, or `impl tablo::PanelModel for \
            {Self} {{ const TENANT_COLUMNS: &[&str] = &[..]; }}`, naming its `TenantId` fields, \
            when its resources declare their own table and detail page"
)]
pub trait PanelModel: Model + IntoExpr<Self> + Sized {
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

    /// The record's foreign keys and [`TenantId`](crate::TenantId) fields, by name, each spelled
    /// as the database stores it, `None` where it holds nothing.
    ///
    /// The default table leaves them out, so a [`ChangedField`](crate::ChangedField) reads them
    /// here: moving a record to another owner or tenant is a change a write names. The derive
    /// lists them; an impl by hand keeps this default, which lists none.
    fn references(&self) -> Vec<(&'static str, Option<String>)> {
        Vec::new()
    }
}

/// A foreign key or tenant field's type, spelled for [`PanelModel::references`].
#[doc(hidden)]
pub trait Reference {
    /// The value as the database stores it, `None` for an empty optional one.
    fn spell(&self) -> Option<String>;
}

/// Spells each key type a foreign key or tenant field holds, and an `Option` of one.
macro_rules! reference {
    ($($ty:ty),* $(,)?) => {
        $(
            impl Reference for $ty {
                fn spell(&self) -> Option<String> {
                    Some(self.to_string())
                }
            }

            impl Reference for Option<$ty> {
                fn spell(&self) -> Option<String> {
                    self.as_ref().map(ToString::to_string)
                }
            }
        )*
    };
}

reference!(
    uuid::Uuid,
    crate::TenantId,
    String,
    i8,
    i16,
    i32,
    i64,
    isize,
    u8,
    u16,
    u32,
    u64,
    usize,
);

/// A copy of `row` without its relations: see [`PanelModel::UNLOADS_RELATIONS`].
pub(crate) fn bare<M: PanelModel + Clone>(row: &M) -> M {
    row.bare()
}

mod field;
#[doc(hidden)]
pub mod pick;

pub use field::FieldColumn;

#[cfg(test)]
mod tests;
