//! [`Widget`] — one block of a page's widget grid, such as a row of [`Stat`]s — and the
//! [`WidgetDef`] it declares.

use std::{future::Future, sync::Arc};

use topcoat::{
    Result,
    context::Cx,
    icon::IconData,
    view::{BoxView, View, ViewExt},
};

use crate::{Color, naming::type_stem};

/// One block of a page's widget grid: a row of stats, a chart, a list of recent records.
///
/// A resource adds widgets above its list with
/// [`ResourceDef::list_widget`](crate::ResourceDef::list_widget). A page adds them with
/// [`PageDef::widget`](crate::PageDef::widget), and [`widgets`](crate::widgets) renders them where
/// the page's [`render`](crate::Page::render) places it:
///
/// ```rust
/// # use tablo_core::{Page, PageDef, Stat, Widget, WidgetDef};
/// # use topcoat::{Result, context::Cx, view::{View, view}};
/// struct Overview;
///
/// impl Widget for Overview {
///     fn declare() -> WidgetDef {
///         WidgetDef::new().full_width()
///     }
///
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(tablo_core::stats(
///             cx,
///             [Stat::new("Orders", "1,204").detail("12 today")],
///         ))
///     }
/// }
///
/// struct Dashboard;
///
/// impl Page for Dashboard {
///     fn declare() -> PageDef {
///         PageDef::new().widget::<Overview>()
///     }
///
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(view! {
///             cx =>
///             tablo_ui::page(
///                 tablo_ui::page_title("Dashboard")
///                 tablo_ui::page_content((tablo_core::widgets::<Self>(cx)))
///             )
///         })
///     }
/// }
/// ```
///
/// A widget reads what it shows itself. One that reads a resource's records asks
/// [`can_list`](crate::can_list) before it reads, shows or links anything, and reads them through
/// [`scoped_query`](crate::scoped_query), which keeps the caller's tenant and the resource's
/// [`Query`](crate::Resource) but not its policy.
pub trait Widget: Sized + Send + Sync + 'static {
    /// The widget's span and access. Defaults to [`WidgetDef::new`]: one column, for every user
    /// who may open the page.
    fn declare() -> WidgetDef {
        WidgetDef::new()
    }

    /// Render the widget's body. The grid checks [`WidgetDef::access`] before calling it. An
    /// error fails the whole page's response, as a page's own error does: the page answers with
    /// the error's status, and none of its other widgets render.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

/// Whether the request's user may see a widget.
type AccessFn = dyn Fn(&Cx) -> bool + Send + Sync;

/// How many columns of the page's widget grid a widget spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Span {
    /// That many columns, at most the grid's.
    Columns(u8),
    /// Every column.
    Full,
}

/// What a [`Widget`] declares: how many columns of the grid it spans, and who sees it.
pub struct WidgetDef {
    name: Option<&'static str>,
    span: Span,
    access: Arc<AccessFn>,
}

impl WidgetDef {
    /// A def with every default: one column, for every user who may open the page.
    pub fn new() -> Self {
        Self::default()
    }

    /// The widget's name, its `data-widget` hook and its key on the page; defaults to its type's
    /// name in kebab case, without `Widget` and generic arguments: `RecentOrdersWidget` →
    /// `recent-orders`, `Count<Book>` → `count`. Each widget of a page needs a distinct name, so a
    /// generic widget mounted twice names each mount.
    #[must_use]
    pub fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Spans `columns` columns of the grid, at most as many as it has, and at least one. On a
    /// narrow screen the grid stacks every widget in one column.
    #[must_use]
    pub fn span(mut self, columns: u8) -> Self {
        self.span = Span::Columns(columns.max(1));
        self
    }

    /// Spans every column of the grid.
    #[must_use]
    pub fn full_width(mut self) -> Self {
        self.span = Span::Full;
        self
    }

    /// Whether the request's user may see the widget. The grid leaves out a widget it refuses
    /// and never renders it. Defaults to every user who may open the page.
    #[must_use]
    pub fn access(mut self, access: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.access = Arc::new(access);
        self
    }
}

impl Default for WidgetDef {
    fn default() -> Self {
        Self {
            name: None,
            span: Span::Columns(1),
            access: Arc::new(|_| true),
        }
    }
}

impl std::fmt::Debug for WidgetDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetDef")
            .field("name", &self.name)
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

/// A [`WidgetDef`] for the widget `W`, as a page holds it.
pub(crate) struct MountedWidget {
    /// The widget's [`WidgetDef::name`].
    pub(crate) name: String,
    pub(crate) span: Span,
    access: Arc<AccessFn>,
    pub(crate) render: for<'a> fn(&'a Cx) -> crate::BoxFuture<'a, Result<BoxView<'a>>>,
}

impl MountedWidget {
    pub(crate) fn new<W: Widget>() -> Self {
        let def = W::declare();
        Self {
            name: def.name.map_or_else(
                || crate::naming::kebab_case(type_stem::<W>("Widget")),
                str::to_string,
            ),
            span: def.span,
            access: def.access,
            render: render::<W>,
        }
    }

    /// Whether the request's user may see the widget.
    pub(crate) fn admits(&self, cx: &Cx) -> bool {
        (self.access)(cx)
    }
}

impl std::fmt::Debug for MountedWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MountedWidget")
            .field("name", &self.name)
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

/// The widgets a page or a resource's list declares, in order, and how many columns their grid
/// has on a wide screen.
#[derive(Debug)]
pub(crate) struct WidgetGrid {
    pub(crate) widgets: Vec<MountedWidget>,
    pub(crate) columns: u8,
}

impl WidgetGrid {
    /// Adds `W` after the widgets already declared.
    pub(crate) fn push<W: Widget>(&mut self) {
        self.widgets.push(MountedWidget::new::<W>());
    }

    /// Sets the grid's columns, from 1 to 4.
    pub(crate) fn set_columns(&mut self, columns: u8) {
        self.columns = columns.clamp(1, 4);
    }

    /// The widgets the request may see, in order.
    pub(crate) fn admitted<'a>(&'a self, cx: &'a Cx) -> impl Iterator<Item = &'a MountedWidget> {
        self.widgets.iter().filter(|widget| widget.admits(cx))
    }

    /// Refuses two widgets of `T`'s grid sharing a name: each keys its cell.
    pub(crate) fn check<T>(&self, errors: &mut Vec<crate::DeclarationError>) {
        let mut seen = std::collections::HashSet::new();
        for widget in &self.widgets {
            if !seen.insert(widget.name.as_str()) {
                errors.push(crate::DeclarationError::of::<T>(
                    crate::declaration::Site::Registration,
                    crate::DeclarationErrorKind::DuplicateWidget {
                        name: widget.name.clone(),
                    },
                ));
            }
        }
    }
}

impl Default for WidgetGrid {
    fn default() -> Self {
        Self {
            widgets: Vec::new(),
            columns: 2,
        }
    }
}

fn render<W: Widget>(cx: &Cx) -> crate::BoxFuture<'_, Result<BoxView<'_>>> {
    Box::pin(async move { Ok(W::render(cx).await?.boxed()) })
}

/// Which way a [`Stat`]'s figure moved, shown as an arrow before its detail line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    /// The figure grew.
    Up,
    /// The figure shrank.
    Down,
    /// The figure held.
    Flat,
}

/// One figure of a [`stats`](crate::stats) row: what it counts, the value, and optionally a line
/// under it, an icon, a link, a trend and a sparkline.
///
/// ```rust
/// # use tablo_core::{Color, Stat, Trend};
/// Stat::new("Revenue", "$12,400")
///     .detail("8% more than last month")
///     .trend(Trend::Up)
///     .color(Color::Success)
///     .chart([9.1, 10.2, 11.0, 12.4]);
/// ```
#[derive(Clone, Debug)]
pub struct Stat {
    pub(crate) label: crate::Text,
    pub(crate) value: String,
    pub(crate) detail: Option<crate::Text>,
    pub(crate) icon: Option<IconData>,
    pub(crate) href: Option<String>,
    pub(crate) trend: Option<Trend>,
    pub(crate) color: Color,
    pub(crate) chart: Vec<f64>,
}

impl Stat {
    /// A figure labelled `label`, with `value` already formatted: `"1,204"`, `"98%"`.
    pub fn new(label: impl Into<crate::Text>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
            icon: None,
            href: None,
            trend: None,
            color: Color::Gray,
            chart: Vec::new(),
        }
    }

    /// The line under the value, such as `"12 this week"`.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<crate::Text>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The icon before the label.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The URL the whole tile links to, such as the list the figure counts.
    #[must_use]
    pub fn href(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    /// Which way the figure moved, as an arrow before the detail line; a stat with no detail line
    /// shows none.
    #[must_use]
    pub fn trend(mut self, trend: Trend) -> Self {
        self.trend = Some(trend);
        self
    }

    /// The color of the detail line and the sparkline; defaults to [`Color::Gray`], muted. A
    /// trend is good or bad only for the figure it moves, so the color is the caller's to pick.
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// The values the tile draws as a sparkline under the value, oldest first. Fewer than two
    /// finite values draw none.
    #[must_use]
    pub fn chart(mut self, values: impl IntoIterator<Item = f64>) -> Self {
        self.chart = values.into_iter().collect();
        self
    }
}

/// How many records a [`RecordTable`] lists unless it says otherwise.
pub const DEFAULT_RECORD_ROWS: usize = 5;

/// The most records a [`RecordTable`] lists: it never pages, so its full list is a link away.
pub const MAX_RECORD_ROWS: usize = 50;

/// A few of a resource's records as a table for a [`Widget`]'s body, under the resource's own
/// columns: the newest posts, the orders awaiting review. [`record_table`](crate::record_table)
/// loads and renders it.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, created_at: jiff::Timestamp }
/// use tablo_core::RecordTable;
///
/// RecordTable::new()
///     .heading("Latest posts")
///     .columns(["title", "created_at"])
///     .order_by(Post::fields().created_at().desc())
///     .limit(5);
/// ```
#[derive(Clone, Debug)]
pub struct RecordTable {
    pub(crate) heading: Option<crate::Text>,
    pub(crate) description: Option<crate::Text>,
    pub(crate) columns: Option<Vec<String>>,
    pub(crate) filter: Option<toasty::stmt::Expr<bool>>,
    pub(crate) order_by: Vec<toasty::stmt::OrderBy>,
    pub(crate) limit: usize,
}

impl RecordTable {
    /// A table of the resource's first [`DEFAULT_RECORD_ROWS`] records in the database's order,
    /// under the columns its list shows by default and its plural label.
    pub fn new() -> Self {
        Self {
            heading: None,
            description: None,
            columns: None,
            filter: None,
            order_by: Vec::new(),
            limit: DEFAULT_RECORD_ROWS,
        }
    }

    /// The heading over the table, in place of the resource's plural label.
    #[must_use]
    pub fn heading(mut self, heading: impl Into<crate::Text>) -> Self {
        self.heading = Some(heading.into());
        self
    }

    /// The line under the heading.
    #[must_use]
    pub fn description(mut self, description: impl Into<crate::Text>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Show only the list's columns these [names](crate::extend::Column::name) name, in this
    /// order, a hidden-by-default one included. A name the list declares no column for fails the
    /// render; one whose field the request may not see is left out.
    #[must_use]
    pub fn columns<S: Into<String>>(mut self, names: impl IntoIterator<Item = S>) -> Self {
        self.columns = Some(names.into_iter().map(Into::into).collect());
        self
    }

    /// Only the records `filter` admits, among those the request may see.
    #[must_use]
    pub fn filter(mut self, filter: toasty::stmt::Expr<bool>) -> Self {
        self.filter = Some(match self.filter.take() {
            Some(previous) => previous.and(filter),
            None => filter,
        });
        self
    }

    /// Order the records by `order`, after any order given before: `Post::fields().created_at()
    /// .desc()` lists the newest first.
    #[must_use]
    pub fn order_by(mut self, order: impl Into<toasty::stmt::OrderBy>) -> Self {
        self.order_by.push(order.into());
        self
    }

    /// List at most `rows` records, at least one and at most [`MAX_RECORD_ROWS`].
    #[must_use]
    pub fn limit(mut self, rows: usize) -> Self {
        self.limit = rows.clamp(1, MAX_RECORD_ROWS);
        self
    }
}

impl Default for RecordTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
