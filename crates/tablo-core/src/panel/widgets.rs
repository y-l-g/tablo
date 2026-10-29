//! The widget grid of a [`Page`], and the row of [`Stat`] tiles and the [`RecordTable`] a widget
//! renders.

use std::{any::TypeId, sync::Arc};

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{
    Page, RecordTable, Resource, Stat, Trend,
    navigation::runtime_link,
    page::MountedPage,
    panel::list::{TableChrome, declared_chrome, wire_table_over},
    resource::mounted,
    table::include_relations,
    topcoat_compat::async_page,
    widget::{MountedWidget, Span, WidgetGrid},
};

/// The widget grid of the page `P`: each [`Widget`](crate::Widget) its
/// [`PageDef::widget`](crate::PageDef::widget) calls added, in their order, that the request may
/// see.
///
/// It renders nothing outside a request to `P`'s panel, for a request the panel's sign-in or
/// [`PageDef::access`](crate::PageDef::access) refuses — the check
/// [`can_open`](crate::can_open) makes — and leaves out each widget whose
/// [`WidgetDef::access`](crate::WidgetDef::access) refuses the request. The default
/// [`Page::render`] places it; the [`Page::render`] docs show a page placing it itself.
pub fn widgets<P: Page>(cx: &Cx) -> BoxView<'_> {
    match super::pages::mounted_admitted(cx, TypeId::of::<P>()) {
        Some(page) => page_grid(cx, page),
        None => ().boxed(),
    }
}

/// The widget grid of `page`, which the request may open: each of its widgets the request may
/// see.
pub(super) fn page_grid<'a>(cx: &'a Cx, page: &MountedPage) -> BoxView<'a> {
    let cells = page
        .widgets
        .admitted(cx)
        .map(|widget| (widget, async_page((widget.render)(cx))))
        .collect();
    grid(cx, page.widgets.columns, cells)
}

/// The grid of `widgets` the request may see, each rendered before the grid returns: a list
/// streams its rows behind a suspense, and a widget rendered lazily beside them would wait for
/// the connection they hold on a pool of one.
pub(crate) async fn rendered_grid<'a>(cx: &'a Cx, widgets: &WidgetGrid) -> Result<BoxView<'a>> {
    let mut cells = Vec::new();
    for widget in widgets.admitted(cx) {
        cells.push((widget, (widget.render)(cx).await?));
    }
    Ok(grid(cx, widgets.columns, cells))
}

/// A grid `columns` wide of each widget with its body.
fn grid<'a>(cx: &'a Cx, columns: u8, cells: Vec<(&MountedWidget, BoxView<'a>)>) -> BoxView<'a> {
    let cells: Vec<BoxView<'a>> = cells
        .into_iter()
        .map(|(widget, body)| {
            let name = widget.name.clone();
            let class = span_class(widget.span, columns);
            view! { cx => <div data-widget=(name) class=(class)>(body)</div> }.boxed()
        })
        .collect();
    if cells.is_empty() {
        return ().boxed();
    }
    view! {
        cx =>
        <div data-widgets="" class=(grid_class(columns))>
            for cell in cells {
                (cell)
            }
        </div>
    }
    .boxed()
}

/// The grid's classes: one column on a narrow screen, at most two on a medium one, `columns` on a
/// wide one.
const fn grid_class(columns: u8) -> &'static str {
    match columns {
        0 | 1 => "grid gap-4",
        2 => "grid gap-4 md:grid-cols-2",
        3 => "grid gap-4 md:grid-cols-2 xl:grid-cols-3",
        _ => "grid gap-4 md:grid-cols-2 xl:grid-cols-4",
    }
}

/// A cell's classes for `span` in a grid `columns` wide; Tailwind needs each class spelled whole.
const fn span_class(span: Span, columns: u8) -> &'static str {
    let span = match span {
        Span::Full => return "min-w-0 col-span-full",
        Span::Columns(span) if span < columns => span,
        Span::Columns(_) => columns,
    };
    match span {
        0 | 1 => "min-w-0",
        2 => "min-w-0 md:col-span-2",
        3 => "min-w-0 md:col-span-2 xl:col-span-3",
        _ => "min-w-0 md:col-span-2 xl:col-span-4",
    }
}

/// A row of [`Stat`] tiles, for a [`Widget`](crate::Widget)'s body: as many to a row as the
/// widget's width holds, up to three, or four when three would leave one tile alone on the last
/// row; nothing when there is no tile.
///
/// ```rust
/// # use tablo_core::Stat;
/// # fn row(cx: &topcoat::context::Cx) -> topcoat::view::BoxView<'_> {
/// tablo_core::stats(
///     cx,
///     [Stat::new("Orders", "1,204"), Stat::new("Refunds", "3")],
/// )
/// # }
/// ```
pub fn stats(cx: &Cx, stats: impl IntoIterator<Item = Stat>) -> BoxView<'_> {
    let stats: Vec<Stat> = stats.into_iter().collect();
    if stats.is_empty() {
        return ().boxed();
    }
    let class = if stats.len() > 3 && stats.len() % 3 == 1 {
        "grid gap-4 @lg:grid-cols-2 @4xl:grid-cols-4"
    } else {
        "grid gap-4 @lg:grid-cols-2 @3xl:grid-cols-3"
    };
    let tiles: Vec<BoxView<'_>> = stats.into_iter().map(|stat| tile(cx, stat)).collect();
    view! {
        cx =>
        <div data-stats="" class="@container">
            <div class=(class)>
                for tile in tiles {
                    (tile)
                }
            </div>
        </div>
    }
    .boxed()
}

fn tile(cx: &Cx, stat: Stat) -> BoxView<'_> {
    let Stat {
        label,
        value,
        detail,
        icon,
        href,
        trend,
        color,
        chart,
    } = stat;
    let detail = detail.map(|detail| detail.localize(cx)).unwrap_or_default();
    let (detail_icon, detail_icon_label) = match trend {
        Some(Trend::Up) => (
            Some(tablo_ui::icons::ARROW_UP),
            crate::t!(cx, "tablo-stat-trend-up"),
        ),
        Some(Trend::Down) => (
            Some(tablo_ui::icons::ARROW_DOWN),
            crate::t!(cx, "tablo-stat-trend-down"),
        ),
        Some(Trend::Flat) => (
            Some(tablo_ui::icons::MINUS),
            crate::t!(cx, "tablo-stat-trend-flat"),
        ),
        None => (None, String::new()),
    };
    let mut attrs = topcoat::view::attributes! { cx => data-stat="" };
    if let Some(href) = &href {
        let mut link = runtime_link(cx, href);
        link.remove("href");
        attrs.extend(link);
    }
    view! {
        cx =>
        tablo_ui::stat(
            label: label.localize(cx),
            value: value,
            detail: detail,
            detail_icon: detail_icon,
            detail_icon_label: detail_icon_label,
            accent: color.text_class(),
            chart: chart,
            icon: icon,
            href: href,
            attrs: attrs
        )
    }
    .boxed()
}

impl Stat {
    /// How many records of `R` the request may see, under `R`'s sidebar label and icon, linking to
    /// its list; `None` when the request may not open that list, so no tile counts records it
    /// cannot see or links to a list that would answer 403.
    ///
    /// The count reads [`scoped_query`](crate::scoped_query): the caller's tenant, `R`'s
    /// [`Query`](Resource::query), and no trashed record.
    ///
    /// # Errors
    ///
    /// The database's, when the count fails.
    pub async fn records<R: Resource>(cx: &Cx) -> Result<Option<Self>> {
        if !super::can_list::<R>(cx) {
            return Ok(None);
        }
        let (Some(item), Some(url)) = (
            super::navigation_item::<R>(cx),
            super::url::resource::<R>(cx),
        ) else {
            return Ok(None);
        };
        let count = crate::db::count(cx, crate::scoped_query::<R>(cx)?).await?;
        let count = crate::display::format_number(crate::display::Number::Int(count.into()), 0);
        let mut stat = Self::new(item.label, count).href(url);
        stat.icon = item.icon;
        Ok(Some(stat))
    }
}

/// `table` over `R`'s records, for a [`Widget`](crate::Widget)'s body: the rows under `R`'s list
/// columns, each linking its View and Edit pages as `R`'s policy allows, with a link to the full
/// list. Two record tables of one resource on a page take distinct headings, which keep their
/// rows' DOM ids apart. It renders nothing when the request may not open that list, the check
/// [`can_list`](super::can_list) makes, or may see none of its columns' fields.
///
/// The rows are the ones the list would show: [`scoped_query`](crate::scoped_query)'s, the
/// caller's tenant, `R`'s [`Query`](Resource::query) and no trashed record, which `table`'s
/// [`filter`](RecordTable::filter) narrows. Its columns hide what
/// [`field_access`](crate::ResourceDef::field_access) hides from the request, on `R` and on the
/// related records they show, as the list does. It never pages, sorts, searches or writes, so it
/// adds no route: a record's own page and the list do.
///
/// ```rust,ignore
/// impl Widget for LatestPosts {
///     fn declare() -> WidgetDef {
///         WidgetDef::new().access(can_list::<PostResource>)
///     }
///
///     async fn render(cx: &Cx) -> Result<impl View> {
///         let table = RecordTable::new()
///             .heading("Latest posts")
///             .order_by(Post::fields().created_at().desc());
///         tablo::record_table::<PostResource>(cx, table).await
///     }
/// }
/// ```
///
/// # Errors
///
/// A [column](RecordTable::column) `R`'s list declares no column for, and the database's,
/// when the load fails.
pub async fn record_table<R: Resource>(
    cx: &Cx,
    table: RecordTable<R::Model>,
) -> Result<BoxView<'_>> {
    let Some(resource) = mounted::<R>(cx).filter(|_| super::can_list::<R>(cx)) else {
        return Ok(().boxed());
    };
    let RecordTable {
        heading,
        description,
        columns,
        filter,
        order_by,
        limit,
        ..
    } = table;
    let shown = resource.table_for(cx);
    let shown = match &columns {
        Some(names) => {
            let declared = resource.table.declared_columns();
            if let Some(name) = names
                .iter()
                .find(|name| !declared.iter().any(|column| column.name() == name.as_str()))
            {
                return Err(crate::error::declaration(format!(
                    "a record table of `{}` names `{name}`, a column its list does not declare",
                    resource.slug
                )));
            }
            Arc::new(shown.only(names))
        }
        None => shown,
    };
    if shown.declared_columns().is_empty() {
        return Ok(().boxed());
    }
    let mut query = resource.scoped_query(cx)?;
    if let Some(filter) = filter {
        query = query.filter(filter);
    }
    for order in order_by {
        query = query.order_by(order);
    }
    let query = include_relations(query.limit(limit), shown.declared_columns());
    let mut db = crate::db::db(cx)?;
    let rows = query
        .exec(&mut db)
        .await
        .map_err(crate::error::unavailable)?;
    shown.restrict_relations(cx, &rows, &mut db).await?;
    let declared = declared_chrome(cx, &resource);
    let chrome = TableChrome {
        delete: false,
        actions: false,
        ..declared
    };
    let back = topcoat::router::request::uri(cx)
        .path_and_query()
        .map_or_else(String::new, ToString::to_string);
    let heading = heading.map_or_else(
        || resource.plural_label.localize(cx),
        |heading| heading.localize(cx),
    );
    let mut wired =
        wire_table_over(cx, &resource, shown, chrome).prefixed(dom_scope(&resource.slug, &heading));
    // An edit begun here lands back here.
    if !back.is_empty() {
        wired = wired.returning_to(back);
    }
    let rows = wired.render_records(cx, rows).await?;
    let description = description.map(|description| description.localize(cx));
    let list = crate::navigation::runtime_link(cx, &resource.url);
    let view_all = crate::t!(cx, "tablo-record-table-view-all");
    let slug = resource.slug.clone();
    Ok(view! {
        cx =>
        <section
            data-record-table=(slug)
            class="relative overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-xs"
        >
            <div class="flex items-start justify-between gap-4 px-5 pt-5 pb-3">
                <div class="flex min-w-0 flex-col gap-1">
                    <h2 class="text-base font-semibold">(heading)</h2>
                    if let Some(description) = description {
                        <p class="text-sm text-muted-foreground">(description)</p>
                    }
                </div>
                <a
                    (list)
                    data-record-table-list=""
                    class="shrink-0 text-sm text-muted-foreground hover:text-foreground"
                >
                    (view_all)
                </a>
            </div>
            (rows)
        </section>
    }
    .boxed())
}

/// The scope of a record table's DOM ids: its resource's slug and its heading, spelled in the
/// characters an id takes.
fn dom_scope(slug: &str, heading: &str) -> String {
    let heading: String = heading
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("{slug}-{heading}")
}
