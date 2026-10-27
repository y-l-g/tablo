use topcoat::{
    Result,
    // Renamed: the tile's `icon` prop takes the name.
    icon::{IconData, icon as icon_view},
    view::{Attributes, StaticClass, View, class, component, view},
};

const STAT: StaticClass = class!(
    "flex flex-col gap-3 rounded-xl border border-border bg-card p-5 text-card-foreground",
    "shadow-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring",
);
/// A linked tile answers the pointer, as the list rows it leads to do.
const STAT_LINK: StaticClass = class!("hover:border-foreground/20 hover:bg-muted/40");
const STAT_LABEL: StaticClass = class!(
    "flex items-center gap-2 text-sm font-medium text-muted-foreground",
    "[&>svg]:size-4 [&>svg]:shrink-0",
);
const STAT_VALUE: StaticClass =
    class!("text-3xl leading-none font-semibold tracking-tight text-foreground tabular-nums");
const STAT_DETAIL: StaticClass =
    class!("flex items-center gap-1 text-sm [&>svg]:size-4 [&>svg]:shrink-0");
const STAT_CHART: StaticClass = class!("-mx-1 h-8 w-[calc(100%+0.5rem)] overflow-visible");

/// The sparkline's drawing box: as wide as 100 units, stretched to the tile's width.
const CHART_WIDTH: f64 = 100.0;
const CHART_HEIGHT: f64 = 24.0;
/// Room above and below the line, so the stroke at the extremes stays inside the box.
const CHART_PAD: f64 = 2.0;

/// One figure for a dashboard: what it counts, the value, and an optional line under it. With an
/// `href` the whole tile is a link, such as to the list the figure counts.
///
/// Lay several out in a grid of your own:
///
/// ```rust
/// # use tablo_ui::{icons, stat};
/// # use topcoat::{context::Cx, view::{BoxView, ViewExt, view}};
/// # fn stats(cx: &Cx) -> BoxView<'_> {
/// view! {
///     cx =>
///     <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
///         stat(
///             label: "Posts",
///             value: "66",
///             detail: "65 drafts",
///             icon: icons::FILE_TEXT,
///             href: "/admin/posts".to_string(),
///         )
///         stat(
///             label: "Comments",
///             value: "4",
///             detail: "2 more than last week",
///             detail_icon: icons::ARROW_UP,
///             accent: "text-success",
///             chart: vec![1.0, 3.0, 2.0, 4.0],
///         )
///     </div>
/// }
/// # .boxed()
/// # }
/// ```
#[component]
pub async fn stat(
    /// What the value counts, e.g. `"Posts"`.
    #[into]
    label: String,
    /// The figure, formatted, e.g. `"1,204"` or `"98%"`.
    #[into]
    value: String,
    /// Optional line under the value, e.g. `"12 this week"`.
    #[into]
    #[default]
    detail: String,
    /// Optional icon before the detail line, such as an arrow for its trend.
    #[into]
    #[default]
    detail_icon: Option<IconData>,
    /// What the detail icon says to a screen reader, e.g. `"Up"`; the icon alone is hidden from
    /// it.
    #[into]
    #[default]
    detail_icon_label: String,
    /// The text-color class of the detail line and the chart, e.g. `"text-success"`; muted when
    /// empty.
    #[into]
    #[default]
    accent: String,
    /// Optional values the tile draws as a sparkline under the value, oldest first. Fewer than two
    /// finite values draw none.
    #[default]
    chart: Vec<f64>,
    /// Optional icon before the label.
    #[into]
    #[default]
    icon: Option<IconData>,
    /// Optional URL the tile links to.
    #[into]
    #[default]
    href: Option<String>,
    /// Extra attributes for the tile.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    let accent = if accent.is_empty() {
        "text-muted-foreground".to_string()
    } else {
        accent
    };
    let line = sparkline_points(&chart);
    let area = line
        .as_ref()
        .map(|line| format!("0,{CHART_HEIGHT} {line} {CHART_WIDTH},{CHART_HEIGHT}"));
    let view_box = format!("0 0 {CHART_WIDTH} {CHART_HEIGHT}");
    let body = view! {
        <p class=(STAT_LABEL)>
            if let Some(glyph) = icon {
                icon_view(
                    data: glyph,
                    attrs: topcoat::view::attributes! { aria-hidden="true" }
                )
            }
            (label)
        </p>
        <p class=(STAT_VALUE)>(value)</p>
        if !detail.is_empty() {
            <p class=(class!(STAT_DETAIL, accent.clone()))>
                if let Some(glyph) = detail_icon {
                    icon_view(
                        data: glyph,
                        attrs: topcoat::view::attributes! { aria-hidden="true" }
                    )
                }
                if !detail_icon_label.is_empty() {
                    <span class="sr-only">
                        (detail_icon_label)
                        " "
                    </span>
                }
                (detail)
            </p>
        }
        if let (Some(line), Some(area)) = (line, area) {
            <svg
                class=(class!(STAT_CHART, accent))
                viewBox=(view_box)
                preserveAspectRatio="none"
                aria-hidden="true"
            >
                <polygon points=(area) fill="currentColor" fill-opacity="0.1" />
                <polyline
                    points=(line)
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                    vector-effect="non-scaling-stroke"
                />
            </svg>
        }
    };
    Ok(view! {
        if let Some(href) = href {
            <a
                href=(href)
                class=(class!(STAT, STAT_LINK, attrs.remove("class")))
                (attrs)
            >
                (body)
            </a>
        } else {
            <div class=(class!(STAT, attrs.remove("class"))) (attrs)>(body)</div>
        }
    })
}

/// The `points` of a sparkline over `values` in the chart's drawing box: evenly spaced from the
/// left edge to the right, the largest value at the top and the smallest at the bottom, a flat
/// series across the middle. Non-finite values are dropped; fewer than two left draw no line.
fn sparkline_points(values: &[f64]) -> Option<String> {
    let values: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if values.len() < 2 {
        return None;
    }
    let (min, max) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), &v| {
            (min.min(v), max.max(v))
        });
    let span = max - min;
    let last = (values.len() - 1) as f64;
    let points: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(index, &value)| {
            let x = index as f64 * CHART_WIDTH / last;
            // A span that overflows to infinity or is zero has no usable scale.
            let share = if span.is_finite() && span > 0.0 {
                (max - value) / span
            } else {
                0.5
            };
            let y = CHART_PAD + share * (CHART_HEIGHT - 2.0 * CHART_PAD);
            format!("{x:.2},{y:.2}")
        })
        .collect();
    Some(points.join(" "))
}

#[cfg(test)]
mod tests;
