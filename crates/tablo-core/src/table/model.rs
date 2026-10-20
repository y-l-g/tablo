//! [`ModelColumns`]: the table and detail page a model reads as by default.

use toasty::{schema::Model, stmt::IntoExpr};

use crate::{Detail, Table};

/// The table and detail page a model reads as when its resource declares none: what a
/// [`ResourceDef`](crate::ResourceDef) without a [`table`](crate::ResourceDef::table) lists and
/// without a [`detail`](crate::ResourceDef::detail) shows.
///
/// Derive it with [`ModelColumns`](derive@crate::ModelColumns), which gives a column to each field
/// a person reads, whether a form or Toasty writes it. An impl that keeps the defaults here lists
/// no column and turns the detail page off, for a resource that declares both.
#[diagnostic::on_unimplemented(
    message = "`{Self}` declares no default columns",
    label = "a resource's model implements `ModelColumns`",
    note = "add `#[derive(tablo::ModelColumns)]` to `{Self}`, or `impl tablo::ModelColumns for \
            {Self} {{}}` when its resources declare their own table and detail page"
)]
pub trait ModelColumns: Model + IntoExpr<Self> + Sized {
    /// The model's [`TenantId`](crate::TenantId) fields, by name: the derive lists each field
    /// typed `TenantId` or `Option<TenantId>`. Mounting a panel refuses a resource over a model
    /// that lists one and declares no [`tenancy`](crate::ResourceDef::tenancy), which would serve
    /// every tenant's rows. An impl that keeps the empty default opts out of that check.
    const TENANT_COLUMNS: &'static [&'static str] = &[];

    /// The default table, which lists no column here.
    fn table() -> Table<Self> {
        Table::new(())
    }

    /// The default detail page, which is off here.
    fn detail() -> Detail<Self> {
        Detail::empty()
    }
}

/// How the [`ModelColumns`](derive@crate::ModelColumns) derive picks a field's column from its
/// type, a choice no trait bound can make: each `Pick*` trait is implemented on
/// [`ModelField`](pick::ModelField) behind one more reference than the next, so method lookup on
/// `&&&&&&ModelField` takes the first whose bounds the field's type meets.
#[doc(hidden)]
pub mod pick {
    use toasty::{
        schema::{Field, Model},
        stmt::List,
    };

    use crate::{
        BooleanColumn, DateColumn, DateValue, Detail, EmbeddedColumn, Lens, RepeaterColumn, Table,
        TextColumn,
        schema::{EmbeddedForm, FormInput},
        values::FormScalar,
    };

    /// One model field, as the derive hands it to the `Pick*` traits.
    pub struct ModelField<M, T, P = T>(pub Lens<M, T, P>);

    /// The type a field of type `T` names its path by: `T`, except `List<T>` for a `Vec<T>`.
    pub type PathOf<T> = <T as Field>::ExprTarget;

    /// A [`DateValue`]: a sortable calendar day in the table, the stored value on the detail page,
    /// which keeps a timestamp's time.
    pub trait PickDate<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T> PickDate<M> for &&&&&&ModelField<M, T>
    where
        M: Model + Send + Sync + 'static,
        T: DateValue + FormScalar,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table.column(DateColumn::new(self.0.clone()).sortable())
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(TextColumn::new(self.0.clone()))
        }
    }

    /// A `bool`: a sortable yes or no.
    pub trait PickToggle<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M> PickToggle<M> for &&&&&ModelField<M, bool>
    where
        M: Model + Send + Sync + 'static,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table.column(BooleanColumn::new(self.0.clone()).sortable())
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(BooleanColumn::new(self.0.clone()))
        }
    }

    /// A `String` or `Option<String>`: a sortable, searchable text.
    pub trait PickString<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T> PickString<M> for &&&&ModelField<M, T>
    where
        M: Model + Send + Sync + 'static,
        T: FormScalar + Field<Inner = String> + Send + Sync + 'static,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table.column(TextColumn::new(self.0.clone()).sortable().searchable())
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(TextColumn::new(self.0.clone()))
        }
    }

    /// Any other [`FormScalar`]: a sortable text, an `Options` enum's by its label.
    pub trait PickScalar<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T> PickScalar<M> for &&&ModelField<M, T>
    where
        M: Model + Send + Sync + 'static,
        T: FormScalar + Send + Sync + 'static,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table.column(TextColumn::new(self.0.clone()).sortable())
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(TextColumn::new(self.0.clone()))
        }
    }

    /// An [`EmbeddedForm`] value: its leaves on the detail page, no table column.
    pub trait PickEmbedded<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T> PickEmbedded<M> for &&ModelField<M, T>
    where
        M: Model + Send + Sync + 'static,
        T: EmbeddedForm + Send + Sync + 'static,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(EmbeddedColumn::new(self.0.clone()))
        }
    }

    /// A list of [`FormInput`] values: its items on the detail page, no table column.
    pub trait PickRepeater<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T> PickRepeater<M> for &ModelField<M, Vec<T>, List<T>>
    where
        M: Model + Send + Sync + 'static,
        T: FormInput,
    {
        fn table(&self, table: Table<M>) -> Table<M> {
            table
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail.column(RepeaterColumn::new(self.0.clone()))
        }
    }

    /// Any other type, which no column reads: nowhere.
    pub trait PickNone<M> {
        fn table(&self, table: Table<M>) -> Table<M>;
        fn detail(&self, detail: Detail<M>) -> Detail<M>;
    }

    impl<M, T, P> PickNone<M> for ModelField<M, T, P> {
        fn table(&self, table: Table<M>) -> Table<M> {
            table
        }

        fn detail(&self, detail: Detail<M>) -> Detail<M> {
            detail
        }
    }
}

#[cfg(test)]
mod tests;
