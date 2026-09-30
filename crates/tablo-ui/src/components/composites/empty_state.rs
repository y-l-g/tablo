use topcoat::{
    Result,
    icon::icon,
    view::{Attributes, Child, StaticClass, View, class, component, view},
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

/// Zero-data rendering for a content region (CONTEXT.md:`EmptyState`).
///
/// A muted icon, a title, an optional detail line, and optional actions (a
/// "Clear search" link, a Create button) in the region that holds no data.
/// It draws no border of its own, so it sits inside whatever surface holds
/// the region — a table cell, a card. [`error_state`](super::error_state::error_state)
/// is its failed-load counterpart.
///
/// ```ignore
/// empty_state(title: "No records yet", detail: "Create the first one.")
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
    Ok(view! {
        <div class=(class!(EMPTY_STATE, attrs.remove("class"))) (attrs)>
            <div class=(EMPTY_STATE_ICON) aria-hidden="true">
                icon(data: crate::icons::INBOX)
            </div>
            <p class=(EMPTY_STATE_TITLE)>(title)</p>
            if !detail.is_empty() {
                <p class=(EMPTY_STATE_DETAIL)>(detail)</p>
            }
            if let Some(action) = action {
                <div class=(EMPTY_STATE_ACTION)>(action)</div>
            }
        </div>
    })
}

#[cfg(test)]
mod tests;
