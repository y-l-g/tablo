use topcoat::{
    Result,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const PANEL: StaticClass =
    class!("flex flex-col gap-5 rounded-xl border border-border py-6 text-card-foreground");

/// Border-only page-level panel.
///
/// The `card` primitive's shape and header/content rhythm without opaque
/// paint: no `bg-card`, no `shadow-sm`. The container draws only its border,
/// so a page card and a form `Section`/`Repeater` share one look.
///
/// Pass sections as children. Use `card_header`/`card_title` and
/// `card_content` inside, as with `card`. `attrs` are forwarded to the
/// `<div>`, with extra classes added to its classes.
///
/// ```ignore
/// view! {
///     cx =>
///     panel(
///         card_header(card_title("Upload"))
///         card_content("Body")
///     )
/// }
/// ```
#[component]
pub async fn panel(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! { <div class=(class!(PANEL, attrs.remove("class"))) (attrs)>(child)</div> })
}

#[cfg(test)]
mod tests;
