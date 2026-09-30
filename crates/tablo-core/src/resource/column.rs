//! Table columns: [`TextColumn`] plus the [`IntoColumns`] seam.

use std::{borrow::Cow, sync::Arc};

use toasty::stmt::{Expr, OrderByExpr};

use crate::schema::{FieldLens, lens_field, lens_label};

/// The share of the table a [`ColumnWidth::Narrow`] column claims, in whole
/// percent.
pub(crate) const NARROW_DEFAULT_PERCENT: u8 = 10;

/// The width a [`TextColumn`] claims in the table's fixed layout.
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
/// of the table. [`TextColumn::width`] overrides either, which is the seam for
/// a column whose content disagrees with its kind.
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
    /// Relations this column's projection reads, as includes on the model.
    includes: Vec<toasty_core::stmt::Include>,
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
        let field = lens_field(path.clone(), &M::schema());
        Self {
            path: Some(path),
            name: field.name.app_unwrap().to_string(),
            label: lens_label(&field),
            project: Arc::new(project),
            searchable: false,
            sortable: false,
            width: ColumnWidth::Wide,
            includes: Vec::new(),
        }
    }

    /// A computed, display-only column (CONTEXT.md Column: "a computed value").
    ///
    /// No field lens — so it cannot be searchable or sortable (it maps to no
    /// query predicate) — but any cell projection compiles: booleans,
    /// timestamps, joined values. Calling `.searchable()` / `.sortable()` on
    /// a computed column panics: a lying sort link / search promise
    /// is worse than a loud build error.
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
            includes: Vec::new(),
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
    pub fn include<T>(mut self, relation: impl Into<toasty::stmt::Include<M, T>>) -> Self {
        let include: toasty_core::stmt::Include = relation.into().into();
        if !self.includes.contains(&include) {
            self.includes.push(include);
        }
        self
    }

    /// The relations this column declared, in declaration order.
    pub(crate) fn includes(&self) -> &[toasty_core::stmt::Include] {
        &self.includes
    }

    pub fn searchable(mut self) -> Self {
        assert!(
            self.path.is_some(),
            "searchable() on computed column '{}': computed columns map to no query predicate",
            self.label
        );
        self.searchable = true;
        self
    }

    pub fn sortable(mut self) -> Self {
        assert!(
            self.path.is_some(),
            "sortable() on computed column '{}': computed columns map to no query predicate",
            self.label
        );
        self.sortable = true;
        self
    }

    pub fn is_searchable(&self) -> bool {
        self.searchable
    }

    pub fn is_sortable(&self) -> bool {
        self.sortable
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

    /// The width this column declares, which the renderer emits on its `th`
    /// and on every `td` of its column.
    pub fn column_width(&self) -> ColumnWidth {
        self.width
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// App-level field name (from the lens). Identifies the column in the
    /// `?sort=` URL parameter.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Render the cell for one row via the typed projection.
    pub fn render_cell(&self, row: &M) -> String {
        (self.project)(row)
    }

    /// The search predicate for this column: a portable, escaped
    /// **substring** match.
    ///
    /// `like_with_escape` keeps the pattern parameterised and lowers to the
    /// same `LIKE … ESCAPE '\\'` on every driver, and
    /// `escape_like_pattern` makes the term literal — a `%` or `_` the user
    /// typed matches that character, it does not act as a wildcard. Note the
    /// driver difference `LIKE` brings: SQLite compares ASCII
    /// case-insensitively, PostgreSQL case-sensitively.
    pub fn to_search_expr(&self, term: &str) -> Option<Expr<bool>> {
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

    pub fn to_order_by(&self, descending: bool) -> Option<OrderByExpr> {
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

/// Convert a single column or tuple of columns into `Vec<TextColumn<M>>`.
///
/// Tuple members are `TextColumn<M>` themselves, so nothing sits between the
/// column types. A single column converts on its own, with no
/// one-element tuple.
///
/// Tuple arities stop at eight, the ceiling every tuple-collection trait
/// shares: `IntoFilters` in `resource/filter.rs`, `IntoSchema` in
/// `schema/tree.rs`, and `IntoRelationColumns` in `resource/relation.rs`.
/// Without variadic generics the idiom is one `macro_rules!` invocation per
/// arity, and eight covers the widest tuple a Resource declares. Extend every
/// list together when a real Resource needs more.
pub trait IntoColumns<M> {
    fn into_columns(self) -> Vec<TextColumn<M>>;
}

impl<M> IntoColumns<M> for TextColumn<M> {
    fn into_columns(self) -> Vec<TextColumn<M>> {
        vec![self]
    }
}

/// Generate the tuple impls of a column-list trait, arities two to eight:
/// `$trait::$method` collects a tuple of `$col<T>` into a `Vec` and hands it to
/// `$wrap`. [`IntoColumns`] and
/// [`IntoRelationColumns`](super::IntoRelationColumns) share it, so the two
/// column lists accept the same tuple shapes. Every element is one column type,
/// so `(a, (b, c))` is not a column list: a table's columns sit in one flat
/// tuple.
macro_rules! column_tuples {
    ($trait:ident, $method:ident, $col:ident, $out:ty, $wrap:expr) => {
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c, d);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c, d, e);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c, d, e, f);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c, d, e, f, g);
        column_tuples!(@arity $trait, $method, $col, $out, $wrap; a, b, c, d, e, f, g, h);
    };
    (@arity $trait:ident, $method:ident, $col:ident, $out:ty, $wrap:expr; $($v:ident),+) => {
        impl<T> $trait<T> for ($(column_tuples!(@element $col $v)),+) {
            fn $method(self) -> $out {
                let ($($v,)+) = self;
                ($wrap)(vec![$($v,)+])
            }
        }
    };
    (@element $col:ident $v:ident) => { $col<T> };
}
pub(crate) use column_tuples;

column_tuples!(
    IntoColumns,
    into_columns,
    TextColumn,
    Vec<TextColumn<T>>,
    |columns| columns
);

/// The table-level floor a wide column contributes to the table's
/// `min-width`, in whole rem.
///
/// A wide column declares no width, so a sum of declared widths alone would
/// let it crush to zero on a narrow viewport. Six rem keeps body text readable
/// and, summed across the wide columns, trips the wrapper's horizontal scroll
/// before the fixed layout crushes them.
pub(crate) const WIDE_COLUMN_MIN_REM: u8 = 6;

/// The most of the table the kind defaults claim together.
///
/// The defaults are shares of the table, and the columns that declare none
/// take what they leave: a total over 100% gives those columns no space at
/// all, and `table-fixed` renders a column with no space at zero width, header
/// text included. The budget keeps the rest of the table for them whatever the
/// column set.
pub(crate) const DEFAULT_WIDTH_BUDGET_PERCENT: u8 = 60;

/// The share a kind default claims, scaled down when the table's defaults
/// together (`total`) exceed [`DEFAULT_WIDTH_BUDGET_PERCENT`].
pub(crate) fn scaled_default_percent(nominal: u8, total: u32) -> u8 {
    if total <= u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) {
        return nominal;
    }
    let scaled = u32::from(nominal) * u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) / total;
    // `scaled` is at most the budget, so the conversion cannot fail.
    u8::try_from(scaled).unwrap_or(DEFAULT_WIDTH_BUDGET_PERCENT)
}

/// The `style` value a kind default emits.
pub(crate) fn default_width_style(percent: u8) -> Cow<'static, str> {
    Cow::Owned(format!("width: {percent}%"))
}

/// The `style` a data column's cells carry: an explicit `Rem`/`Percent`
/// verbatim, a kind default scaled against the table's defaults (`total`), and
/// nothing for a wide column, which takes a share of what the declared ones
/// leave.
pub(crate) fn column_width_style(width: ColumnWidth, total: u32) -> Option<Cow<'static, str>> {
    width.explicit_css().or_else(|| {
        width
            .default_percent()
            .map(|nominal| default_width_style(scaled_default_percent(nominal, total)))
    })
}

/// The terms of a fixed-layout table's `min-width`: every share as emitted and
/// the lengths as one rem total.
///
/// With `w-full` the table never exceeds its container on its own, so without
/// the floor the wrapper's `overflow-x-auto` never scrolls; with it the table
/// keeps its measure on a narrow viewport and the wrapper scrolls.
#[derive(Default)]
pub(crate) struct MinWidth {
    percent: Vec<u8>,
    rem: u32,
}

impl MinWidth {
    /// A share of the table, as the column emits it.
    pub(crate) fn share(&mut self, percent: u8) {
        self.percent.push(percent);
    }

    /// A length, in whole rem.
    pub(crate) fn rem(&mut self, rem: u8) {
        self.rem += u32::from(rem);
    }

    /// A data column's term: its scaled share or its length, and
    /// [`WIDE_COLUMN_MIN_REM`] for a wide column, which declares nothing.
    pub(crate) fn column(&mut self, width: ColumnWidth, total: u32) {
        match width {
            ColumnWidth::Wide => self.rem(WIDE_COLUMN_MIN_REM),
            ColumnWidth::Narrow => {
                self.share(scaled_default_percent(NARROW_DEFAULT_PERCENT, total))
            }
            ColumnWidth::Rem(rem) => self.rem(rem),
            ColumnWidth::Percent(share) => self.share(share),
        }
    }

    /// The `min-width` style, emitted only when the sum carries a length:
    /// shares alone are a fraction of the container and can never overflow it.
    pub(crate) fn style(&self) -> Option<Cow<'static, str>> {
        (self.rem > 0).then(|| {
            let mut parts: Vec<String> = self
                .percent
                .iter()
                .map(|share| format!("{share}%"))
                .collect();
            parts.push(format!("{}rem", self.rem));
            if parts.len() == 1 {
                Cow::Owned(format!("min-width: {}", parts[0]))
            } else {
                Cow::Owned(format!("min-width: calc({})", parts.join(" + ")))
            }
        })
    }
}

#[cfg(test)]
mod tests;
