use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const PAGE: StaticClass = class!("mx-auto flex w-full max-w-7xl flex-col gap-6 p-4 md:p-8");
/// The title and the actions share the first line; the description always
/// takes a line of its own below them, whatever order the children come in.
const PAGE_HEADER: StaticClass = class!(
    "flex flex-wrap items-center gap-x-4 gap-y-1.5 \
     [&>[data-page-description]]:order-last [&>[data-page-description]]:basis-full"
);
const PAGE_TITLE: StaticClass =
    class!("min-w-0 flex-1 truncate text-2xl font-semibold tracking-tight text-foreground");
const PAGE_DESCRIPTION: StaticClass = class!("text-sm text-muted-foreground");
const PAGE_ACTIONS: StaticClass = class!("flex shrink-0 flex-wrap items-center gap-2");
const PAGE_CONTENT: StaticClass = class!("flex flex-col gap-6");

/// Standard container for an admin page, owning its max width, padding, and vertical rhythm.
///
/// ```text
/// page(
///     page_header(
///         page_title("Users")
///         page_description("Manage your users.")
///         page_actions(create_button)
///     )
///     page_content(table_view)
/// )
/// ```
#[component]
pub async fn page(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <div class=(class!(PAGE, attrs.remove("class"))) (attrs)>(child)</div> })
}

/// The page's heading row: a [`page_title`], an optional [`page_description`]
/// under it, and optional [`page_actions`] at the end of the title's line.
#[component]
pub async fn page_header(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(PAGE_HEADER, attrs.remove("class"))) (attrs)>(child)</div>
    })
}

#[component]
pub async fn page_title(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <h1 class=(class!(PAGE_TITLE, attrs.remove("class"))) (attrs)>(child)</h1>
    })
}

#[component]
pub async fn page_description(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!(PAGE_DESCRIPTION, attrs.remove("class")))
            data-page-description=""
            (attrs)
        >
            (child)
        </p>
    })
}

/// The page-level controls (Create, Edit, Back...) at the end of the title's
/// line in a [`page_header`].
#[component]
pub async fn page_actions(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(PAGE_ACTIONS, attrs.remove("class"))) (attrs)>(child)</div>
    })
}

#[component]
pub async fn page_content(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(PAGE_CONTENT, attrs.remove("class"))) (attrs)>(child)</div>
    })
}
