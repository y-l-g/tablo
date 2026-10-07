//! Table and detail columns: the [`Column`] trait, the built-in [`TextColumn`],
//! [`ComputedColumn`], [`BooleanColumn`], [`FileColumn`] and [`EmbeddedColumn`], and the
//! [`IntoColumns`] seam.

use std::{borrow::Cow, sync::Arc};

use toasty::stmt::{Expr, List, OrderByExpr, Query};
use topcoat::{context::Cx, icon::icon, view::*};

use crate::{
    Lens,
    form::FormScalar,
    schema::{Binding, FieldResolver},
};

mod embedded;

pub use embedded::EmbeddedColumn;

/// One column of a table or a [`Detail`](crate::Detail) declares its label, its value read off
/// the record, and its query predicates.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # use tablo_core::Column;
/// struct Initials;
///
/// impl Column<User> for Initials {
///     fn name(&self) -> &str {
///         "initials"
///     }
///     fn label(&self) -> &str {
///         "Initials"
///     }
///     fn text(&self, row: &User) -> String {
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
    fn text(&self, row: &M) -> String;

    /// The row's table cell.
    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let text = self.text(row);
        view! { cx => (text) }.boxed()
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

    /// Bind an embedded path through `resolver`'s app schema.
    #[doc(hidden)]
    fn bind(&self, _resolver: &FieldResolver) {}
}

/// The relations a [`Column`] reads off its row.
pub struct Includes<M>(
    Vec<crate::toasty_compat::UntypedInclude>,
    std::marker::PhantomData<fn() -> M>,
);

impl<M> Default for Includes<M> {
    fn default() -> Self {
        Self(Vec::new(), std::marker::PhantomData)
    }
}

impl<M> Clone for Includes<M> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), std::marker::PhantomData)
    }
}

impl<M> std::fmt::Debug for Includes<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Includes").field(&self.0).finish()
    }
}

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
pub struct TextColumn<M, T> {
    lens: Lens<M, T>,
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    format: Arc<dyn Fn(&T) -> String + Send + Sync>,
    /// The `LIKE` predicate for a search pattern, when [`searchable`](Self::searchable).
    search: Option<SearchFn>,
    sortable: bool,
    /// The width this column claims in the table's fixed layout.
    width: ColumnWidth,
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
        let binding = Binding::of(&lens.path().clone());
        Self {
            lens,
            binding,
            label: None,
            format: Arc::new(T::to_label),
            search: None,
            sortable: false,
            width: ColumnWidth::Wide,
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

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }
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
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn text(&self, row: &M) -> String {
        (self.format)(self.lens.read(row))
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
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

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }
}

impl<M, T> Clone for TextColumn<M, T> {
    fn clone(&self) -> Self {
        Self {
            lens: self.lens.clone(),
            binding: self.binding.clone(),
            label: self.label.clone(),
            format: Arc::clone(&self.format),
            search: self.search.clone(),
            sortable: self.sortable,
            width: self.width,
        }
    }
}

impl<M, T> std::fmt::Debug for TextColumn<M, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextColumn")
            .field("name", &self.binding.name())
            .field(
                "label",
                &self.label.as_deref().unwrap_or(self.binding.label()),
            )
            .field("searchable", &self.search.is_some())
            .field("sortable", &self.sortable)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

/// A display-only column rendering any text from the row, with no query predicate.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     author_id: uuid::Uuid,
/// #     #[belongs_to(key = author_id, references = id)]
/// #     author: toasty::Deferred<Author>,
/// # }
/// tablo_core::ComputedColumn::new("Author", |p: &Post| p.author.get().name.clone())
///     .include(Post::fields().author());
/// ```
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
pub struct ComputedColumn<M> {
    name: String,
    label: String,
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

    fn text(&self, row: &M) -> String {
        (self.project)(row)
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn includes(&self) -> Includes<M> {
        self.includes.clone()
    }
}

impl<M> Clone for ComputedColumn<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            label: self.label.clone(),
            project: Arc::clone(&self.project),
            width: self.width,
            includes: self.includes.clone(),
        }
    }
}

impl<M> std::fmt::Debug for ComputedColumn<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComputedColumn")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("width", &self.width)
            .field("includes", &self.includes.len())
            .finish_non_exhaustive()
    }
}

/// A column of a `bool` field rendered as an icon.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, featured: bool }
/// tablo_core::BooleanColumn::new(tablo_core::lens!(Post.featured)).sortable();
/// ```
pub struct BooleanColumn<M> {
    lens: Lens<M, bool>,
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    sortable: bool,
    labels: (String, String),
    width: ColumnWidth,
}

impl<M> BooleanColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `bool` field `lens` reads.
    pub fn new(lens: Lens<M, bool>) -> Self {
        let binding = Binding::of(&lens.path().clone());
        Self {
            lens,
            binding,
            label: None,
            sortable: false,
            labels: ("Yes".to_string(), "No".to_string()),
            width: ColumnWidth::Narrow,
        }
    }

    /// Make the header a sort link.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

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
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn text(&self, row: &M) -> String {
        if *self.lens.read(row) {
            self.labels.0.clone()
        } else {
            self.labels.1.clone()
        }
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = *self.lens.read(row);
        let text = self.text(row);
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

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }
}

impl<M> Clone for BooleanColumn<M> {
    fn clone(&self) -> Self {
        Self {
            lens: self.lens.clone(),
            binding: self.binding.clone(),
            label: self.label.clone(),
            sortable: self.sortable,
            labels: self.labels.clone(),
            width: self.width,
        }
    }
}

impl<M> std::fmt::Debug for BooleanColumn<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BooleanColumn")
            .field("name", &self.binding.name())
            .field(
                "label",
                &self.label.as_deref().unwrap_or(self.binding.label()),
            )
            .field("sortable", &self.sortable)
            .field("width", &self.width)
            .finish_non_exhaustive()
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
pub struct FileColumn<M> {
    lens: Lens<M, String>,
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    width: ColumnWidth,
}

impl<M> FileColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` field `lens` reads.
    pub fn new(lens: Lens<M, String>) -> Self {
        let binding = Binding::of(&lens.path().clone());
        Self {
            lens,
            binding,
            label: None,
            width: ColumnWidth::Wide,
        }
    }

    /// Replace the label the field's name gives it.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }
}

impl<M> Column<M> for FileColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        self.binding.name()
    }

    fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    fn text(&self, row: &M) -> String {
        self.lens.read(row).clone()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        crate::schema::stored_upload(cx, self.lens.read(row))
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }
}

impl<M> Clone for FileColumn<M> {
    fn clone(&self) -> Self {
        Self {
            lens: self.lens.clone(),
            binding: self.binding.clone(),
            label: self.label.clone(),
            width: self.width,
        }
    }
}

impl<M> std::fmt::Debug for FileColumn<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileColumn")
            .field("name", &self.binding.name())
            .field(
                "label",
                &self.label.as_deref().unwrap_or(self.binding.label()),
            )
            .field("width", &self.width)
            .finish_non_exhaustive()
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

impl<M, T> IntoColumns<M> for TextColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        vec![Arc::new(self)]
    }
}

impl<M> IntoColumns<M> for ComputedColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        vec![Arc::new(self)]
    }
}

impl<M> IntoColumns<M> for BooleanColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        vec![Arc::new(self)]
    }
}

impl<M> IntoColumns<M> for FileColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        vec![Arc::new(self)]
    }
}

impl<M, T> IntoColumns<M> for EmbeddedColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: crate::EmbeddedForm + Send + Sync + 'static,
{
    fn into_columns(self) -> Vec<BoxColumn<M>> {
        vec![Arc::new(self)]
    }
}

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

#[cfg(test)]
mod tests;
