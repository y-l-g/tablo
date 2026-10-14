use topcoat::{
    Result,
    icon::icon,
    view::{Attributes, Child, StaticClass, View, class, component},
};

const EMPTY_STATE: StaticClass = class!("flex flex-col items-center gap-2 px-6 py-12 text-center");
const EMPTY_STATE_ICON: StaticClass = class!(
    "mb-1 flex size-10 items-center justify-center rounded-full bg-muted \
     text-muted-foreground [&>svg]:size-5"
);
const EMPTY_STATE_TITLE: StaticClass = class!("text-sm font-medium text-foreground");
const EMPTY_STATE_DETAIL: StaticClass = class!("text-sm text-muted-foreground");
const EMPTY_STATE_ACTION: StaticClass =
    class!("mt-2 flex flex-wrap items-center justify-center gap-2");

/// Renders zero-data content inside the region that holds no data.
///
/// ```rust
/// # use tablo_ui::empty_state;
/// # use topcoat::{context::Cx, view::{BoxView, ViewExt, view}};
/// # fn empty(cx: &Cx) -> BoxView<'_> {
/// view! { cx => empty_state(title: "No records yet", detail: "Create the first one.") }
/// # .boxed()
/// # }
/// ```
#[component]
pub async fn empty_state(
    /// What is empty, e.g. `"No records yet"`.
    #[into]
    title: String,
    /// Optional muted detail line.
    #[into]
    #[default]
    detail: String,
    /// Optional controls under the text.
    #[default]
    action: Option<Child<'_>>,
    /// Extra attributes for the container.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    super::state_shell::state_shell!(
        __cx,
        title,
        detail,
        action,
        attrs,
        EMPTY_STATE,
        EMPTY_STATE_TITLE,
        EMPTY_STATE_DETAIL,
        EMPTY_STATE_ACTION,
        crate::icons::INBOX,
        Some(EMPTY_STATE_ICON)
    )
}
