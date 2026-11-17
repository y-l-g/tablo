//! Tablo's alert: a message set in the page, ruled on its leading edge in its tone. The markup
//! carries `data-slot="alert"` and `data-tone`.

use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// An alert's tone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// Information, in the foreground's tone.
    #[default]
    Neutral,
    /// An error or a refusal.
    Destructive,
}

impl AlertVariant {
    const fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Destructive => "destructive",
        }
    }

    fn classes(self) -> StaticClass {
        match self {
            Self::Neutral => class!(
                "border-border bg-card text-foreground before:bg-foreground/25",
                "[&>svg]:text-muted-foreground",
            ),
            Self::Destructive => class!(
                "border-destructive/30 bg-destructive/[0.06] text-destructive before:bg-destructive",
            ),
        }
    }
}

const BASE: StaticClass = class!(
    "relative grid w-full grid-cols-[0_1fr] items-start gap-y-1 overflow-hidden rounded-lg",
    "border py-3 pr-4 pl-4.5 text-sm has-[>svg]:grid-cols-[1rem_1fr] has-[>svg]:gap-x-3",
    "before:absolute before:inset-y-0 before:left-0 before:w-[3px] before:content-['']",
    "[&>svg]:size-4 [&>svg]:translate-y-0.5",
);

/// The alert. It carries no `role="alert"`, which interrupts a screen reader the moment the
/// element appears: right for a message arriving during the visit, not for one rendered with the
/// page. Pass it among the `attrs` where that is what you want.
#[component]
pub async fn alert(
    cx: &Cx,
    #[default] variant: AlertVariant,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(BASE, variant.classes(), attrs.remove("class")))
            data-slot="alert"
            data-tone=(variant.name())
            (attrs)
        >
            (child)
        </div>
    })
}

/// The alert's first line.
#[component]
pub async fn alert_title(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <p
            class=(class!(
                "col-start-2 font-medium tracking-tight",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </p>
    })
}

/// The lines under the title.
#[component]
pub async fn alert_description(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(
                "col-start-2 text-sm text-muted-foreground",
                attrs.remove("class"),
            ))
            (attrs)
        >
            (child)
        </div>
    })
}
