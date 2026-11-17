//! Tablo's badge: a short label set apart, `data-slot="badge"` and `data-tone` in its markup.

use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, Class, StaticClass, View, class, component, view},
};

/// How loud a badge is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    /// A tint of the primary color.
    #[default]
    Primary,
    /// A gray tint.
    Secondary,
    /// A border and no fill.
    Outline,
    /// A tint of the destructive color.
    Destructive,
}

impl BadgeVariant {
    const fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Outline => "outline",
            Self::Destructive => "destructive",
        }
    }

    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!("bg-primary/10 text-primary ring-primary/25"),
            Self::Secondary => class!("bg-muted text-muted-foreground ring-foreground/10"),
            Self::Outline => class!("text-foreground ring-input"),
            Self::Destructive => class!("bg-destructive/10 text-destructive ring-destructive/25"),
        }
    }
}

const BASE: StaticClass = class!(
    "inline-flex w-fit shrink-0 items-center justify-center gap-1 rounded-sm px-1.5 py-0.5",
    "text-xs font-medium whitespace-nowrap ring-1 ring-inset [&>svg]:size-3.5 [&>svg]:shrink-0",
);

/// A badge's classes, for an element that reads as one.
#[must_use]
pub fn badge_variants(variant: BadgeVariant) -> Class<(StaticClass, StaticClass)> {
    class!(BASE, variant.classes())
}

/// A `<span>` badge.
#[component]
pub async fn badge(
    cx: &Cx,
    #[default] variant: BadgeVariant,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <span
            class=(class!(BASE, variant.classes(), attrs.remove("class")))
            data-slot="badge"
            data-tone=(variant.name())
            (attrs)
        >
            (child)
        </span>
    })
}
