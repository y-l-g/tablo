//! Table and detail columns: the [`Column`] trait, the built-in [`TextColumn`],
//! [`ComputedColumn`], [`RelationColumn`], [`CountColumn`], [`BooleanColumn`], [`FileColumn`] and
//! [`EmbeddedColumn`], and the [`IntoColumns`] seam.

use std::{borrow::Cow, sync::Arc};

use derive_where::derive_where;
use toasty::stmt::{Expr, List, OrderByExpr, Path, Query};
use topcoat::{context::Cx, icon::icon, view::*};

use crate::{
    Detail, IntoDetail, Lens,
    form::FormScalar,
    schema::{Binding, FieldResolver},
};

/// The `label` and `width` builders of a column over a [`ColumnBase`], inside its inherent impl.
macro_rules! base_builders {
    () => {
        /// Replace the label the field's name gives it.
        pub fn label(mut self, label: impl Into<String>) -> Self {
            self.base.label = Some(label.into());
            self
        }

        /// Declare this column's width.
        pub fn width(mut self, width: $crate::table::ColumnWidth) -> Self {
            self.base.width = width;
            self
        }
    };
}

/// The [`Column`] methods a column over a [`ColumnBase`] answers from it, inside its `Column`
/// impl; `bind` adds the binding's `misdeclared` and `bind`.
macro_rules! base_column_methods {
    () => {
        fn name(&self) -> &str {
            self.base.name()
        }

        fn label(&self) -> &str {
            self.base.label()
        }

        fn column_width(&self) -> $crate::table::ColumnWidth {
            self.base.width
        }
    };
    (bind) => {
        base_column_methods!();

        fn misdeclared(&self) -> Option<$crate::DeclarationErrorKind> {
            self.base.binding.misdeclared()
        }

        fn bind(&self, resolver: &$crate::schema::FieldResolver) {
            self.base.binding.bind(resolver);
        }
    };
}

mod embedded;
mod relation;
mod repeater;

pub use embedded::EmbeddedColumn;
pub use relation::{CountColumn, RelationColumn, RelationLens, ToOneRelation};
pub use repeater::RepeaterColumn;

/// One column of a table or a [`Detail`](crate::Detail) declares its label, its value read off
/// the record, and its query predicates.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # use tablo_core::extend::Column;
/// # use topcoat::context::Cx;
/// struct Initials;
///
/// impl Column<User> for Initials {
///     fn name(&self) -> &str {
///         "initials"
///     }
///     fn label(&self) -> &str {
///         "Initials"
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
    fn label(&self) -> &str;

    /// The row's value as plain text.
    fn text(&self, cx: &Cx, row: &M) -> String;

    /// The row's table cell.
    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        crate::schema::value_cell(cx, &self.text(cx, row))
    }

    /// The record's entry on a detail page: the [`label`](Self::label) over the
    /// [`cell`](Self::cell).
    fn entry<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        crate::schema::read_only(cx, self.label(), self.cell(cx, row))
    }

    /// The width the column claims.
    fn column_width(&self) -> ColumnWidth {
        ColumnWidth::Narrow
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

    /// Bind an embedded path through `resolver`'s app schema.
    #[doc(hidden)]
    fn bind(&self, _resolver: &FieldResolver) {}
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

/// Include in `query` every relation `columns` declare, once each.
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

/// What a column over one field declares beside its lens: the field's binding, the declared label
/// and the width.
#[derive(Clone, Debug)]
struct ColumnBase {
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    width: ColumnWidth,
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
        }
    }

    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }
}

/// The share of the table a [`ColumnWidth::Narrow`] column claims, in whole
/// percent.
pub(crate) const NARROW_DEFAULT_PERCENT: u8 = 10;

/// The width a [`Column`] claims in the table's fixed layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnWidth {
    /// Take a share of whatever the declared columns leave.
    #[default]
    Wide,
    /// A share of the table for a status, boolean, date or count cell.
    Narrow,
    /// An explicit length in whole rem.
    Rem(u8),
    /// An explicit share of the table in whole percent.
    Percent(u8),
}

impl ColumnWidth {
    /// The share of the table this column claims as a kind default, in whole percent.
    pub(crate) fn default_percent(self) -> Option<u8> {
        match self {
            Self::Narrow => Some(NARROW_DEFAULT_PERCENT),
            Self::Wide | Self::Rem(_) | Self::Percent(_) => None,
        }
    }

    /// The `style` attribute value an explicit declaration emits.
    pub(crate) fn explicit_css(self) -> Option<Cow<'static, str>> {
        match self {
            Self::Rem(rem) => Some(Cow::Owned(format!("width: {rem}rem"))),
            Self::Percent(percent) => Some(Cow::Owned(format!("width: {percent}%"))),
            Self::Wide | Self::Narrow => None,
        }
    }
}

/// A column of one field, rendered as text and bound through a [`Lens`] for sorting and search.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     email: String,
/// #     age: i64,
/// # }
/// tablo_core::TextColumn::new(tablo_core::lens!(User.name))
///     .searchable()
///     .sortable();
/// tablo_core::TextColumn::new(tablo_core::lens!(User.age))
///     .sortable()
///     .format(|age| format!("{age} years"));
/// ```
///
/// The cell is the value's [`FormScalar::to_label`], an [`Options`](crate::Options) enum's label
/// or any other type's form spelling, unless [`format`](Self::format) says otherwise. Only a
/// string field is [`searchable`](Self::searchable):
///
/// ```compile_fail
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, age: i64 }
/// # fn main() {
/// tablo_core::TextColumn::new(tablo_core::lens!(User.age)).searchable();
/// # }
/// ```
#[derive_where(Clone, Debug)]
pub struct TextColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    #[derive_where(skip(Debug))]
    format: Arc<dyn Fn(&T) -> String + Send + Sync>,
    /// The `LIKE` predicate for a search pattern, when [`searchable`](Self::searchable).
    #[derive_where(skip(Debug))]
    search: Option<SearchFn>,
    sortable: bool,
}

/// A searchable column's predicate for an escaped `LIKE` pattern.
type SearchFn = Arc<dyn Fn(String) -> Expr<bool> + Send + Sync>;

/// The escape character the search pattern declares to `LIKE`.
pub(crate) const LIKE_ESCAPE: char = '\\';

/// Wrap `term` as a `LIKE` pattern matching it anywhere in the column.
pub(crate) fn escape_like_pattern(term: &str) -> String {
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for c in term.chars() {
        if c == LIKE_ESCAPE || c == '%' || c == '_' {
            pattern.push(LIKE_ESCAPE);
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Escaped substring predicate for `lens` matching `term`.
pub fn contains_expr<M, T>(lens: &Lens<M, T>, term: &str) -> Option<Expr<bool>>
where
    M: toasty::schema::Model,
    T: toasty::schema::Field<Inner = String>,
{
    let trimmed = term.trim();
    (!trimmed.is_empty()).then(|| {
        lens.path()
            .clone()
            .like_with_escape(escape_like_pattern(trimmed), LIKE_ESCAPE)
    })
}

impl<M, T> TextColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind a column to the field `lens` reads, which must be one field of the model.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: FormScalar + Send + Sync + 'static,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Wide),
            lens,
            format: Arc::new(T::to_label),
            search: None,
            sortable: false,
        }
    }

    /// Render the field's value through `format`.
    pub fn format(mut self, format: impl Fn(&T) -> String + Send + Sync + 'static) -> Self {
        self.format = Arc::new(format);
        self
    }

    /// Make the header a sort link.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    base_builders!();
}

impl<M, T> TextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: toasty::schema::Field<Inner = String> + Send + Sync + 'static,
{
    /// Join the table's search with a substring match on this string field.
    pub fn searchable(mut self) -> Self {
        let path = self.lens.path().clone();
        self.search = Some(Arc::new(move |pattern| {
            path.clone().like_with_escape(pattern, LIKE_ESCAPE)
        }));
        self
    }
}

impl<M, T> Column<M> for TextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        (self.format)(self.lens.read(row))
    }

    fn is_searchable(&self) -> bool {
        self.search.is_some()
    }

    /// A portable, escaped substring match.
    fn search_expr(&self, term: &str) -> Option<Expr<bool>> {
        let t = term.trim();
        let search = self.search.as_ref()?;
        (!t.is_empty()).then(|| search(escape_like_pattern(t)))
    }

    fn is_sortable(&self) -> bool {
        self.sortable
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable.then(|| {
            let path = self.lens.path().clone();
            if descending { path.desc() } else { path.asc() }
        })
    }
}

/// A display-only column rendering any text from the row, with no query predicate.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, body: String }
/// tablo_core::ComputedColumn::new("Words", |p: &Post| {
///     p.body.split_whitespace().count().to_string()
/// });
/// ```
///
/// A closure that reads a relation declares it with [`include`](Self::include), so the page
/// loads it; one relation's record or count is a [`RelationColumn`] or a [`CountColumn`], which
/// declare their own.
///
/// It maps to no column, so it neither searches nor sorts:
///
/// ```compile_fail
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # fn main() {
/// tablo_core::ComputedColumn::new("Name", |u: &User| u.name.clone()).sortable();
/// # }
/// ```
#[derive_where(Clone, Debug)]
pub struct ComputedColumn<M> {
    name: String,
    label: String,
    #[derive_where(skip(Debug))]
    project: Arc<dyn Fn(&M) -> String + Send + Sync>,
    width: ColumnWidth,
    /// Relations the projection reads.
    includes: Includes<M>,
}

impl<M> ComputedColumn<M>
where
    M: toasty::schema::Model,
{
    /// Declare a column headed `label` rendering `project(row)`.
    pub fn new(
        label: impl Into<String>,
        project: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        let label = label.into();
        Self {
            name: label.to_lowercase(),
            label,
            project: Arc::new(project),
            width: ColumnWidth::Narrow,
            includes: Includes::new(),
        }
    }

    /// Declare a relation the projection reads, so the table loads it.
    pub fn include<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        self.includes = self.includes.with(relation);
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }
}

impl<M> Column<M> for ComputedColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn text(&self, _cx: &Cx, row: &M) -> String {
        (self.project)(row)
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn includes(&self) -> Includes<M> {
        self.includes.clone()
    }
}

/// A column of a `bool` field rendered as an icon.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// tablo_core::BooleanColumn::new(tablo_core::lens!(Post.featured)).sortable();
/// ```
#[derive_where(Clone, Debug)]
pub struct BooleanColumn<M> {
    lens: Lens<M, bool>,
    base: ColumnBase,
    sortable: bool,
    labels: (String, String),
}

impl<M> BooleanColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `bool` field `lens` reads.
    pub fn new(lens: Lens<M, bool>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Narrow),
            lens,
            sortable: false,
            labels: ("Yes".to_string(), "No".to_string()),
        }
    }

    /// Make the header a sort link.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    base_builders!();

    /// The words for `true` and `false`.
    pub fn labels(mut self, yes: impl Into<String>, no: impl Into<String>) -> Self {
        self.labels = (yes.into(), no.into());
        self
    }
}

impl<M> Column<M> for BooleanColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        if *self.lens.read(row) {
            self.labels.0.clone()
        } else {
            self.labels.1.clone()
        }
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = *self.lens.read(row);
        let text = self.text(cx, row);
        let (data, class) = if value {
            (tablo_ui::icons::CIRCLE_CHECK, "size-4 text-primary")
        } else {
            (tablo_ui::icons::X, "size-4 text-muted-foreground")
        };
        view! {
            cx =>
            <span class="inline-flex items-center" data-boolean=(value.to_string())>
                icon(
                    data: data,
                    attrs: attributes! { class=(class) aria-hidden="true" }
                )
                <span class="sr-only">(text)</span>
            </span>
        }
        .boxed()
    }

    fn is_sortable(&self) -> bool {
        self.sortable
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable.then(|| {
            let path = self.lens.path().clone();
            if descending { path.desc() } else { path.asc() }
        })
    }
}

/// A column of a `String` field holding an uploaded file's path, rendered as a link to the file.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Doc { #[key] #[auto] id: uuid::Uuid, path: String }
/// tablo_core::FileColumn::new(tablo_core::lens!(Doc.path));
/// ```
///
/// The cell links a rooted path or an absolute `http(s)` URL, the same rule a
/// [`Field::file`](crate::Field::file) control applies, and shows any other value as text.
#[derive_where(Clone, Debug)]
pub struct FileColumn<M> {
    lens: Lens<M, String>,
    base: ColumnBase,
}

impl<M> FileColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` field `lens` reads.
    pub fn new(lens: Lens<M, String>) -> Self {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Wide),
            lens,
        }
    }

    base_builders!();
}

impl<M> Column<M> for FileColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).clone()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let path = self.lens.read(row);
        if path.trim().is_empty() {
            return crate::schema::value_cell(cx, path);
        }
        crate::schema::stored_upload(cx, path)
    }
}

/// A table's columns, as the table stores them.
pub(crate) type BoxColumn<M> = Arc<dyn Column<M>>;

/// Convert a single built-in column, a tuple of any [`Column`]s, a `Vec` or slice
/// of boxed columns, or `()` for none yet, into a table's column list.
///
/// [`Table::column`](super::Table::column) appends past it.
pub trait IntoColumns<M> {
    #[doc(hidden)]
    fn into_columns(self) -> Vec<BoxColumn<M>>;
}

/// The [`IntoColumns`] and [`IntoDetail`] impls of the built-in columns: each is one column.
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

            impl<$($g),+> IntoDetail<M> for $ty<$($g),+>
            where
                Self: Column<M> + 'static,
            {
                fn into_detail(self) -> Detail<M> {
                    Detail::empty().column(self)
                }
            }
        )+
    };
}

column_conversions!(
    TextColumn<M, T>,
    ComputedColumn<M>,
    RelationColumn<M>,
    CountColumn<M>,
    BooleanColumn<M>,
    FileColumn<M>,
    EmbeddedColumn<M, T>,
    RepeaterColumn<M, T>,
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
