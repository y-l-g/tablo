// SYNC: topcoat-ui-registry@0.10.0 sha256:c96f34a75f49d24388eb854b864033954060896ae008e76a229ab306299185fd — do not hand-edit. Sync via `cargo xtask sync-topcoat-ui` (ADR-0007).
use topcoat::{
    Result,
    view::{Attributes, StaticClass, View, class, component, view},
};

/// Classes for the input's dimensions, border, and interaction states.
const INPUT: StaticClass = class!(
    "h-9 w-full min-w-0 rounded-lg border border-border bg-transparent px-3",
    "text-sm transition-colors outline-none",
    "placeholder:text-muted-foreground",
    "file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-sm file:font-medium",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive",
    "focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
);

/// A styled input.
///
/// Pass input attributes and event handlers through `attrs`. Extra classes are added to
/// the input's classes. It fills its container by default. Set `aria-invalid="true"` to
/// show the error border and focus ring.
///
/// ```ignore
/// view! {
///     input(attrs: attributes! { type="email" placeholder="you@example.com" })
/// }
/// ```
#[component]
pub async fn input(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! { <input class=(class!(INPUT, attrs.remove("class"))) (attrs)> })
}
