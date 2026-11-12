//! [`FieldColumn`]: one model field, as a model's derived `columns()` hands it over.

use derive_where::derive_where;
use toasty::{
    schema::{Field, Model},
    stmt::List,
};

use crate::{
    BooleanColumn, ColorColumn, DateColumn, DateValue, Detail, EmbeddedColumn, FileColumn,
    IconColumn, ImageColumn, IntoColumns, IntoDetail, Lens, ListColumn, RepeaterColumn,
    SelectColumn, Text, TextColumn, TextInputColumn, ToggleColumn,
    column::BoxColumn,
    schema::{EmbeddedForm, FormInput, Options},
    values::FormScalar,
};

/// One field of a model, as the [`PanelModel`](derive@crate::PanelModel) derive's
/// `columns()` hands it over: `Product::columns().qty`.
///
/// Placed as it is, in a [`Table`](crate::Table) or on a [`Detail`] page, it is the column the
/// derive picks for the field's type there, with the field's `#[columns(..)]` keys applied: the
/// column the model's default table or detail page shows. A kind method, such as
/// [`text`](Self::text), [`date`](Self::date) or [`toggle`](Self::toggle), is that column type's
/// own builder over the field instead, labelled by the field's `#[columns(label = "..")]`; each
/// compiles only for a field type its column reads.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Product { #[key] #[auto] id: uuid::Uuid, name: String, qty: i64, active: bool }
/// use tablo_core::Table;
///
/// let columns = Product::columns();
/// Table::new((
///     // The derive's pick: a sortable, searchable text.
///     columns.name,
///     // A text column of the derive's, reshaped.
///     columns.qty.text().numeric(0).sortable(),
///     // A switch saving the record as it flips, in place of the derive's yes or no.
///     columns.active.toggle(),
/// ));
/// ```
///
/// [`lens`](Self::lens) hands over the field's [`Lens`] for a filter, a tab or a field access
/// rule; [`lens!`](crate::lens!) builds one for a path the derive does not reach, such as an
/// embedded value's own field.
#[derive_where(Clone)]
pub struct FieldColumn<M, T, P = T> {
    lens: Lens<M, T, P>,
    label: Option<Text>,
    table: fn() -> Vec<BoxColumn<M>>,
    detail: fn() -> Detail<M>,
}

impl<M, T, P> FieldColumn<M, T, P> {
    /// The field `lens` reads, labelled `label`, placed in a table as `table` builds it and on a
    /// detail page as `detail` does; only the derive builds one.
    #[doc(hidden)]
    pub fn new(
        lens: Lens<M, T, P>,
        label: Option<Text>,
        table: fn() -> Vec<BoxColumn<M>>,
        detail: fn() -> Detail<M>,
    ) -> Self {
        Self {
            lens,
            label,
            table,
            detail,
        }
    }

    /// The field's lens: its query path paired with its reader.
    pub fn lens(&self) -> Lens<M, T, P> {
        self.lens.clone()
    }
}

/// The kind methods of [`FieldColumn`]: each binds its column type to the field, labelled by the
/// field's `label` key.
macro_rules! kinds {
    ($($(#[$doc:meta])* $method:ident => $column:ty;)+) => {
        $(
            $(#[$doc])*
            pub fn $method(self) -> $column {
                let column = <$column>::new(self.lens);
                match self.label {
                    Some(label) => column.label(label),
                    None => column,
                }
            }
        )+
    };
}

impl<M, T> FieldColumn<M, T>
where
    M: Model,
    T: FormScalar + Send + Sync + 'static,
{
    kinds! {
        /// A [`TextColumn`]: the value's text, or an `Options` enum's label.
        text => TextColumn<M, T>;
        /// An [`IconColumn`]: an `Options` enum's icon in place of its label.
        icon => IconColumn<M, T>;
        /// A [`TextInputColumn`]: a text input saving the record as it changes.
        text_input => TextInputColumn<M, T>;
    }
}

impl<M, T> FieldColumn<M, T>
where
    M: Model,
    T: DateValue,
{
    kinds! {
        /// A [`DateColumn`]: the value's calendar day.
        date => DateColumn<M, T>;
    }
}

impl<M, T> FieldColumn<M, T>
where
    M: Model,
    T: Options + FormScalar + Send + Sync + 'static,
{
    kinds! {
        /// A [`SelectColumn`]: a select over the enum's options, saving the record as it
        /// changes.
        select => SelectColumn<M, T>;
    }
}

impl<M, T> FieldColumn<M, T>
where
    M: Model,
    T: Field<Inner = String>,
{
    kinds! {
        /// A [`ColorColumn`]: a swatch of the CSS color the text holds.
        color => ColorColumn<M, T>;
        /// An [`ImageColumn`]: the image the text names.
        image => ImageColumn<M, T>;
    }

    /// A [`RichTextColumn`](crate::RichTextColumn): the cleaned HTML the text holds.
    #[cfg(feature = "rich-text")]
    pub fn rich_text(self) -> crate::RichTextColumn<M, T> {
        let column = crate::RichTextColumn::new(self.lens);
        match self.label {
            Some(label) => column.label(label),
            None => column,
        }
    }
}

impl<M> FieldColumn<M, bool>
where
    M: Model,
{
    kinds! {
        /// A [`BooleanColumn`]: a yes or no.
        boolean => BooleanColumn<M>;
        /// A [`ToggleColumn`]: a switch saving the record as it flips.
        toggle => ToggleColumn<M>;
    }
}

impl<M> FieldColumn<M, String>
where
    M: Model,
{
    kinds! {
        /// A [`FileColumn`]: a link to the upload the text names.
        file => FileColumn<M>;
    }
}

impl<M, T> FieldColumn<M, T>
where
    M: Model,
    T: EmbeddedForm,
{
    kinds! {
        /// An [`EmbeddedColumn`]: the embedded value's leaves.
        embedded => EmbeddedColumn<M, T>;
    }
}

impl<M, T> FieldColumn<M, Vec<T>, List<T>>
where
    M: Model,
    T: FormScalar,
{
    kinds! {
        /// A [`ListColumn`]: a badge per value.
        list => ListColumn<M, T>;
    }
}

impl<M, T> FieldColumn<M, Vec<T>, List<T>>
where
    M: Model,
    T: FormInput,
{
    kinds! {
        /// A [`RepeaterColumn`]: the list's items.
        repeater => RepeaterColumn<M, T>;
    }
}

/// The table column the derive picks, or none for a field only a detail page shows.
impl<M, T, P> IntoColumns<M> for FieldColumn<M, T, P> {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        (self.table)()
    }
}

/// The detail page column the derive picks.
impl<M, T, P> IntoDetail<M> for FieldColumn<M, T, P> {
    fn into_detail(self) -> Detail<M> {
        (self.detail)()
    }
}

impl<M, T, P> From<FieldColumn<M, T, P>> for Lens<M, T, P> {
    fn from(column: FieldColumn<M, T, P>) -> Self {
        column.lens
    }
}
