//! How the [`ModelColumns`](derive@crate::ModelColumns) derive picks a field's column and filter
//! from its type, a choice no trait bound can make: each `Pick*` trait is implemented on
//! [`ModelField`] behind one more reference than the next, so method lookup on `&&&&&&ModelField`
//! takes the first whose bounds the field's type meets.
//!
//! A pick returns the column or the filter itself, so a field's `#[columns(..)]` keys apply to it
//! through the traits below, each implemented by what it fits: a key on a column it does not fit
//! fails to compile at the key, with the trait's message.

use std::{marker::PhantomData, sync::Arc};

use toasty::{
    schema::{Field, Model},
    stmt::{IntoExpr, List},
};

use crate::{
    BooleanColumn, Currency, DateColumn, DateValue, Detail, EmbeddedColumn, Lens, ListColumn,
    Numeric, RepeaterColumn, Table, Text, TextColumn,
    schema::{EmbeddedForm, FormInput, Options},
    table::filter::{DateFilter, FilterDate, SelectFilter, TernaryFilter},
    values::FormScalar,
};

/// One model field, as the derive hands it to the `Pick*` traits.
pub struct ModelField<M, T, P = T>(pub Lens<M, T, P>);

/// The type a field of type `T` names its path by: `T`, except `List<T>` for a `Vec<T>`.
pub type PathOf<T> = <T as Field>::ExprTarget;

/// The table column of a field only the detail page shows: an embedded value or a list.
pub struct Absent;

/// The column of a field no column reads.
pub struct NoColumn;

/// The filter of a field no built-in filter matches, which no table takes.
pub struct NoFilter<T>(PhantomData<T>);

/// Declares a column `Pick*` trait: the field's table column and its detail page column.
macro_rules! column_pick {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub trait $name<M> {
            type Table;
            type Detail;
            fn table_column(&self) -> Self::Table;
            fn detail_column(&self) -> Self::Detail;
        }
    };
}

column_pick! {
    /// A [`DateValue`]: a sortable calendar day in the table, the stored value on the detail
    /// page, which keeps a timestamp's time.
    PickDate
}

impl<M, T> PickDate<M> for &&&&&&ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: DateValue + FormScalar,
{
    type Table = DateColumn<M, T>;
    type Detail = TextColumn<M, T>;

    fn table_column(&self) -> Self::Table {
        DateColumn::new(self.0.clone()).sortable()
    }

    fn detail_column(&self) -> Self::Detail {
        TextColumn::new(self.0.clone())
    }
}

column_pick! {
    /// A `bool`: a sortable yes or no.
    PickToggle
}

impl<M> PickToggle<M> for &&&&&ModelField<M, bool>
where
    M: Model + Send + Sync + 'static,
{
    type Table = BooleanColumn<M>;
    type Detail = BooleanColumn<M>;

    fn table_column(&self) -> Self::Table {
        BooleanColumn::new(self.0.clone()).sortable()
    }

    fn detail_column(&self) -> Self::Detail {
        BooleanColumn::new(self.0.clone())
    }
}

column_pick! {
    /// A `String` or `Option<String>`: a sortable, searchable text.
    PickString
}

impl<M, T> PickString<M> for &&&&ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: FormScalar + Field<Inner = String> + Send + Sync + 'static,
{
    type Table = TextColumn<M, T>;
    type Detail = TextColumn<M, T>;

    fn table_column(&self) -> Self::Table {
        TextColumn::new(self.0.clone()).sortable().searchable()
    }

    fn detail_column(&self) -> Self::Detail {
        TextColumn::new(self.0.clone())
    }
}

column_pick! {
    /// Any other [`FormScalar`]: a text, an `Options` enum's by its label, sortable when the type
    /// is [`SORTABLE`](FormScalar::SORTABLE).
    PickScalar
}

impl<M, T> PickScalar<M> for &&&ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    type Table = TextColumn<M, T>;
    type Detail = TextColumn<M, T>;

    fn table_column(&self) -> Self::Table {
        let column = TextColumn::new(self.0.clone());
        if T::SORTABLE {
            column.sortable()
        } else {
            column
        }
    }

    fn detail_column(&self) -> Self::Detail {
        TextColumn::new(self.0.clone())
    }
}

column_pick! {
    /// An [`EmbeddedForm`] value: its leaves on the detail page, no table column.
    PickEmbedded
}

impl<M, T> PickEmbedded<M> for &&ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: EmbeddedForm + Send + Sync + 'static,
{
    type Table = Absent;
    type Detail = EmbeddedColumn<M, T>;

    fn table_column(&self) -> Absent {
        Absent
    }

    fn detail_column(&self) -> Self::Detail {
        EmbeddedColumn::new(self.0.clone())
    }
}

column_pick! {
    /// A list of [`FormInput`] values: its items on the detail page, no table column.
    PickRepeater
}

impl<M, T> PickRepeater<M> for &ModelField<M, Vec<T>, List<T>>
where
    M: Model + Send + Sync + 'static,
    T: FormInput,
{
    type Table = Absent;
    type Detail = RepeaterColumn<M, T>;

    fn table_column(&self) -> Absent {
        Absent
    }

    fn detail_column(&self) -> Self::Detail {
        RepeaterColumn::new(self.0.clone())
    }
}

column_pick! {
    /// A list of [`FormScalar`] values a list column stores: a badge per value.
    PickList
}

// The same depth as `PickRepeater`: no item type is both a `FormInput` and a `FormScalar`.
impl<M, T> PickList<M> for &ModelField<M, Vec<T>, List<T>>
where
    M: Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    type Table = ListColumn<M, T>;
    type Detail = ListColumn<M, T>;

    fn table_column(&self) -> Self::Table {
        ListColumn::new(self.0.clone())
    }

    fn detail_column(&self) -> Self::Detail {
        ListColumn::new(self.0.clone())
    }
}

column_pick! {
    /// Any other type, which no column reads: nowhere.
    PickNone
}

impl<M, T, P> PickNone<M> for ModelField<M, T, P> {
    type Table = NoColumn;
    type Detail = NoColumn;

    fn table_column(&self) -> NoColumn {
        NoColumn
    }

    fn detail_column(&self) -> NoColumn {
        NoColumn
    }
}

/// Declares a filter `Pick*` trait: the filter `#[columns(filter)]` registers for the field.
macro_rules! filter_pick {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub trait $name<M> {
            type Filter;
            fn table_filter(&self) -> Self::Filter;
        }
    };
}

filter_pick! {
    /// A `bool`: a [`TernaryFilter`].
    PickTernaryFilter
}

impl<M> PickTernaryFilter<M> for &&&ModelField<M, bool>
where
    M: Model + Send + Sync + 'static,
{
    type Filter = TernaryFilter<M>;

    fn table_filter(&self) -> Self::Filter {
        TernaryFilter::new(self.0.clone())
    }
}

filter_pick! {
    /// A [`FilterDate`]: a [`DateFilter`] over a range of days.
    PickDateFilter
}

impl<M, T> PickDateFilter<M> for &&ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: FilterDate,
{
    type Filter = DateFilter<M>;

    fn table_filter(&self) -> Self::Filter {
        DateFilter::new(self.0.clone()).range()
    }
}

filter_pick! {
    /// An [`Options`] enum: a [`SelectFilter`] over its options.
    PickSelectFilter
}

impl<M, T> PickSelectFilter<M> for &ModelField<M, T>
where
    M: Model + Send + Sync + 'static,
    T: Options + FormScalar + IntoExpr<T> + Send + Sync + 'static,
{
    type Filter = SelectFilter<M>;

    fn table_filter(&self) -> Self::Filter {
        SelectFilter::of(self.0.clone())
    }
}

filter_pick! {
    /// Any other type, which no built-in filter matches.
    PickNoFilter
}

impl<M, T, P> PickNoFilter<M> for ModelField<M, T, P> {
    type Filter = NoFilter<T>;

    fn table_filter(&self) -> Self::Filter {
        NoFilter(PhantomData)
    }
}

/// Places a picked column in the table or on the detail page; [`Absent`] and [`NoColumn`] place
/// nothing.
pub trait Place<M> {
    fn place_in_table(self, table: Table<M>) -> Table<M>;
    fn place_in_detail(self, detail: Detail<M>) -> Detail<M>;
}

/// Places a picked filter in the table.
#[diagnostic::on_unimplemented(
    message = "`#[columns(filter)]` has no filter for this field: `{Self}`",
    label = "no built-in filter matches the field's type",
    note = "a `bool` takes a `TernaryFilter`, a date or timestamp a `DateFilter`, and an \
            `Options` enum a `SelectFilter`; declare any other filter on the resource's table"
)]
pub trait PlaceFilter<M> {
    fn place_filter(self, table: Table<M>) -> Table<M>;
}

/// `#[columns(label = "..")]`: the column's header and entry label, and its filter's.
#[diagnostic::on_unimplemented(
    message = "`#[columns(label = ..)]` labels a column, and no column shows this field",
    label = "the field's type picks no column"
)]
pub trait Label {
    fn label(self, label: Text) -> Self;
}

/// `#[columns(badge)]`: a text column's value as a badge.
#[diagnostic::on_unimplemented(
    message = "`#[columns(badge)]` shows a text column's value as a badge, and this field's \
               column is a `{Self}`",
    label = "the field's type picks a column that renders no badge"
)]
pub trait Badge {
    fn badge(self) -> Self;
}

/// `#[columns(money = ..)]`: a number spelled as an amount of a currency.
#[diagnostic::on_unimplemented(
    message = "`#[columns(money = ..)]` spells a number as an amount, and this field's column is \
               a `{Self}`",
    label = "the field is no integer, float or decimal, nor an `Option` of one"
)]
pub trait Money {
    fn money(self, currency: Currency) -> Self;
}

/// `#[columns(hidden_by_default)]`: a table column each reader may show, hidden at first.
#[diagnostic::on_unimplemented(
    message = "`#[columns(hidden_by_default)]` hides a table column, and the table shows no \
               column for this field",
    label = "only the detail page, or nothing, shows the field's type"
)]
pub trait HiddenByDefault {
    fn hidden_by_default(self) -> Self;
}

/// Implements [`Place`], [`Label`] and, with `hidden`, [`HiddenByDefault`] for a built-in column.
macro_rules! column_keys {
    ($column:ty, [$($generic:ident),+] where [$($bounds:tt)*]) => {
        impl<$($generic),+> Place<M> for $column
        where
            M: Model + Send + Sync + 'static,
            $($bounds)*
        {
            fn place_in_table(self, table: Table<M>) -> Table<M> {
                table.column(self)
            }

            fn place_in_detail(self, detail: Detail<M>) -> Detail<M> {
                detail.column(self)
            }
        }

        impl<$($generic),+> Label for $column
        where
            M: Model + Send + Sync + 'static,
            $($bounds)*
        {
            fn label(self, label: Text) -> Self {
                <$column>::label(self, label)
            }
        }
    };
    ($column:ty, [$($generic:ident),+] where [$($bounds:tt)*], hidden) => {
        column_keys!($column, [$($generic),+] where [$($bounds)*]);

        impl<$($generic),+> HiddenByDefault for $column
        where
            M: Model + Send + Sync + 'static,
            $($bounds)*
        {
            fn hidden_by_default(self) -> Self {
                <$column>::hidden_by_default(self)
            }
        }
    };
}

column_keys!(TextColumn<M, T>, [M, T] where [T: Send + Sync + 'static,], hidden);
column_keys!(DateColumn<M, T>, [M, T] where [T: DateValue,], hidden);
column_keys!(BooleanColumn<M>, [M] where [], hidden);
column_keys!(EmbeddedColumn<M, T>, [M, T] where [T: EmbeddedForm + Send + Sync + 'static,]);
column_keys!(RepeaterColumn<M, T>, [M, T] where [T: FormInput,]);
column_keys!(ListColumn<M, T>, [M, T] where [T: FormScalar + Send + Sync + 'static,], hidden);

impl<M> Place<M> for Absent {
    fn place_in_table(self, table: Table<M>) -> Table<M> {
        table
    }

    fn place_in_detail(self, detail: Detail<M>) -> Detail<M> {
        detail
    }
}

/// The table leaves the field out, and the detail page's column takes the label.
impl Label for Absent {
    fn label(self, _label: Text) -> Self {
        self
    }
}

impl<M> Place<M> for NoColumn {
    fn place_in_table(self, table: Table<M>) -> Table<M> {
        table
    }

    fn place_in_detail(self, detail: Detail<M>) -> Detail<M> {
        detail
    }
}

impl<M, T> Badge for TextColumn<M, T>
where
    M: Model,
{
    fn badge(self) -> Self {
        TextColumn::badge(self)
    }
}

impl<M, T> Money for TextColumn<M, T>
where
    M: Model,
    T: Numeric + 'static,
{
    fn money(self, currency: Currency) -> Self {
        TextColumn::money(self, currency)
    }
}

/// Implements [`PlaceFilter`] and [`Label`] for a built-in filter.
macro_rules! filter_keys {
    ($($filter:ident),+) => {
        $(
            impl<M> PlaceFilter<M> for $filter<M>
            where
                M: Model + Send + Sync + 'static,
            {
                fn place_filter(self, mut table: Table<M>) -> Table<M> {
                    table.push_filter(Arc::new(self));
                    table
                }
            }

            impl<M> Label for $filter<M>
            where
                M: Model + Send + Sync + 'static,
            {
                fn label(self, label: Text) -> Self {
                    $filter::label(self, label)
                }
            }
        )+
    };
}

filter_keys!(TernaryFilter, DateFilter, SelectFilter);

/// The table refuses the filter through [`PlaceFilter`]; the label adds no second error.
impl<T> Label for NoFilter<T> {
    fn label(self, _label: Text) -> Self {
        self
    }
}
