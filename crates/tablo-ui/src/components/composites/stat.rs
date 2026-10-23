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
const STAT_DETAIL: StaticClass = class!("text-sm text-muted-foreground");

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
///         stat(label: "Comments", value: "4")
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
    /// Optional muted line under the value, e.g. `"12 this week"`.
    #[into]
    #[default]
    detail: String,
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
            <p class=(STAT_DETAIL)>(detail)</p>
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
