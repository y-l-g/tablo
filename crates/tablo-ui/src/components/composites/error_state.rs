use topcoat::{
    Result,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const ERROR_STATE: StaticClass = class!(
    "flex flex-col items-center gap-2 rounded-xl border border-destructive/30 \
     bg-card px-6 py-12 text-center text-card-foreground \
     [&>svg]:mb-1 [&>svg]:size-6 [&>svg]:text-destructive"
);
const ERROR_STATE_TITLE: StaticClass = class!("text-sm font-medium text-destructive");
const ERROR_STATE_DETAIL: StaticClass = class!("text-sm text-muted-foreground");
const ERROR_STATE_ACTION: StaticClass = class!("text-sm font-medium text-primary hover:underline");

/// Renders a failed load inside the content region that failed.
///
/// Keeps `detail` generic: never renders error internals (driver messages, SQL, paths) into the
/// page.
///
/// ```text
/// let action = view! { cx => <a href=(list_url)>"Retry"</a> }.boxed();
/// Ok(view! { cx =>
///     error_state(
///         title: "Couldn't load Users",
///         detail: "Something went wrong while loading the records.",
///         action: Some(action.into()),
///     )
/// }.boxed())
/// ```
#[component]
pub async fn error_state(
    /// Short heading naming what failed, e.g. `"Couldn't load Users"`.
    #[into]
    title: String,
    /// Optional muted detail line. Keep it generic — no error internals.
    #[into]
    #[default]
    detail: String,
    /// Optional action under the text (e.g. a retry link). It inherits the
    /// component's action styling; pass a plain `<a>` (or button) without
    /// your own color classes.
    #[default]
    action: Option<Child<'_>>,
    /// Extra attributes for the container. Add `role="alert"` here when the
    /// region swaps in during a visit and the failure should be announced.
    #[default]
    mut attrs: Attributes,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(ERROR_STATE, attrs.remove("class"))) (attrs)>
            icon(data: iconify_icon!("lucide:circle-alert"))
            <p class=(class!(ERROR_STATE_TITLE))>(title)</p>
            if !detail.is_empty() {
                <p class=(class!(ERROR_STATE_DETAIL))>(detail)</p>
            }
            if let Some(action) = action {
                <div class=(class!(ERROR_STATE_ACTION))>(action)</div>
            }
        </div>
    })
}

#[cfg(test)]
mod tests;
