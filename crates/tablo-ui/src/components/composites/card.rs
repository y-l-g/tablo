//! Tablo's card: a surface that opens with a ruled head band. Each part carries its own
//! `data-slot` (`card`, `card-header`, `card-title`, `card-description`, `card-content`,
//! `card-footer`), which each theme's skin keys on.

use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

const CARD: StaticClass = class!(
    "flex flex-col rounded-xl border border-border bg-card text-card-foreground shadow-card",
    "has-[>[data-slot=table]]:overflow-hidden",
);
/// The head band: a tint, a rule under it, its title and description stacked.
const HEADER: StaticClass = class!(
    "flex flex-col gap-1 border-b border-rule bg-head px-5 py-3.5 first:rounded-t-[inherit]",
    "last:rounded-b-[inherit] last:border-b-0",
);
const TITLE: StaticClass = class!("tablo-heading text-[0.9375rem] leading-snug text-foreground");
const DESCRIPTION: StaticClass = class!("text-sm text-muted-foreground");
const CONTENT: StaticClass = class!("px-5 py-5");
const FOOTER: StaticClass =
    class!("flex items-center gap-2 border-t border-rule px-5 py-3.5 last:rounded-b-[inherit]");

/// The card's surface.
#[component]
pub async fn card(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(CARD, attrs.remove("class"))) data-slot="card" (attrs)>
            (child)
        </div>
    })
}

/// The head band, holding a [`card_title`] and a [`card_description`].
#[component]
pub async fn card_header(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(HEADER, attrs.remove("class")))
            data-slot="card-header"
            (attrs)
        >
            (child)
        </div>
    })
}

/// The card's heading, an `<h3>`.
#[component]
pub async fn card_title(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <h3 class=(class!(TITLE, attrs.remove("class"))) data-slot="card-title" (attrs)>
            (child)
        </h3>
    })
}

/// A line under the title.
#[component]
pub async fn card_description(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!(DESCRIPTION, attrs.remove("class")))
            data-slot="card-description"
            (attrs)
        >
            (child)
        </p>
    })
}

/// The card's body.
#[component]
pub async fn card_content(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(CONTENT, attrs.remove("class")))
            data-slot="card-content"
            (attrs)
        >
            (child)
        </div>
    })
}

/// A ruled row of actions under the body.
#[component]
pub async fn card_footer(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(FOOTER, attrs.remove("class")))
            data-slot="card-footer"
            (attrs)
        >
            (child)
        </div>
    })
}
