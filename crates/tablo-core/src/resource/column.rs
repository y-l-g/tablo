//! Table columns: the [`Column`] trait, the built-in [`TextColumn`] and
//! [`BooleanColumn`], and the [`IntoColumns`] seam.

use std::{borrow::Cow, sync::Arc};

use toasty::stmt::{Expr, OrderByExpr};
use topcoat::{context::Cx, icon::icon, view::*};

use crate::schema::{FieldLens, LensBinding};

/// One table column: what its header says, what each row's cell shows, and
/// which query predicates it contributes.
///
/// The built-in [`TextColumn`] and [`BooleanColumn`] implement this trait
/// and nothing more, so an app column has the same reach: implement it, and
/// pass the value to [`Table::new`](super::Table::new) in a tuple or to
/// [`Table::column`](super::Table::column).
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
/// [`text`](Self::text) are required. The cell defaults to the text, and a
/// column contributes no search, sort or relation until it says so.
pub trait Column<M>: Send + Sync {
    /// The column's identifier, distinct within its table: the `?sort=`
    /// value that names it.
    fn name(&self) -> &str;

    /// The header text, which the CSV export also writes.
    fn label(&self) -> &str;

    /// The row's value as plain text: the CSV export's cell, and the table
    /// cell unless [`cell`](Self::cell) renders something else.
    fn text(&self, row: &M) -> String;

    /// The row's table cell. Defaults to [`text`](Self::text).
    ///
    /// The cell sits in a `td` that truncates, so a view wider than its
    /// column clips to an ellipsis.
    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let text = self.text(row);
        view! { cx => (text) }.boxed()
    }

    /// The width the column claims in the table's fixed layout. Defaults to
    /// [`ColumnWidth::Narrow`].
    fn column_width(&self) -> ColumnWidth {
        ColumnWidth::Narrow
    }

    /// Whether the column joins the table's search: the search toolbar
    /// renders when any column does. Defaults to `false`.
    fn is_searchable(&self) -> bool {
        false
    }

    /// The predicate a search for `term` adds, OR-ed with the other
    /// searchable columns'. `term` is trimmed and not empty. Defaults to
    /// none.
    fn search_expr(&self, _term: &str) -> Option<Expr<bool>> {
        None
    }

    /// Whether the header links to a sort on this column. Defaults to
    /// `false`.
    fn is_sortable(&self) -> bool {
        false
    }

    /// The ordering a sort on this column applies. Defaults to none.
    fn order_by(&self, _descending: bool) -> Option<OrderByExpr> {
        None
    }

    /// The relations [`text`](Self::text) and [`cell`](Self::cell) read,
    /// which the list and the export load. Defaults to none.
    fn includes(&self) -> Includes<M> {
        Includes::new()
    }

    /// What is wrong with this column's declaration, which
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) reports. The built-in
    /// columns record a lens that binds no single field here, and a search or sort
    /// asked of a computed column.
    #[doc(hidden)]
    fn misdeclared(&self) -> Option<String> {
        None
    }
}

/// The relations a [`Column`] reads off its row, which the list and the
/// export load before rendering it.
///
/// Built with [`Includes::with`], typed on the table's model `M`, so a
/// relation the model does not have is a compile error.
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

    /// Add `relation`: `Includes::new().with(Post::fields().author())`. A
    /// relation already present is not added twice.
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

    /// How many relations there are.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The share of the table a [`ColumnWidth::Narrow`] column claims, in whole
/// percent.
pub(crate) const NARROW_DEFAULT_PERCENT: u8 = 10;

/// The width a [`Column`] claims in the table's fixed layout.
///
/// Widths are **shares of the table**, so what a table declares is a fraction
/// of its container rather than a length that can outgrow it: the columns that
/// declare none take what the declared ones leave. A length
/// ([`Rem`](Self::Rem)) is the exception — lengths do not shrink with the
/// table, and a table whose lengths exceed its width gives the columns that
/// declare none no space at all, header text included.
///
/// The renderer writes the width into the column's `th` and every row's `td`
/// as an inline `style` attribute — data, never a generated Tailwind class.
/// Tailwind generates only the class literals it finds in source, so a width
/// assembled at render (`w-[{n}%]`) would emit no CSS at all (ADR-0006); a
/// declared width is read by the layout directly.
///
/// A column's **kind** picks the default: `TextColumn::r#for` binds a `String`
/// field, so its cells hold the row's own text — a title, a name, a body — and
/// it defaults to [`Wide`](Self::Wide), taking a share of what the declared
/// columns leave; [`TextColumn::computed`] derives its cell (a status, a
/// boolean, a date, a count) and defaults to [`Narrow`](Self::Narrow), a share
/// of the table, as does a [`BooleanColumn`] and any [`Column`] that does not
/// override [`Column::column_width`]. [`TextColumn::width`] overrides either,
/// which is the seam for a column whose content disagrees with its kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnWidth {
    /// Take a share of whatever the declared columns leave: the column
    /// declares no width, and `table-fixed` splits the remainder between the
    /// wide columns instead of measuring the rows currently rendered.
    #[default]
    Wide,
    /// A share of the table for a status, boolean, date or count cell: the
    /// default for a [`TextColumn::computed`] column. The renderer resolves
    /// the share (10% nominally) against the table's other kind defaults.
    Narrow,
    /// An explicit length in whole rem: `Rem(14)` declares `14rem`. A length
    /// does not shrink with the table, so a table narrower than the lengths it
    /// declares gives the columns that declare none no space at all.
    Rem(u8),
    /// An explicit share of the table in whole percent: `Percent(30)`
    /// declares `30%`.
    Percent(u8),
}

impl ColumnWidth {
    /// The share of the table this column claims as a **kind default**, in
    /// whole percent, or `None` for a column that declares an explicit width
    /// or none at all.
    ///
    /// A nominal: the renderer scales the kind defaults down together when
    /// their total would leave the wide columns less than their share of the
    /// table.
    pub(crate) fn default_percent(self) -> Option<u8> {
        match self {
            Self::Narrow => Some(NARROW_DEFAULT_PERCENT),
            Self::Wide | Self::Rem(_) | Self::Percent(_) => None,
        }
    }

    /// The `style` attribute value an **explicit** declaration emits, or
    /// `None` for [`Wide`](Self::Wide), which declares nothing, and for
    /// [`Narrow`](Self::Narrow), whose share the renderer resolves against the
    /// rest of the table.
    pub(crate) fn explicit_css(self) -> Option<Cow<'static, str>> {
        match self {
            Self::Rem(rem) => Some(Cow::Owned(format!("width: {rem}rem"))),
            Self::Percent(percent) => Some(Cow::Owned(format!("width: {percent}%"))),
            Self::Wide | Self::Narrow => None,
        }
    }
}

/// Text column bound to a typed lens **and** a typed projection.
///
/// The lens (`FieldLens<M, String>`) is the query side: it names the column
/// and produces search/sort predicates — `TextColumn::for(User::fields().name(), ..)`
/// fails to compile if the field does not exist (ADR-0001).
///
/// The projection closure is the render side: it reads the value off a model
/// instance for the cell (`|u| u.name.clone()`). Toasty models are plain
/// structs and expose no instance→field reflection, so the closure is the
/// only way to read a field generically (upstream gap #119: instance →
/// field-value extraction). A typo in the closure body fails at compile
/// time — there is no string dispatch and no panic at render.
///
/// A projection that reads a **relation** declares it with [`Self::include`],
/// because the closure is opaque to the framework: the list and the export
/// load every relation the table's columns declared, and nothing else.
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

/// The escape character the search pattern declares to `LIKE`:
/// backslash, escaped in the pattern by [`escape_like_pattern`].
pub(crate) const LIKE_ESCAPE: char = '\\';

/// Wrap `term` as a `LIKE` pattern matching it anywhere in the column, with
/// `%`, `_` and the escape character itself escaped so the term stays literal.
///
/// Toasty ships the SQL half (`like_with_escape`) but not this one: escaping is
/// app-side because only the app knows whether it is building a literal or a
/// pattern.
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
    /// Bind a column to a `String` field lens plus a projection closure:
    /// `TextColumn::for(User::fields().name(), |u| u.name.clone())`.
    ///
    /// The closure receives each rendered row and returns the cell text, so
    /// computed cells (`|u| u.active.then(|| "Active".into()).unwrap_or_default()`)
    /// are as natural as plain field reads.
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

    /// A computed, display-only column (CONTEXT.md Column: "a computed value").
    ///
    /// No field lens — so it cannot be searchable or sortable (it maps to no
    /// query predicate) — but any cell projection compiles: booleans,
    /// timestamps, joined values. Calling `.searchable()` / `.sortable()` on
    /// a computed column is a misdeclaration
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses: a lying sort
    /// link or search promise is worse than a loud build error.
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

    /// Declare a relation this column's projection reads:
    /// `.include(Post::fields().author())` for `|p| p.author.get().name.clone()`.
    /// The path is typed on the table's model, so a relation the model does not
    /// have is a compile error.
    ///
    /// The list and the export load every relation the table's columns declared,
    /// so **declare every relation the closure reads**. One it does not declare
    /// arrives unloaded, and the closure's `is_unloaded` guard (the
    /// unloaded-relation contract of ADR-0011, `"(unloaded)"` plus a
    /// `debug_assert!`) turns that into a loud failure instead of a silent
    /// `"-"`.
    ///
    /// Repeat calls accumulate:
    /// `.include(Post::fields().author()).include(Post::fields().comments())`.
    ///
    /// One query loads a relation once for the whole table: two includes of the
    /// same relation merge, and an unfiltered one wins over a filtered one
    /// (Toasty ORs their filters). A column that counts a filtered subset should
    /// filter in its closure rather than rely on a filtered include.
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

    /// Record `modifier` on a computed column as a misdeclaration: it maps to
    /// no query predicate.
    fn refuse_computed(&mut self, modifier: &str) {
        if self.path.is_none() && self.misdeclared.is_none() {
            self.misdeclared = Some(format!(
                "{modifier}() on computed column '{}': computed columns map to no query predicate",
                self.label
            ));
        }
    }

    /// Declare this column's width in the table's fixed layout:
    /// `.width(ColumnWidth::Percent(20))` for a column that knows its own
    /// measure.
    ///
    /// The default follows the column's kind — see [`ColumnWidth`]. Override
    /// it when the content disagrees with the kind: a `computed` column that
    /// holds a name or a title is [`Wide`](ColumnWidth::Wide), a `String` field
    /// that holds a status is [`Narrow`](ColumnWidth::Narrow).
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

    /// A portable, escaped **substring** match.
    ///
    /// `like_with_escape` keeps the pattern parameterised and lowers to the
    /// same `LIKE … ESCAPE '\\'` on every driver, and
    /// `escape_like_pattern` makes the term literal — a `%` or `_` the user
    /// typed matches that character, it does not act as a wildcard. Note the
    /// driver difference `LIKE` brings: SQLite compares ASCII
    /// case-insensitively, PostgreSQL case-sensitively.
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
            // Cursor determinism is the engine's job: toasty's
            // `normalize_cursor_order` appends the physical PK columns to
            // ambiguous cursor orderings internally.
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

/// A column of a `bool` field, rendered as an icon: a check for `true`, a
/// cross for `false`.
///
/// Built on the public [`Column`] trait alone. The icon carries a
/// screen-reader label, and the CSV export writes the same label:
/// `"Yes"`/`"No"` unless [`labels`](Self::labels) names others.
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
    /// Bind the column to a `bool` field lens, which names it and sorts it,
    /// and a projection that reads the value off a row (upstream gap #119).
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

    /// The words for `true` and `false`, which the icon's screen-reader
    /// label and the CSV export carry.
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

/// A table's columns, as the table stores them: shared, so the list, the
/// export and the live-search handler read one declaration.
pub(crate) type BoxColumn<M> = Arc<dyn Column<M>>;

/// Convert a single built-in column, or a tuple of any [`Column`]s, into a
/// table's column list.
///
/// A tuple takes columns of any type, an app's own among them. A single
/// column converts on its own when it is a built-in; a single app column is
/// a one-element tuple, `(MyColumn,)`, or goes through
/// [`Table::column`](super::Table::column).
///
/// Tuple arities stop at eight, the ceiling every tuple-collection trait
/// shares: `IntoFilters` in `resource/filter.rs` and `IntoSchema` in
/// `schema/tree.rs`. Without variadic generics the idiom is one
/// `macro_rules!` invocation per arity; [`Table::column`](super::Table::column)
/// appends past it.
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

/// The tuple impls of [`IntoColumns`], one arity per invocation. Every
/// element is a [`Column`], so `(a, (b, c))` is not a column list: a
/// table's columns sit in one flat tuple.
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
