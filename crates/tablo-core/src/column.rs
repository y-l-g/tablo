//! Columns, which a [`Table`](crate::Table) lists and a [`Detail`](crate::Detail) shows: the
//! [`Column`] trait, the built-in [`TextColumn`], [`DateColumn`], [`ComputedColumn`],
//! [`RelationColumn`], [`CountColumn`], [`BooleanColumn`], [`FileColumn`], [`ColorColumn`],
//! [`ImageColumn`], [`EmbeddedColumn`], [`RepeaterColumn`] and [`RichTextColumn`], and the
//! [`IntoColumns`] seam.
//!
//! The module sits below both: `table` re-exports it, and `detail` implements
//! [`IntoDetail`](crate::IntoDetail) for each built-in column.

use std::sync::Arc;

use derive_where::derive_where;
use toasty::stmt::{Expr, List, OrderByExpr, Path, Query};
use topcoat::{context::Cx, view::*};

use crate::{display::Alignment, schema::Binding};

/// The `label` and `width` builders of a column over a [`ColumnBase`], inside its inherent impl.
macro_rules! base_builders {
    () => {
        /// Replace the label the field's name gives it.
        pub fn label(mut self, label: impl Into<$crate::Text>) -> Self {
            self.base.label = Some(label.into());
            self
        }

        /// Declare this column's width.
        pub fn width(mut self, width: $crate::column::ColumnWidth) -> Self {
            self.base.width = width;
            self
        }

        toggle_builders!(base.toggle);
    };
    (sortable) => {
        base_builders!();

        /// Make the header a sort link.
        pub fn sortable(mut self) -> Self {
            self.sortable = true;
            self
        }
    };
}

/// The `toggleable` and `hidden_by_default` builders of a column holding its [`ColumnToggle`] at
/// the field path given, inside its inherent impl.
macro_rules! toggle_builders {
    ($($field:ident).+) => {
        /// Let each reader hide this column from their list; it starts shown.
        pub fn toggleable(mut self) -> Self {
            self.$($field).+ = $crate::column::ColumnToggle::ShownByDefault;
            self
        }

        /// Let each reader show this column in their list; it starts hidden.
        pub fn hidden_by_default(mut self) -> Self {
            self.$($field).+ = $crate::column::ColumnToggle::HiddenByDefault;
            self
        }
    };
}

/// The [`Column`] methods a column over a [`ColumnBase`] answers from it, inside its `Column`
/// impl; `binding` adds the binding's `misdeclared`.
macro_rules! base_column_methods {
    () => {
        base_column_methods!(@shared);

        fn reads(&self) -> Option<$crate::lens::Reads<M>> {
            Some(self.base.reads())
        }
    };
    (binding) => {
        base_column_methods!();
        base_column_methods!(@misdeclared);
    };
    // A column that answers `reads` itself.
    (binding, reads) => {
        base_column_methods!(@shared);
        base_column_methods!(@misdeclared);
    };
    (@misdeclared) => {
        fn misdeclared(&self) -> Option<$crate::DeclarationErrorKind> {
            self.base.binding.misdeclared()
        }
    };
    (@shared) => {
        fn related(&self) -> $crate::column::Related {
            $crate::column::Related::None
        }

        fn name(&self) -> &str {
            self.base.name()
        }

        fn label(&self) -> $crate::Text {
            self.base.label()
        }

        fn column_width(&self) -> $crate::column::ColumnWidth {
            self.base.width
        }

        fn toggle(&self) -> $crate::column::ColumnToggle {
            self.base.toggle
        }
    };
}

fn sort_order<M, T>(path: &Path<M, T>, descending: bool) -> OrderByExpr {
    if descending {
        path.clone().desc()
    } else {
        path.clone().asc()
    }
}

mod related;
pub use related::Related;
mod boolean;
mod color;
mod computed;
mod date;
mod embedded;
mod file;
mod image;
mod relation;
mod repeater;
mod rich_text;
mod text;

pub use boolean::BooleanColumn;
pub use color::ColorColumn;
pub use computed::ComputedColumn;
pub use date::{DateColumn, DateValue};
pub use embedded::EmbeddedColumn;
pub use file::FileColumn;
pub use image::ImageColumn;
pub use relation::{CountColumn, RelationColumn, RelationLens, ToOneRelation};
pub use repeater::RepeaterColumn;
pub use rich_text::RichTextColumn;
pub use text::{TextColumn, contains_expr};
#[cfg(test)]
use {color::is_css_color, text::escape_like_pattern};

/// One column of a table or a [`Detail`](crate::Detail) declares its label, its value read off
/// the record, and its query predicates.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # use tablo_core::{Text, extend::Column};
/// # use topcoat::context::Cx;
/// struct Initials;
///
/// impl Column<User> for Initials {
///     fn name(&self) -> &str {
///         "initials"
///     }
///     fn label(&self) -> Text {
///         "Initials".into()
///     }
///     fn text(&self, _cx: &Cx, row: &User) -> String {
///         row.name
///             .split_whitespace()
///             .filter_map(|w| w.chars().next())
///             .collect()
///     }
/// }
/// ```
///
/// Only [`name`](Self::name), [`label`](Self::label) and
/// [`text`](Self::text) are required.
pub trait Column<M>: Send + Sync {
    /// The column's identifier, distinct within its table.
    fn name(&self) -> &str;

    /// The header text.
    fn label(&self) -> crate::Text;

    /// The row's value as plain text.
    fn text(&self, cx: &Cx, row: &M) -> String;

    /// The row's table cell.
    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        crate::schema::value_cell(cx, &self.text(cx, row))
    }

    /// The record's entry on a detail page: the [`label`](Self::label) over the
    /// [`cell`](Self::cell).
    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        crate::schema::read_only(cx, &self.label(), self.cell(cx, row))
    }

    /// The width the column claims.
    fn column_width(&self) -> ColumnWidth {
        ColumnWidth::Auto
    }

    /// Whether each reader may hide or show the column in their list, and how it starts. A
    /// detail page shows every column.
    fn toggle(&self) -> ColumnToggle {
        ColumnToggle::Fixed
    }

    /// Where the header and the cells align their content.
    fn alignment(&self) -> Alignment {
        Alignment::Start
    }

    /// Whether the column joins the table's search.
    fn is_searchable(&self) -> bool {
        false
    }

    /// The predicate a search for `term` adds.
    fn search_expr(&self, _term: &str) -> Option<Expr<bool>> {
        None
    }

    /// Whether the header links to a sort on this column.
    fn is_sortable(&self) -> bool {
        false
    }

    /// The ordering a sort on this column applies.
    fn order_by(&self, _descending: bool) -> Option<OrderByExpr> {
        None
    }

    /// The relations [`text`](Self::text), [`cell`](Self::cell) and [`entry`](Self::entry) read.
    fn includes(&self) -> Includes<M> {
        Includes::new()
    }

    /// Check which related records [`includes`](Self::includes) loaded into `rows` the related
    /// resource shows the user, before any cell renders: a cell shows only those. Every loader
    /// that applies the includes runs it through `ex`, the executor that loaded the rows.
    #[doc(hidden)]
    fn restrict<'a>(
        &'a self,
        _cx: &'a Cx,
        _rows: &'a [M],
        _ex: &'a mut dyn toasty::Executor,
    ) -> crate::BoxFuture<'a, topcoat::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    /// The fields of the row [`text`](Self::text), [`cell`](Self::cell),
    /// [`entry`](Self::entry), [`search_expr`](Self::search_expr) and
    /// [`order_by`](Self::order_by) read, or `None` when the column does not say.
    ///
    /// A panel drops a column reading a field
    /// [`field_access`](crate::ResourceDef::field_access) hides from the user, and refuses to
    /// mount a panel restricting a field whose columns answer `None`. The built-in columns
    /// answer their own field.
    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        None
    }

    /// Whether the column shows a field of a related record that the related resource's
    /// [`field_access`](crate::ResourceDef::field_access) hides from `cx`: a panel drops it then,
    /// as it drops one reading a hidden field of the row.
    #[doc(hidden)]
    fn masked(&self, _cx: &Cx) -> bool {
        false
    }

    /// What the column reads of the related records it loads: see [`Related`]. A column answers
    /// [`Related::Row`] unless it says otherwise, since a closure reading its row may reach any
    /// relation the page loads.
    #[doc(hidden)]
    fn related(&self) -> Related {
        Related::Row
    }

    /// What is wrong with this column's declaration.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        None
    }

    /// The source the column labels its records by, when the context's panel cannot load from it.
    #[doc(hidden)]
    fn unavailable_source(&self, _cx: &Cx) -> Option<&'static str> {
        None
    }
}

/// The relations a [`Column`] reads off its row.
#[derive_where(Clone, Debug, Default)]
pub struct Includes<M>(
    Vec<crate::toasty_compat::UntypedInclude>,
    #[derive_where(skip(Debug))] std::marker::PhantomData<fn() -> M>,
);

impl<M> Includes<M> {
    /// No relation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `relation`.
    pub fn with<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        let include: crate::toasty_compat::UntypedInclude = relation.into().into();
        if !self.0.contains(&include) {
            self.0.push(include);
        }
        self
    }

    /// The relations, in the order they were added.
    pub(crate) fn into_vec(self) -> Vec<crate::toasty_compat::UntypedInclude> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One [`UnregisteredLabelSource`](DeclarationErrorKind::UnregisteredLabelSource) per column
/// labelling its records by a source `cx`'s panel cannot load from.
pub(crate) fn unavailable_sources<'c, M: 'c>(
    cx: &Cx,
    columns: impl IntoIterator<Item = &'c dyn Column<M>>,
) -> Vec<crate::DeclarationErrorKind> {
    columns
        .into_iter()
        .filter_map(|column| {
            column.unavailable_source(cx).map(|source| {
                crate::DeclarationErrorKind::UnregisteredLabelSource {
                    column: column.name().to_string(),
                    source,
                }
            })
        })
        .collect()
}

pub(crate) fn include_relations<'c, M>(
    mut query: Query<List<M>>,
    columns: impl IntoIterator<Item = &'c BoxColumn<M>>,
) -> Query<List<M>>
where
    M: toasty::schema::Model + 'c,
{
    let mut seen: Vec<crate::toasty_compat::UntypedInclude> = Vec::new();
    for include in columns.into_iter().flat_map(|c| c.includes().into_vec()) {
        if !seen.contains(&include) {
            query = query.include(include.clone());
            seen.push(include);
        }
    }
    query
}

/// Run each column's [`restrict`](Column::restrict) over `rows`, which a query through
/// [`include_relations`] loaded.
pub(crate) async fn restrict_relations<'c, M>(
    cx: &Cx,
    columns: impl IntoIterator<Item = &'c BoxColumn<M>>,
    rows: &[M],
    ex: &mut dyn toasty::Executor,
) -> topcoat::Result<()>
where
    M: 'c,
{
    for column in columns {
        column.restrict(cx, rows, &mut *ex).await?;
    }
    Ok(())
}

/// What a column over one field declares beside its lens: the field's binding, the declared label
/// and the width.
#[derive(Clone, Debug)]
struct ColumnBase {
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<crate::Text>,
    width: ColumnWidth,
    toggle: ColumnToggle,
}

impl ColumnBase {
    /// Bind `path`, with no declared label.
    fn new<M, T>(path: &Path<M, T>, width: ColumnWidth) -> Self
    where
        M: toasty::schema::Model,
    {
        Self {
            binding: Binding::of(path),
            label: None,
            width,
            toggle: ColumnToggle::Fixed,
        }
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> crate::Text {
        self.label.clone().unwrap_or_else(|| self.binding.label())
    }

    /// The field the column reads.
    fn reads<M>(&self) -> crate::lens::Reads<M> {
        self.binding.reads()
    }
}

/// How a [`Column`] takes the table's width.
///
/// The table lays out automatically: each column is as wide as its content, and the first column
/// that [`Grow`](Self::Grow)s takes the width left over. A table wider than its card scrolls
/// sideways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnWidth {
    /// As wide as its content, on one line: a status, date, number or boolean.
    #[default]
    Auto,
    /// As wide as its content up to 20rem, past which a value truncates; the first growing column
    /// also takes the table's spare width. The default for text and relation columns.
    Grow,
    /// Wrap the content onto several lines, between 12rem and 28rem wide.
    Wrap,
    /// Exactly this many rem; a value past that truncates.
    Rem(u8),
}

/// Whether each reader may hide a [`Column`] from their list, and whether it starts shown.
///
/// A list with a toggleable column renders a Columns menu in its toolbar, whose Apply keeps the
/// checked columns and Reset restores the defaults. A panel keeps each signed-in user's choice per
/// resource, in the [`ColumnPreference`](crate::auth::ColumnPreference) model, so the relation
/// tables over a resource show what its list shows; a column added later, or whose default changes,
/// takes its default until the reader picks otherwise. Search, sort and export still read every
/// column. A panel without auth, and a table a page renders with
/// [`wired_table`](crate::wired_table), render no menu and show the defaults.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, slug: String }
/// use tablo_core::{TextColumn, lens};
///
/// TextColumn::new(lens!(Post.title)).toggleable();
/// TextColumn::new(lens!(Post.slug)).hidden_by_default();
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnToggle {
    /// Always shown.
    #[default]
    Fixed,
    /// Shown until the reader hides it: `toggleable()`.
    ShownByDefault,
    /// Hidden until the reader shows it: `hidden_by_default()`.
    HiddenByDefault,
}

impl ColumnToggle {
    /// Whether the reader may hide or show the column.
    pub(crate) fn is_toggleable(self) -> bool {
        self != Self::Fixed
    }
}

impl ColumnWidth {
    /// The classes of the box a body cell wraps its content in, if any.
    pub(crate) fn content_class(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Grow => Some("max-w-xs truncate"),
            Self::Wrap => Some("min-w-48 max-w-md whitespace-normal"),
            Self::Rem(_) => Some("truncate"),
        }
    }

    /// The `style` of a cell of this width, header and body alike.
    pub(crate) fn style(self) -> Option<String> {
        match self {
            Self::Rem(rem) => Some(format!("width: {rem}rem; max-width: {rem}rem")),
            Self::Auto | Self::Grow | Self::Wrap => None,
        }
    }
}

/// A table's columns, as the table stores them.
pub(crate) type BoxColumn<M> = Arc<dyn Column<M>>;

/// Whether `column` shows to `cx`, which may not view the row's `hidden` fields: it reads none of
/// them, and no field of a related record its resource hides from `cx`.
pub(crate) fn shows<M>(cx: &Cx, column: &dyn Column<M>, hidden: &[String]) -> bool {
    crate::lens::shows(column.reads(), hidden) && !column.masked(cx)
}

/// Convert a single built-in column, a tuple of any [`Column`]s, a `Vec` or slice
/// of boxed columns, or `()` for none yet, into a table's column list.
///
/// [`Table::column`](crate::Table::column) appends past it.
pub trait IntoColumns<M> {
    #[doc(hidden)]
    fn into_columns(self) -> Vec<BoxColumn<M>>;
}

/// The [`IntoColumns`] impls of the built-in columns: each is one column.
macro_rules! column_conversions {
    ($($ty:ident<$($g:ident),+>),+ $(,)?) => {
        $(
            impl<$($g),+> IntoColumns<M> for $ty<$($g),+>
            where
                Self: Column<M> + 'static,
            {
                fn into_columns(self) -> Vec<BoxColumn<M>> {
                    vec![Arc::new(self)]
                }
            }
        )+
    };
}

column_conversions!(
    TextColumn<M, T>,
    DateColumn<M, T>,
    ComputedColumn<M>,
    RelationColumn<M>,
    CountColumn<M>,
    BooleanColumn<M>,
    FileColumn<M>,
    ColorColumn<M, T>,
    ImageColumn<M, T>,
    EmbeddedColumn<M, T>,
    RepeaterColumn<M, T>,
    RichTextColumn<M, T>,
);

impl<M> IntoColumns<M> for () {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        Vec::new()
    }
}

impl<M> IntoColumns<M> for Vec<BoxColumn<M>> {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        self
    }
}

impl<M> IntoColumns<M> for &[BoxColumn<M>] {
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        self.to_vec()
    }
}

/// The tuple impls of [`IntoColumns`], one arity per invocation.
macro_rules! into_columns_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<M, $($T),+> IntoColumns<M> for ($($T,)+)
        where
            $($T: Column<M> + 'static,)+
        {
            fn into_columns(self) -> Vec<BoxColumn<M>> {
                let ($($v,)+) = self;
                vec![$(Arc::new($v) as BoxColumn<M>,)+]
            }
        }
    };
}

into_columns_tuples!(A => a);
into_columns_tuples!(A => a, B => b);
into_columns_tuples!(A => a, B => b, C => c);
into_columns_tuples!(A => a, B => b, C => c, D => d);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_columns_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k
);
into_columns_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k, L => l
);

#[cfg(test)]
mod tests;
