// SYNC: topcoat-ui-registry@0.11.0 sha256:76f6d29cb5f824676250ad0fd002e95654f68ec12a35f3497749d621ced7fb84 — do not hand-edit. Sync via `cargo xtask sync-topcoat-ui` (ADR-0007).
use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// Classes for a textarea that grows with its content. Browsers without content sizing
/// support keep the minimum height and scroll.
const TEXTAREA: StaticClass = class!(
    "field-sizing-content min-h-16 w-full rounded-lg border border-border",
    "bg-transparent px-3 py-2 text-sm transition-colors outline-none",
    "placeholder:text-muted-foreground",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive",
    "focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// A text input for multiple lines.
///
/// Pass the initial value as children. `attrs` are forwarded to the `<textarea>`, with
/// extra classes added to its classes. It fills its container and grows with its
/// content where the browser supports this. Set `aria-invalid="true"` to show the error
/// border and focus ring.
///
/// ```ignore
/// view! {
///     textarea(attrs: attributes! { name="feedback" placeholder="Tell us more" })
/// }
/// ```
#[component]
pub async fn textarea(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <textarea class=(class!(TEXTAREA, attrs.remove("class"))) (attrs)>
            (child)
        </textarea>
    })
}
