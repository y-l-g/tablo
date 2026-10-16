//! Table and detail columns: the [`Column`] trait, the built-in [`TextColumn`], [`DateColumn`],
//! [`ComputedColumn`], [`RelationColumn`], [`CountColumn`], [`BooleanColumn`], [`FileColumn`],
//! [`ColorColumn`], [`ImageColumn`] and [`EmbeddedColumn`], and the [`IntoColumns`] seam.

use std::sync::Arc;

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
    (sortable) => {
        base_builders!();

        /// Make the header a sort link.
        pub fn sortable(mut self) -> Self {
            self.sortable = true;
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

fn sort_order<M, T>(path: &Path<M, T>, descending: bool) -> OrderByExpr {
    if descending {
        path.clone().desc()
    } else {
        path.clone().asc()
    }
}

mod display;
mod embedded;
pub(crate) mod model;
mod relation;
mod repeater;

pub use display::{Alignment, Color, Currency, Number, Numeric};
pub use embedded::EmbeddedColumn;
pub use model::ModelColumns;
pub use relation::{CountColumn, RelationColumn, RelationLens, ToOneRelation};
pub use repeater::RepeaterColumn;

/// One column of a table or a [`Detail`](crate::Detail) declares its label, its value read off
/// the record, and its query predicates.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
        ColumnWidth::Auto
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

/// A column of one field, rendered as text and bound through a [`Lens`] for sorting and search.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
/// string field is [`searchable`](Self::searchable).
///
/// A column presents its value as a [`badge`](Self::badge), in a [`color`](Self::color), with a
/// muted [`description`](Self::description) line under it, or as a right-aligned
/// [`numeric`](Self::numeric) or [`money`](Self::money) amount:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     number: String,
/// #     customer: String,
/// #     status: Status,
/// #     total_cents: i64,
/// # }
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// # enum Status { #[option(color = "warning")] Pending, #[option(color = "success")] Paid }
/// use tablo_core::{Currency, TextColumn, lens};
///
/// TextColumn::new(lens!(Order.number)).description(|order: &Order| order.customer.clone());
/// TextColumn::new(lens!(Order.status)).badge();
/// TextColumn::new(lens!(Order.total_cents)).money(Currency::USD.minor_units());
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
    /// The value's color: its own, an `Options` enum's, unless [`color`](Self::color) says
    /// otherwise.
    #[derive_where(skip(Debug))]
    color: ColorFn<T>,
    /// Whether the color tints the text, which only a declared [`color`](Self::color) does.
    tinted: bool,
    badge: bool,
    /// Whether the digits take equal widths, so a column of amounts lines up.
    tabular: bool,
    alignment: Alignment,
    #[derive_where(skip(Debug))]
    description: Option<DescriptionFn<M>>,
}

/// A searchable column's predicate for an escaped `LIKE` pattern.
type SearchFn = Arc<dyn Fn(String) -> Expr<bool> + Send + Sync>;

/// A column's color for a value.
type ColorFn<T> = Arc<dyn Fn(&T) -> Option<Color> + Send + Sync>;

/// A column's second line for a row.
type DescriptionFn<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

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
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
            lens,
            format: Arc::new(T::to_label),
            search: None,
            sortable: false,
            color: Arc::new(T::color),
            tinted: false,
            badge: false,
            tabular: false,
            alignment: Alignment::Start,
            description: None,
        }
    }

    /// Render the field's value through `format`.
    pub fn format(mut self, format: impl Fn(&T) -> String + Send + Sync + 'static) -> Self {
        self.format = Arc::new(format);
        self
    }

    /// Show the value as a badge, with a dot in its color: an `Options` enum's
    /// `#[option(color = "…")]`, or the [`color`](Self::color) declared here.
    pub fn badge(mut self) -> Self {
        self.badge = true;
        self
    }

    /// Color the value: its badge's dot, or its text when it is no badge.
    pub fn color(mut self, color: impl Fn(&T) -> Option<Color> + Send + Sync + 'static) -> Self {
        self.color = Arc::new(color);
        self.tinted = true;
        self
    }

    /// Show `description(row)` as a muted line under the value, such as a customer's email under
    /// their name. Search, sort and export read the value alone.
    pub fn description(
        mut self,
        description: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        self.description = Some(Arc::new(description));
        self
    }

    /// Align the header and the cells.
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    base_builders!(sortable);
}

impl<M, T> TextColumn<M, T>
where
    M: toasty::schema::Model,
    T: Numeric + 'static,
{
    /// Spell the value as a number with `decimals` digits after the point and its thousands
    /// grouped, `12,345.60`, aligned to the end with equal-width digits.
    pub fn numeric(mut self, decimals: u8) -> Self {
        self.format = Arc::new(move |value: &T| {
            value
                .number()
                .map(|number| display::format_number(number, decimals))
                .unwrap_or_default()
        });
        self.amount()
    }

    /// Spell the value as an amount of `currency`, `$1,234.50`, aligned to the end with
    /// equal-width digits.
    pub fn money(mut self, currency: Currency) -> Self {
        self.format = Arc::new(move |value: &T| currency.format(value));
        self.amount()
    }

    /// Align the column as an amount.
    fn amount(mut self) -> Self {
        self.alignment = Alignment::End;
        self.tabular = true;
        self.base.width = ColumnWidth::Auto;
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
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        (self.format)(self.lens.read(row))
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.lens.read(row);
        let text = (self.format)(value);
        let description = self.description.as_ref().map(|describe| describe(row));
        let value: BoxView<'a> = if text.trim().is_empty() {
            crate::schema::value_cell(cx, &text)
        } else if self.badge {
            badge_cell(cx, text, (self.color)(value))
        } else {
            let color = self
                .tinted
                .then(|| (self.color)(value))
                .flatten()
                .map(Color::text_class);
            let class = [color, self.tabular.then_some("tabular-nums")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            if class.is_empty() {
                view! { cx => (text) }.boxed()
            } else {
                view! { cx => <span class=(class)>(text)</span> }.boxed()
            }
        };
        match description.filter(|description| !description.trim().is_empty()) {
            None => value,
            Some(description) => view! {
                cx =>
                <div class="flex min-w-0 flex-col">
                    <span class="truncate">(value)</span>
                    <span class="truncate text-xs text-muted-foreground">
                        (description)
                    </span>
                </div>
            }
            .boxed(),
        }
    }

    fn alignment(&self) -> Alignment {
        self.alignment
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
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

/// A column of a date field, rendered as a calendar day.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, published_at: jiff::Timestamp }
/// tablo_core::DateColumn::new(tablo_core::lens!(Post.published_at)).sortable();
/// ```
///
/// The cell spells the day `Oct 14, 2026`, and a `None` renders the table's dash. The export
/// writes the stored spelling, `2026-10-14T09:30:00Z` or `2026-10-14`, as a [`TextColumn`] does;
/// a group header follows the field, not the column.
#[derive_where(Clone, Debug)]
pub struct DateColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    sortable: bool,
}

/// The field types a [`DateColumn`] spells as a calendar day: `jiff::civil::Date`,
/// `jiff::civil::DateTime`, `jiff::Timestamp` (read in UTC, as a
/// [`DateFilter`](crate::DateFilter) reads it) and an `Option` of any of them.
pub trait DateValue: Sized + Send + Sync + 'static {
    /// The value's calendar day, `None` when it holds none.
    fn calendar_date(&self) -> Option<jiff::civil::Date>;

    /// The value's stored spelling, which a column's text and export carry. The default spells
    /// the calendar day as an ISO date.
    fn stored_text(&self) -> String {
        self.calendar_date()
            .map(|day| day.to_string())
            .unwrap_or_default()
    }
}

impl DateValue for jiff::civil::Date {
    fn calendar_date(&self) -> Option<jiff::civil::Date> {
        Some(*self)
    }

    fn stored_text(&self) -> String {
        FormScalar::to_label(self)
    }
}

impl DateValue for jiff::civil::DateTime {
    fn calendar_date(&self) -> Option<jiff::civil::Date> {
        Some(self.date())
    }

    fn stored_text(&self) -> String {
        FormScalar::to_label(self)
    }
}

impl DateValue for jiff::Timestamp {
    fn calendar_date(&self) -> Option<jiff::civil::Date> {
        Some(self.to_zoned(jiff::tz::TimeZone::UTC).date())
    }

    fn stored_text(&self) -> String {
        FormScalar::to_label(self)
    }
}

impl<T: DateValue> DateValue for Option<T> {
    fn calendar_date(&self) -> Option<jiff::civil::Date> {
        self.as_ref().and_then(DateValue::calendar_date)
    }

    fn stored_text(&self) -> String {
        self.as_ref()
            .map(DateValue::stored_text)
            .unwrap_or_default()
    }
}

impl<M, T> DateColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the date field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: DateValue,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
        }
    }

    base_builders!(sortable);
}

impl<M, T> Column<M> for DateColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: DateValue,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).stored_text()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let day = self
            .lens
            .read(row)
            .calendar_date()
            .map(|day| day.strftime("%b %-d, %Y").to_string())
            .unwrap_or_default();
        crate::schema::value_cell(cx, &day)
    }

    fn is_sortable(&self) -> bool {
        self.sortable
    }

    fn order_by(&self, descending: bool) -> Option<OrderByExpr> {
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

/// A display-only column rendering any text from the row, with no query predicate.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
/// It maps to no column, so it neither searches nor sorts.
#[derive_where(Clone, Debug)]
pub struct ComputedColumn<M> {
    name: String,
    label: String,
    #[derive_where(skip(Debug))]
    project: Arc<dyn Fn(&M) -> String + Send + Sync>,
    width: ColumnWidth,
    alignment: Alignment,
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
            width: ColumnWidth::Auto,
            alignment: Alignment::Start,
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

    /// Align the header and the cells.
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
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

    fn alignment(&self) -> Alignment {
        self.alignment
    }

    fn includes(&self) -> Includes<M> {
        self.includes.clone()
    }
}

/// A column of a `bool` field rendered as an icon.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            sortable: false,
            labels: ("Yes".to_string(), "No".to_string()),
        }
    }

    base_builders!(sortable);

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
            (tablo_ui::icons::CIRCLE_X, "size-4 text-destructive")
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
        self.sortable
            .then(|| sort_order(self.lens.path(), descending))
    }
}

/// A column of a `String` field holding an uploaded file's path, rendered as a link to the file.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
            base: ColumnBase::new(lens.path(), ColumnWidth::Grow),
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

/// `text` as an outline badge, led by a dot in `color` when it has one.
fn badge_cell<'a>(cx: &'a Cx, text: String, color: Option<Color>) -> BoxView<'a> {
    view! {
        cx =>
        tablo_ui::badge(
            variant: tablo_ui::BadgeVariant::Outline,
            if let Some(color) = color {
                <span class=(color.dot_class()) aria-hidden="true"></span>
            }
            (text)
        )
    }
    .boxed()
}

/// A column of a `String` field holding a CSS color, rendered as a swatch beside the value.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Team { #[key] #[auto] id: uuid::Uuid, color: String }
/// tablo_core::ColorColumn::new(tablo_core::lens!(Team.color));
/// ```
///
/// A hex color (`#3b82f6`) or a color function (`rgb(…)`, `hsl(…)`, `oklch(…)`, …) shows its
/// swatch; any other value shows as text alone, since it reaches a `style` attribute.
#[derive_where(Clone, Debug)]
pub struct ColorColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
}

impl<M, T> ColorColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` or `Option<String>` field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: toasty::schema::Field<Inner = String>,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
        }
    }

    base_builders!();
}

impl<M, T> Column<M> for ColorColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_form()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.text(cx, row).trim().to_string();
        if !is_css_color(&value) {
            return crate::schema::value_cell(cx, &value);
        }
        let swatch = format!("background-color: {value}");
        view! {
            cx =>
            <span class="inline-flex items-center gap-2">
                <span
                    class="size-4 shrink-0 rounded-sm ring-1 ring-foreground/15 ring-inset"
                    style=(swatch)
                    aria-hidden="true"
                ></span>
                <span class="font-mono text-xs">(value)</span>
            </span>
        }
        .boxed()
    }
}

/// Whether `value` is a hex color or a color function of plain arguments, safe inside a `style`
/// attribute: nothing in it can end the declaration or open another.
fn is_css_color(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    const FUNCTIONS: &[&str] = &[
        "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(",
    ];
    let lower = value.to_ascii_lowercase();
    let Some(function) = FUNCTIONS.iter().find(|f| lower.starts_with(**f)) else {
        return false;
    };
    let Some(arguments) = lower[function.len()..].strip_suffix(')') else {
        return false;
    };
    arguments
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b" .,%/+-".contains(&b))
}

/// A column of a `String` field holding an uploaded image's path, rendered as a thumbnail.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Product { #[key] #[auto] id: uuid::Uuid, image: String }
/// tablo_core::ImageColumn::new(tablo_core::lens!(Product.image)).circular();
/// ```
///
/// A rooted path, such as an upload's, shows as a 40px square thumbnail; an empty value shows
/// the table's dash, and any other value its text. An absolute `http(s)` URL shows as an image
/// only once [`remote`](Self::remote) allows it.
#[derive_where(Clone, Debug)]
pub struct ImageColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    size: u16,
    circular: bool,
    remote: bool,
}

impl<M, T> ImageColumn<M, T>
where
    M: toasty::schema::Model,
{
    /// Bind the column to the `String` or `Option<String>` field `lens` reads.
    pub fn new(lens: Lens<M, T>) -> Self
    where
        T: toasty::schema::Field<Inner = String>,
    {
        Self {
            base: ColumnBase::new(lens.path(), ColumnWidth::Auto),
            lens,
            size: 40,
            circular: false,
            remote: false,
        }
    }

    /// Show the thumbnail `pixels` wide and tall.
    pub fn size(mut self, pixels: u16) -> Self {
        self.size = pixels;
        self
    }

    /// Crop the thumbnail to a circle, for an avatar.
    pub fn circular(mut self) -> Self {
        self.circular = true;
        self
    }

    /// Also show an absolute `http(s)` URL as an image. Every browser that renders the table
    /// then loads it from that host as the page opens, so whoever can write the field learns who
    /// views the list, and when: allow it for a field only trusted users write.
    pub fn remote(mut self) -> Self {
        self.remote = true;
        self
    }

    base_builders!();
}

impl<M, T> Column<M> for ImageColumn<M, T>
where
    M: toasty::schema::Model + Send + Sync + 'static,
    T: FormScalar + Send + Sync + 'static,
{
    base_column_methods!(bind);

    fn text(&self, _cx: &Cx, row: &M) -> String {
        self.lens.read(row).to_form()
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let src = self.text(cx, row).trim().to_string();
        let shown = if self.remote {
            crate::schema::is_linkable(&src)
        } else {
            crate::schema::is_rooted(&src)
        };
        if !shown {
            return crate::schema::value_cell(cx, &src);
        }
        let class = if self.circular {
            "shrink-0 rounded-full object-cover ring-1 ring-foreground/10"
        } else {
            "shrink-0 rounded-md object-cover ring-1 ring-foreground/10"
        };
        let size = self.size.to_string();
        view! {
            cx =>
            <img
                src=(src)
                alt=""
                loading="lazy"
                referrerpolicy="no-referrer"
                width=(size.clone())
                height=(size)
                class=(class)
            >
        }
        .boxed()
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
