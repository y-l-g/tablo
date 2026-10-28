//! [`TextColumn`]: one field rendered as text, searchable on a string field, and the escaped
//! `LIKE` pattern its search builds.

use std::sync::Arc;

use derive_where::derive_where;
use toasty::stmt::{Expr, OrderByExpr};
use topcoat::{context::Cx, view::*};

use super::{Column, ColumnBase, ColumnWidth, sort_order};
use crate::{
    Lens, Text,
    display::{Alignment, Color, Currency, Numeric},
    values::FormScalar,
};

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
/// [`sortable`](Self::sortable) orders by the stored column, so a type a driver stores as text
/// sorts by its spelling: a decimal column on SQLite, MySQL or Turso lists `10.00`, `100`, `9.50`.
/// The [`ModelColumns`](derive@crate::ModelColumns) derive leaves a type that is not
/// [`SORTABLE`](crate::extend::TypedValue::SORTABLE) unsortable.
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
    format: Arc<dyn Fn(&T) -> Text + Send + Sync>,
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
    /// The fields the description reads, when declared.
    description_reads: Option<crate::lens::Reads<M>>,
}

/// A searchable column's predicate for an escaped `LIKE` pattern.
type SearchFn = Arc<dyn Fn(String) -> Expr<bool> + Send + Sync>;

/// A column's color for a value.
type ColorFn<T> = Arc<dyn Fn(&T) -> Option<Color> + Send + Sync>;

/// A column's second line for a row.
type DescriptionFn<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

/// The escape character the search pattern declares to `LIKE`.
const LIKE_ESCAPE: char = '\\';

/// Wrap `term` as a `LIKE` pattern matching it anywhere in the column.
pub(super) fn escape_like_pattern(term: &str) -> String {
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
            description_reads: None,
        }
    }

    /// Render the field's value through `format`: a `String`, or a [`Text`] the request's locale
    /// spells.
    pub fn format<L: Into<Text>>(
        mut self,
        format: impl Fn(&T) -> L + Send + Sync + 'static,
    ) -> Self {
        self.format = Arc::new(move |value| format(value).into());
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
    ///
    /// The description may read any field, so a resource that
    /// [restricts a field](crate::ResourceDef::field_access) refuses the column until
    /// [`reading`](Self::reading) declares the ones it reads.
    pub fn description(
        mut self,
        description: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        self.description = Some(Arc::new(description));
        self
    }

    /// Declare the fields the [`description`](Self::description) reads, beside the column's own.
    pub fn reading(mut self, reads: crate::lens::Reads<M>) -> Self {
        self.description_reads = Some(reads);
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
                .map(|number| crate::display::format_number(number, decimals))
                .unwrap_or_default()
                .into()
        });
        self.amount()
    }

    /// Spell the value as an amount of `currency`, `$1,234.50`, aligned to the end with
    /// equal-width digits.
    pub fn money(mut self, currency: Currency) -> Self {
        self.format = Arc::new(move |value: &T| currency.format(value).into());
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
    base_column_methods!(binding, reads);

    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        let own = self.base.reads();
        match (&self.description, &self.description_reads) {
            (None, _) => Some(own),
            (Some(_), None) => None,
            (Some(_), Some(declared)) => Some(own.union(declared)),
        }
    }

    fn text(&self, cx: &Cx, row: &M) -> String {
        (self.format)(self.lens.read(row)).localize(cx)
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.lens.read(row);
        let text = (self.format)(value).localize(cx);
        let color = (!text.trim().is_empty() && (self.badge || self.tinted))
            .then(|| (self.color)(value))
            .flatten();
        let description = self.description.as_ref().map(|describe| describe(row));
        text_cell(cx, text, color, self.badge, self.tabular, description)
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

/// A text cell: `text`, as a badge or tinted by `color`, over its `description`. A free function,
/// so tablo-core compiles the view once rather than once for each model and field type.
fn text_cell<'a>(
    cx: &'a Cx,
    text: String,
    color: Option<Color>,
    badge: bool,
    tabular: bool,
    description: Option<String>,
) -> BoxView<'a> {
    let value: BoxView<'a> = if text.trim().is_empty() {
        crate::schema::value_cell(cx, &text)
    } else if badge {
        badge_cell(cx, text, color)
    } else {
        let class = [
            color.map(Color::text_class),
            tabular.then_some("tabular-nums"),
        ]
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
