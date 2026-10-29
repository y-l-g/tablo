//! [`TextColumn`]: one field rendered as text, searchable on a string field, and the escaped
//! `LIKE` pattern its search builds.

use std::sync::Arc;

use derive_where::derive_where;
use toasty::stmt::{Expr, OrderByExpr};
use topcoat::{context::Cx, icon::IconData, view::*};

use super::{
    Column, ColumnBase, ColumnWidth, ReadFn,
    cell::{TextCell, text_cell},
    sort_order,
};
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
///
/// A cell also links to a [`url`](Self::url), shows a [`tooltip`](Self::tooltip), cuts a long
/// value to a [`limit`](Self::limit), offers a [`copyable`](Self::copyable) button, shows an
/// [`icon`](Self::icon) before the value, and a [`placeholder`](Self::placeholder) for an empty
/// one:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, email: String, bio: String, phone: String }
/// use tablo_core::{Reads, TextColumn, lens};
///
/// TextColumn::new(lens!(User.email))
///     .url(|user: &User| Some(format!("mailto:{}", user.email)))
///     .reading(Reads::nothing())
///     .copyable();
/// TextColumn::new(lens!(User.bio)).limit(40);
/// TextColumn::new(lens!(User.phone)).placeholder("No phone");
/// ```
///
/// A closure over the row, a [`description`](Self::description), a [`url`](Self::url), a
/// [`tooltip`](Self::tooltip) or a [`money_by`](Self::money_by), may read any field, so a
/// resource that [restricts a field](crate::ResourceDef::field_access) refuses the column until
/// [`reading`](Self::reading) declares the ones they read.
#[derive_where(Clone, Debug)]
pub struct TextColumn<M, T> {
    lens: Lens<M, T>,
    base: ColumnBase,
    #[derive_where(skip(Debug))]
    format: FormatFn<M, T>,
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
    description: Option<RowFn<M, String>>,
    /// The value's icon: its own, an `Options` enum's, unless [`icon`](Self::icon) says
    /// otherwise.
    #[derive_where(skip(Debug))]
    icon: ReadFn<T, Option<IconData>>,
    #[derive_where(skip(Debug))]
    url: Option<RowFn<M, Option<String>>>,
    new_tab: bool,
    #[derive_where(skip(Debug))]
    tooltip: Option<RowFn<M, Option<String>>>,
    limit: Option<usize>,
    copyable: bool,
    placeholder: Option<Text>,
    /// Whether a closure over the row reads beyond the column's field.
    reads_row: bool,
    /// The fields the closures over the row read, when declared.
    row_reads: Option<crate::lens::Reads<M>>,
}

/// A searchable column's predicate for an escaped `LIKE` pattern.
type SearchFn = Arc<dyn Fn(String) -> Expr<bool> + Send + Sync>;

/// A column's color for a value.
type ColorFn<T> = ReadFn<T, Option<Color>>;

/// A closure over the row.
type RowFn<M, V> = ReadFn<M, V>;

/// A column's spelling of its value, which may read the rest of the row.
type FormatFn<M, T> = Arc<dyn Fn(&M, &T) -> Text + Send + Sync>;

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
            format: Arc::new(|_, value: &T| value.to_label()),
            search: None,
            sortable: false,
            color: Arc::new(T::color),
            tinted: false,
            badge: false,
            tabular: false,
            alignment: Alignment::Start,
            description: None,
            icon: Arc::new(T::icon),
            url: None,
            new_tab: false,
            tooltip: None,
            limit: None,
            copyable: false,
            placeholder: None,
            reads_row: false,
            row_reads: None,
        }
    }

    /// Render the field's value through `format`: a `String`, or a [`Text`] the request's locale
    /// spells.
    pub fn format<L: Into<Text>>(
        mut self,
        format: impl Fn(&T) -> L + Send + Sync + 'static,
    ) -> Self {
        self.format = Arc::new(move |_, value| format(value).into());
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
        self.reads_row = true;
        self
    }

    /// Link the value to `url(row)`, or show it unlinked when that is `None`. The link takes a
    /// relative URL or an `http`, `https`, `mailto` or `tel` one; any other scheme, such as
    /// `javascript:`, leaves the value unlinked.
    pub fn url(mut self, url: impl Fn(&M) -> Option<String> + Send + Sync + 'static) -> Self {
        self.url = Some(Arc::new(url));
        self.reads_row = true;
        self
    }

    /// Open the [`url`](Self::url) in a new tab.
    pub fn open_url_in_new_tab(mut self) -> Self {
        self.new_tab = true;
        self
    }

    /// Show `tooltip(row)` when the pointer rests on the value.
    pub fn tooltip(
        mut self,
        tooltip: impl Fn(&M) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.tooltip = Some(Arc::new(tooltip));
        self.reads_row = true;
        self
    }

    /// Show at most `chars` characters of the value, then an ellipsis; the whole value shows
    /// when the pointer rests on it, unless a [`tooltip`](Self::tooltip) says otherwise. Search,
    /// sort and export read the whole value.
    pub fn limit(mut self, chars: usize) -> Self {
        self.limit = Some(chars);
        self
    }

    /// Follow the value with a button copying it to the clipboard, which needs JavaScript.
    pub fn copyable(mut self) -> Self {
        self.copyable = true;
        self
    }

    /// Show `placeholder` for an empty value, in place of a dash. An export writes the empty
    /// value.
    pub fn placeholder(mut self, placeholder: impl Into<Text>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Show `icon(value)` before the value, in its color, replacing a badge's dot.
    pub fn icon(mut self, icon: impl Fn(&T) -> Option<IconData> + Send + Sync + 'static) -> Self {
        self.icon = Arc::new(icon);
        self
    }

    /// Declare the fields the closures over the row read, beside the column's own: the
    /// [`description`](Self::description), [`url`](Self::url), [`tooltip`](Self::tooltip) and
    /// [`money_by`](Self::money_by).
    pub fn reading(mut self, reads: crate::lens::Reads<M>) -> Self {
        self.row_reads = Some(reads);
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
        self.format = Arc::new(move |_, value: &T| {
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
        self.format = Arc::new(move |_, value: &T| currency.format(value).into());
        self.amount()
    }

    /// Spell the value as an amount of the currency `currency(row)` names, such as an order's
    /// own, aligned as [`money`](Self::money) aligns it.
    pub fn money_by(mut self, currency: impl Fn(&M) -> Currency + Send + Sync + 'static) -> Self {
        self.format = Arc::new(move |row, value: &T| currency(row).format(value).into());
        self.reads_row = true;
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

    /// A closure over the row may reach any relation the page loads, as a computed column's does.
    fn related(&self) -> crate::column::Related {
        if self.reads_row {
            crate::column::Related::Row
        } else {
            crate::column::Related::None
        }
    }

    fn reads(&self) -> Option<crate::lens::Reads<M>> {
        let own = self.base.reads();
        match (self.reads_row, &self.row_reads) {
            (false, _) => Some(own),
            (true, None) => None,
            (true, Some(declared)) => Some(own.union(declared)),
        }
    }

    fn text(&self, cx: &Cx, row: &M) -> String {
        (self.format)(row, self.lens.read(row)).localize(cx)
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &M) -> BoxView<'a> {
        let value = self.lens.read(row);
        let text = (self.format)(row, value).localize(cx);
        let color = (!text.trim().is_empty())
            .then(|| (self.color)(value))
            .flatten();
        // An icon takes the value's color, as a badge's dot does; only a declared color tints the
        // text.
        let icon = (self.icon)(value).map(|icon| (icon, color));
        let color = color.filter(|_| self.badge || self.tinted);
        text_cell(
            cx,
            TextCell {
                text,
                color,
                badge: self.badge,
                tabular: self.tabular,
                icon,
                description: self.description.as_ref().map(|describe| describe(row)),
                url: self.url.as_ref().and_then(|url| url(row)),
                new_tab: self.new_tab,
                tooltip: self.tooltip.as_ref().and_then(|tooltip| tooltip(row)),
                limit: self.limit,
                copyable: self.copyable,
                placeholder: self.placeholder.clone(),
            },
        )
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
