//! [`ToggleColumn`], [`SelectColumn`] and [`TextInputColumn`]: one field edited in its cell,
//! through the record's edit route.

use derive_where::derive_where;
use toasty::stmt::OrderByExpr;
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth, boolean::boolean_cell, sort_order};
use crate::{Lens, schema::Options, values::FormScalar};

mod view;

/// The control an editable column renders, which the resource's form must edit its field with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    /// A checkbox or a switch.
    Toggle,
    /// A choice over static options.
    Select,
    /// A one-line text input.
    Text,
}

impl Editor {
    /// Whether the control saves as soon as it changes, rather than on Enter.
    pub(crate) fn saves_on_change(self) -> bool {
        matches!(self, Self::Toggle | Self::Select)
    }

    /// The control the resource's form must edit the field with, as a mount error names it.
    pub(crate) fn control(self) -> &'static str {
        match self {
            Self::Toggle => "a checkbox or switch",
            Self::Select => "a choice over static options",
            Self::Text => "a one-line text field",
        }
    }
}

/// A column of a `bool` field rendered as a switch that saves the record when it flips.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// tablo_core::ToggleColumn::new(tablo_core::lens!(Post.featured));
/// ```
///
/// See [Editing in a cell](ToggleColumn#editing-in-a-cell) for where the switch renders and
/// what it posts.
///
/// # Editing in a cell
///
/// A list, a relation table or a widget's record table renders the control on a row whose record
/// the user may update and that is not in the trash, for a field the user may change; anywhere
/// else, and on a detail page, the cell shows the value. The control posts its field alone to the
/// record's edit route, with the record's version, so the save runs every check the edit page's
/// does and lands back on the table. A refused value opens the edit page with its errors.
///
/// The mount refuses the column unless the resource's form edits the field with the matching
/// control, a checkbox or switch here, shown and enabled whatever the other fields hold. Flipping
/// the switch and changing a select save through the runtime's one line of script; a text input
/// saves on Enter.
///
/// The control spells the value as the field's form spelling, as a derived record form does, so a
/// hand-written [`RecordForm`](trait@crate::RecordForm) must read the key as that spelling.
#[derive_where(Clone, Debug)]
pub struct ToggleColumn<M> {
    lens: Lens<M, bool>,
    base: ColumnBase,
    sortable: bool,
}

impl<M> ToggleColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `bool` field `lens` reads.
    pub fn new(lens: Lens<M, bool>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
        }
    }

    base_builders!(sortable);
}

impl<M> Column<M> for ToggleColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, cx: &Cx, row: &M) -> String {
        crate::Text::message(if *self.lens.read(row) {
            "tablo-yes"
        } else {
            "tablo-no"
        })
        .localize(cx)
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        boolean_cell(cx, *self.lens.read(row), self.text(cx, row))
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

impl<M> crate::column::ColumnInternals<M> for ToggleColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_internals!(binding);

    fn edits(&self) -> Option<Editor> {
        Some(Editor::Toggle)
    }

    fn editor<'a>(&self, cx: &'a Cx, row: &M) -> Option<BoxView<'a>> {
        Some(view::toggle(
            cx,
            self.base.name(),
            self.base.label().localize(cx),
            *self.lens.read(row),
        ))
    }
}

/// A column of an [`Options`] field rendered as a select that saves the record when it changes.
///
/// ```rust
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// # enum Status { Draft, Published }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, status: Status }
/// tablo_core::SelectColumn::new(tablo_core::lens!(Post.status));
/// ```
///
/// It offers the type's options, and edits as [`ToggleColumn`](ToggleColumn#editing-in-a-cell)
/// says, the form's control a choice offering the same options under the same labels.
#[derive_where(Clone, Debug)]
pub struct SelectColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    sortable: bool,
}

impl<M, T> SelectColumn<M, T>
where
    M: toasty::schema::Model,
    T: Options + FormScalar + Send + Sync + 'static,
{
    /// Bind the column to the field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
        }
    }

    base_builders!(sortable);
}

impl<M, T> Column<M> for SelectColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: Options + FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, cx: &Cx, row: &M) -> String {
        T::label_of(&self.lens.read(row).to_form()).localize(cx)
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

impl<M, T> crate::column::ColumnInternals<M> for SelectColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: Options + FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);

    fn edits(&self) -> Option<Editor> {
        Some(Editor::Select)
    }

    fn edit_options(&self) -> Vec<(String, crate::Text)> {
        T::options()
    }

    fn editor<'a>(&self, cx: &'a Cx, row: &M) -> Option<BoxView<'a>> {
        let options = T::options()
            .into_iter()
            .map(|(value, label)| (value, label.localize(cx)))
            .collect();
        Some(view::select(
            cx,
            self.base.name(),
            self.base.label().localize(cx),
            self.lens.read(row).to_form(),
            options,
        ))
    }
}

/// A column of one field rendered as a text input that saves the record on Enter.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Product { #[key] #[auto] id: uuid::Uuid, stock: i64 }
/// tablo_core::TextInputColumn::new(tablo_core::lens!(Product.stock));
/// ```
///
/// The input takes the type's input type, a number for an integer, and edits as
/// [`ToggleColumn`](ToggleColumn#editing-in-a-cell) says, the form's control a one-line text
/// field that is no password.
#[derive_where(Clone, Debug)]
pub struct TextInputColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    sortable: bool,
}

impl<M, T> TextInputColumn<M, T>
where
    M: toasty::schema::Model,
    T: FormScalar + Send + Sync + 'static,
{
    /// Bind the column to the field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
            sortable: false,
        }
    }

    base_builders!(sortable);
}

impl<M, T> Column<M> for TextInputColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!();

    fn text(&self, cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_label().localize(cx)
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

impl<M, T> crate::column::ColumnInternals<M> for TextInputColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_internals!(binding);

    fn edits(&self) -> Option<Editor> {
        Some(Editor::Text)
    }

    fn editor<'a>(&self, cx: &'a Cx, row: &M) -> Option<BoxView<'a>> {
        let value = T::input_value(&self.lens.read(row).to_form(), &crate::time_zone(cx));
        Some(view::text_input(
            cx,
            view::TextInput {
                name: self.base.name(),
                label: self.base.label().localize(cx),
                value,
                input_type: T::INPUT_TYPE,
                step: T::STEP,
            },
        ))
    }
}
