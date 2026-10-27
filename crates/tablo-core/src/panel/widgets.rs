//! The widget grid of a [`Page`], and the row of [`Stat`] tiles a widget renders.

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{
    Page, Resource, Stat, Trend,
    widget::{MountedWidget, Span},
};

/// The widget grid of the page `P`: each [`Widget`](crate::Widget) its
/// [`PageDef::widget`](crate::PageDef::widget) calls added, in their order, that the request may
/// see.
///
/// It renders nothing outside a request to `P`'s panel or for a request
/// [`PageDef::access`](crate::PageDef::access) refuses, and leaves out each widget whose
/// [`WidgetDef::access`](crate::WidgetDef::access) refuses the request. The
/// [`Widget`](crate::Widget) docs show a page placing it.
pub fn widgets<P: Page>(cx: &Cx) -> BoxView<'_> {
    let Some(page) = super::pages::mounted_page::<P>(cx).filter(|page| page.admits(cx)) else {
        return ().boxed();
    };
    grid(cx, &page.widgets, page.widget_columns)
}

/// The grid of `widgets` the request may see, `columns` wide on a wide screen.
fn grid<'a>(cx: &'a Cx, widgets: &'a [MountedWidget], columns: u8) -> BoxView<'a> {
    let cells: Vec<BoxView<'a>> = widgets
        .iter()
        .filter(|widget| widget.admits(cx))
        .map(|widget| {
            let name = widget.name.clone();
            let body = (widget.render)(cx);
            view! {
                cx =>
                <div data-widget=(name) class=(span_class(widget.span, columns))>
                    (body)
                </div>
            }
            .boxed()
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
        Span::Full => return "col-span-full",
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

/// A row of [`Stat`] tiles, for a [`Widget`](crate::Widget)'s body: three to a row on a wide
/// screen, or four when three would leave one tile alone on the last row.
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
    let class = if stats.len() > 3 && stats.len() % 3 == 1 {
        "grid gap-4 sm:grid-cols-2 xl:grid-cols-4"
    } else {
        "grid gap-4 sm:grid-cols-2 xl:grid-cols-3"
    };
    let tiles: Vec<BoxView<'_>> = stats.into_iter().map(|stat| tile(cx, stat)).collect();
    view! {
        cx =>
        <div data-stats="" class=(class)>
            for tile in tiles {
                (tile)
            }
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
    let detail_icon = trend.map(|trend| match trend {
        Trend::Up => tablo_ui::icons::ARROW_UP,
        Trend::Down => tablo_ui::icons::ARROW_DOWN,
        Trend::Flat => tablo_ui::icons::MINUS,
    });
    view! {
        cx =>
        tablo_ui::stat(
            label: label.localize(cx),
            value: value,
            detail: detail,
            detail_icon: detail_icon,
            accent: color.text_class(),
            chart: chart,
            icon: icon,
            href: href,
            attrs: topcoat::view::attributes! { data-stat="" }
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
        let count = crate::scoped_query::<R>(cx)?
            .count()
            .exec(&mut crate::db::db(cx)?)
            .await?;
        let mut stat = Self::new(item.label, count.to_string()).href(url);
        stat.icon = item.icon;
        Ok(Some(stat))
    }
}
