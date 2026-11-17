//! Tablo's button: a variant for its weight, a size for its height. The markup carries
//! `data-slot="button"`, `data-tone` and `data-size`, which each theme's skin keys on.

use topcoat::{
    Result,
    context::Cx,
    view::{Attributes, Child, Class, StaticClass, View, class, component, view},
};

/// How much a button weighs on its page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The page's one main action, in the primary color.
    #[default]
    Primary,
    /// A filled, quiet action.
    Secondary,
    /// A bordered action on the control surface: the page's other actions.
    Outline,
    /// No surface until hovered: toolbar and row actions.
    Ghost,
    /// An action that deletes or cannot be undone.
    Destructive,
}

impl ButtonVariant {
    const fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Destructive => "destructive",
        }
    }

    fn classes(self) -> StaticClass {
        match self {
            Self::Primary => class!(
                "border-primary bg-primary text-primary-foreground shadow-button",
                "hover:bg-[color-mix(in_oklab,var(--primary),var(--foreground)_12%)]",
                "hover:border-[color-mix(in_oklab,var(--primary),var(--foreground)_12%)]",
            ),
            Self::Secondary => class!(
                "border-transparent bg-foreground/[0.06] text-foreground",
                "hover:bg-foreground/10 active:bg-foreground/[0.14]",
            ),
            Self::Outline => class!(
                "border-input bg-control text-foreground shadow-control",
                "hover:border-[color-mix(in_oklab,var(--input),var(--foreground)_20%)] hover:bg-muted",
            ),
            Self::Ghost => class!(
                "border-transparent text-foreground hover:bg-foreground/[0.06]",
                "active:bg-foreground/10",
            ),
            Self::Destructive => class!(
                "border-destructive bg-destructive text-destructive-foreground shadow-button",
                "hover:bg-[color-mix(in_oklab,var(--destructive),var(--foreground)_12%)]",
            ),
        }
    }
}

/// How tall a button is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    /// 2rem: toolbars and dense rows.
    Sm,
    /// 2.25rem, the height of a field.
    #[default]
    Md,
    /// 2.5rem: a page outside the shell, such as sign-in.
    Lg,
    /// A square of the default height, for a lone icon.
    Icon,
}

impl ButtonSize {
    const fn name(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Icon => "icon",
        }
    }

    fn classes(self) -> StaticClass {
        match self {
            Self::Sm => class!("h-8 gap-1.5 px-3 text-[0.8125rem] [&>svg]:size-3.5"),
            Self::Md => class!("h-9 gap-2 px-3.5 text-sm"),
            Self::Lg => class!("h-10 gap-2 px-5 text-sm"),
            Self::Icon => class!("size-9"),
        }
    }
}

const BASE: StaticClass = class!(
    "inline-flex shrink-0 cursor-pointer items-center justify-center rounded-md border",
    "font-medium whitespace-nowrap outline-none select-none",
    "transition-[color,background-color,border-color,box-shadow,translate] duration-150",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50",
    "aria-disabled:pointer-events-none aria-disabled:opacity-50",
    "[&>svg]:size-4 [&>svg]:shrink-0",
);

/// The classes of a button, for a link or a summary that reads as one. Give the element
/// `data-slot="button"` and the variant's `data-tone` too, so each theme's skin reaches it.
#[must_use]
pub fn button_variants(
    variant: ButtonVariant,
    size: ButtonSize,
) -> Class<(StaticClass, StaticClass, StaticClass)> {
    class!(BASE, variant.classes(), size.classes())
}

/// A `<button>`; its `type` is the browser's default unless `attrs` names one.
#[component]
pub async fn button(
    cx: &Cx,
    #[default] variant: ButtonVariant,
    #[default] size: ButtonSize,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    let class = class!(
        BASE,
        variant.classes(),
        size.classes(),
        attrs.remove("class")
    );
    Ok(view! {
        <button
            class=(class)
            data-slot="button"
            data-tone=(variant.name())
            data-size=(size.name())
            (attrs)
        >
            (child)
        </button>
    })
}
