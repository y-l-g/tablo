//! Tablo's form controls: text fields, boxes, radios and switches on the control surface. Each
//! carries its `data-slot` (`input`, `textarea`, `checkbox`, `radio`, `switch`), which each
//! theme's skin keys on.

use topcoat::{
    Result,
    context::Cx,
    icon::{icon, iconify::iconify_icon},
    view::{Attributes, Child, StaticClass, View, class, component, view},
};

/// What every text field shares: the control surface, its edge, and the focus halo.
const FIELD: StaticClass = class!(
    "w-full min-w-0 rounded-md border border-input bg-control text-sm text-foreground",
    "shadow-control transition-[border-color,box-shadow] outline-none",
    "placeholder:text-muted-foreground/75",
    "hover:border-[color-mix(in_oklab,var(--input),var(--foreground)_22%)]",
    "focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/20",
    "aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive/20",
    "disabled:pointer-events-none disabled:opacity-50 read-only:bg-muted/60",
);

const INPUT: StaticClass = class!(
    "h-9 px-3",
    "file:mr-3 file:h-full file:border-0 file:bg-transparent file:text-sm file:font-medium",
);

const TEXTAREA: StaticClass = class!("field-sizing-content min-h-20 px-3 py-2 leading-relaxed");

/// A one-line `<input>`; its `type` is text unless `attrs` names another.
#[component]
pub async fn input(cx: &Cx, #[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <input
            class=(class!(FIELD, INPUT, attrs.remove("class")))
            data-slot="input"
            (attrs)
        >
    })
}

/// A `<textarea>` that grows with its text.
#[component]
pub async fn textarea(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <textarea
            class=(class!(FIELD, TEXTAREA, attrs.remove("class")))
            data-slot="textarea"
            (attrs)
        >
            (child)
        </textarea>
    })
}

/// A box's edge and the fill it takes once checked.
const BOX: StaticClass = class!(
    "peer size-4 shrink-0 cursor-pointer appearance-none border border-input bg-control",
    "shadow-control transition-colors outline-none checked:border-primary checked:bg-primary",
    "hover:border-[color-mix(in_oklab,var(--input),var(--foreground)_25%)]",
    "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
    "focus-visible:ring-offset-background disabled:pointer-events-none",
);

/// The wrapper the mark overlays: the `<input>` renders no children or pseudo-elements, so the
/// mark is a sibling revealed by the input's `peer` state.
const HOLDER: StaticClass = class!("peer relative inline-flex shrink-0 has-[:disabled]:opacity-50");

/// A checkbox.
#[component]
pub async fn checkbox(cx: &Cx, #[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <span class=(class!(HOLDER, attrs.remove("class"))) data-slot="checkbox">
            <input
                type="checkbox"
                class=(class!(BOX, "rounded-[min(var(--radius-sm),5px)]"))
                (attrs)
            >
            icon(
                data: iconify_icon!("lucide:check"),
                attrs: topcoat::view::attributes! {
                    class="pointer-events-none absolute inset-0 m-auto size-3 stroke-[3] \
                        text-primary-foreground opacity-0 peer-checked:opacity-100"
                }
            )
        </span>
    })
}

/// The group a set of [`radio_group_item`]s sits in.
#[component]
pub async fn radio_group(
    cx: &Cx,
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            role="radiogroup"
            class=(class!("grid gap-3", attrs.remove("class")))
            (attrs)
        >
            (child)
        </div>
    })
}

/// One radio; checked, its ring takes the primary color around a dot of it.
#[component]
pub async fn radio_group_item(cx: &Cx, #[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <span class=(class!(HOLDER, attrs.remove("class"))) data-slot="radio">
            <input
                type="radio"
                class=(class!(BOX, "rounded-full checked:bg-control"))
                (attrs)
            >
            <span
                class="pointer-events-none absolute inset-0 m-auto size-2 rounded-full bg-primary \
                    opacity-0 transition-opacity peer-checked:opacity-100"
            ></span>
        </span>
    })
}

/// A checkbox drawn as a track and a thumb, for a setting that takes effect as it is.
#[component]
pub async fn switch(cx: &Cx, #[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <span class=(class!(HOLDER, attrs.remove("class"))) data-slot="switch">
            <input
                type="checkbox"
                role="switch"
                class="peer h-5 w-9 shrink-0 cursor-pointer appearance-none rounded-full \
                    bg-foreground/15 shadow-[inset_0_1px_2px_oklch(0_0_0/0.08)] \
                    transition-colors outline-none checked:bg-primary \
                    focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 \
                    focus-visible:ring-offset-background disabled:pointer-events-none"
                (attrs)
            >
            <span
                class="pointer-events-none absolute top-1/2 left-0.5 size-4 -translate-y-1/2 \
                    rounded-full bg-white shadow-[0_1px_2px_oklch(0_0_0/0.2)] \
                    transition-transform duration-150 peer-checked:translate-x-4"
                data-thumb=""
            ></span>
        </span>
    })
}
