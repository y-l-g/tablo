//! Table columns: the [`Column`] trait, the built-in [`TextColumn`] and
//! [`BooleanColumn`], and the [`IntoColumns`] seam.

use std::{borrow::Cow, sync::Arc};

use toasty::stmt::{Expr, OrderByExpr};
use topcoat::{context::Cx, icon::icon, view::*};

use crate::schema::{FieldLens, LensBinding};

/// One table column declares its header, its cell, and its query predicates.
///
/// ```ignore
/// struct Initials;
///
/// impl Column<User> for Initials {
///     fn name(&self) -> &str { "initials" }
///     fn label(&self) -> &str { "Initials" }
///     fn text(&self, row: &User) -> String {
///         row.name.split_whitespace().filter_map(|w| w.chars().next()).collect()
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

    /// The relations [`text`](Self::text) and [`cell`](Self::cell) read.
    fn includes(&self) -> Includes<M> {
        Includes::new()
    }

    /// What is wrong with this column's declaration.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<String> {
        None
    }
}

/// The relations a [`Column`] reads off its row.
pub struct Includes<M>(
    Vec<toasty_core::stmt::Include>,
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
        let include: toasty_core::stmt::Include = relation.into().into();
        if !self.0.contains(&include) {
            self.0.push(include);
        }
        self
    }

    /// The relations, in the order they were added.
    pub(crate) fn into_vec(self) -> Vec<toasty_core::stmt::Include> {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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

/// Text column bound to a typed lens and a typed projection (upstream gap #119).
#[derive(Clone)]
pub struct TextColumn<M> {
    /// The query-side lens; `None` for [`Self::computed`] columns, which
    /// render a value but declare no predicates.
    path: Option<FieldLens<M, String>>,
    name: String,
    label: String,
    project: Arc<dyn Fn(&M) -> String + Send + Sync>,
    searchable: bool,
    sortable: bool,
    /// The width this column claims in the table's fixed layout.
    width: ColumnWidth,
    /// Relations this column's projection reads.
    includes: Includes<M>,
    /// What is wrong with the declaration ([`Column::misdeclared`]).
    misdeclared: Option<String>,
}

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

impl<M> TextColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind a column to a `String` field lens plus a projection closure.
    pub fn r#for(
        path: FieldLens<M, String>,
        project: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        let binding = LensBinding::of(path.clone());
        Self {
            path: Some(path),
            name: binding.name,
            label: binding.label,
            project: Arc::new(project),
            searchable: false,
            sortable: false,
            width: ColumnWidth::Wide,
            includes: Includes::new(),
            misdeclared: binding.misdeclared,
        }
    }

    /// Declare a computed, display-only column.
    pub fn computed(
        label: impl Into<String>,
        project: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        let label = label.into();
        let name = label.to_lowercase();
        Self {
            path: None,
            name,
            label,
            project: Arc::new(project),
            searchable: false,
            sortable: false,
            width: ColumnWidth::Narrow,
            includes: Includes::new(),
            misdeclared: None,
        }
    }

    /// Declare a relation this column's projection reads.
    pub fn include<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        self.includes = self.includes.with(relation);
        self
    }

    pub fn searchable(mut self) -> Self {
        self.refuse_computed("searchable");
        self.searchable = true;
        self
    }

    pub fn sortable(mut self) -> Self {
        self.refuse_computed("sortable");
        self.sortable = true;
        self
    }

    fn refuse_computed(&mut self, modifier: &str) {
        if self.path.is_none() && self.misdeclared.is_none() {
            self.misdeclared = Some(format!(
                "{modifier}() on computed column '{}': computed columns map to no query predicate",
                self.label
            ));
        }
    }

    /// Declare this column's width.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }
}

impl<M> Column<M> for TextColumn<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    /// The typed projection's output.
    fn text(&self, row: &M) -> String {
        (self.project)(row)
    }

    fn column_width(&self) -> ColumnWidth {
        self.width
    }

    fn is_searchable(&self) -> bool {
        self.searchable
    }

    /// A portable, escaped substring match.
    fn search_expr(&self, term: &str) -> Option<Expr<bool>> {
        let t = term.trim();
        if !self.searchable || t.is_empty() {
            return None;
        }
        Some(
            self.path
                .clone()?
                .like_with_escape(escape_like_pattern(t), LIKE_ESCAPE),
        )
    }

    fn is_sortable(&self) -> bool {
        self.sortable
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        if self.sortable {
            let path = self.path.clone()?;
            Some(if descending { path.desc() } else { path.asc() })
        } else {
            None
        }
    }

    fn includes(&self) -> Includes<M> {
        self.includes.clone()
    }

    fn misdeclared(&self) -> Option<String> {
        self.misdeclared.clone()
    }
}

impl<M> std::fmt::Debug for TextColumn<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextColumn")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("searchable", &self.searchable)
            .field("sortable", &self.sortable)
            .field("width", &self.width)
            .field("includes", &self.includes.len())
            .finish_non_exhaustive()
    }
}

/// A column of a `bool` field rendered as an icon.
///
/// ```ignore
/// BooleanColumn::r#for(Post::fields().featured(), |p: &Post| p.featured).sortable()
/// ```
pub struct BooleanColumn<M> {
    path: FieldLens<M, bool>,
    name: String,
    label: String,
    project: Arc<dyn Fn(&M) -> bool + Send + Sync>,
    sortable: bool,
    labels: (String, String),
    misdeclared: Option<String>,
}

impl<M> BooleanColumn<M>
where
    M: toasty::schema::Model,
{
    /// Bind the column to a `bool` field lens and a projection (upstream gap #119).
    pub fn r#for(
        path: FieldLens<M, bool>,
        project: impl Fn(&M) -> bool + Send + Sync + 'static,
    ) -> Self {
        let binding = LensBinding::of(path.clone());
        Self {
            path,
            name: binding.name,
            label: binding.label,
            project: Arc::new(project),
            sortable: false,
            labels: ("Yes".to_string(), "No".to_string()),
            misdeclared: binding.misdeclared,
        }
    }

    /// Make the header a sort link.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
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
        &self.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn text(&self, row: &M) -> String {
        if (self.project)(row) {
            self.labels.0.clone()
        } else {
            self.labels.1.clone()
        }
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = (self.project)(row);
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
            let path = self.path.clone();
            if descending { path.desc() } else { path.asc() }
        })
    }
    fn misdeclared(&self) -> Option<String> {
        self.misdeclared.clone()
    }
}

impl<M> std::fmt::Debug for BooleanColumn<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BooleanColumn")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("sortable", &self.sortable)
            .finish_non_exhaustive()
    }
}

/// A table's columns, as the table stores them.
pub(crate) type BoxColumn<M> = Arc<dyn Column<M>>;

/// Convert a single built-in column, or a tuple of any [`Column`]s, into a table's column list.
///
/// [`Table::column`](super::Table::column) appends past it.
pub trait IntoColumns<M> {
    #[doc(hidden)]
    fn into_columns(self) -> Vec<BoxColumn<M>>;
}

impl<M> IntoColumns<M> for TextColumn<M>
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
