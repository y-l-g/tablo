//! Provides the toast surface and stack.

use topcoat::{
    Result,
    icon::icon,
    view::{Attributes, Child, StaticClass, View, attributes, class, component, view},
};

use crate::icons;

/// Positions the toast stack bottom-right.
const TOASTER: StaticClass = class!(
    "pointer-events-none fixed right-4 bottom-4 z-50 flex w-[356px] \
     max-w-[calc(100vw-2rem)] flex-col gap-3.5 sm:right-6 sm:bottom-6",
);

/// Styles the toast surface.
///
/// The toast slides in, then fades out 4s later; hovering or focusing it holds it, and the
/// countdown restarts once it is left. Its close control hides it at once.
const TOAST: StaticClass = class!(
    "pointer-events-auto relative flex w-full items-center gap-1.5 rounded-lg \
     border border-border bg-background p-4 text-[13px] text-foreground shadow-lg \
     [overflow-wrap:anywhere] \
     invisible opacity-0 starting:visible starting:translate-y-full starting:opacity-100 \
     [transition:translate_400ms_cubic-bezier(0.25,0.1,0.25,1),opacity_400ms_ease_4s,visibility_0s_linear_4.4s] \
     hover:visible hover:opacity-100 hover:[transition-delay:0s] \
     focus-within:visible focus-within:opacity-100 focus-within:[transition-delay:0s] \
     has-[[data-dismiss]:checked]:hidden \
     focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none",
);

/// Styles the icon slot.
const ICON: StaticClass = class!("flex size-4 shrink-0 items-center justify-center");

/// Styles the content column.
const CONTENT: StaticClass = class!("flex min-w-0 flex-1 flex-col gap-0.5");

/// Styles the toast title.
const TITLE: StaticClass = class!("font-medium leading-normal");

/// Styles the toast description.
const DESCRIPTION: StaticClass = class!("leading-snug text-muted-foreground");

/// Styles the close button.
const CLOSE: StaticClass = class!(
    "absolute top-0 left-0 flex size-5 -translate-x-[35%] -translate-y-[35%] \
     cursor-pointer items-center justify-center rounded-full border border-border \
     bg-background text-foreground transition-colors hover:bg-muted",
);

/// Renders the toast stack.
///
/// ```rust
/// # use tablo_ui::{icons, toast, toast_close, toast_content, toast_description, toast_icon, toast_title, toaster};
/// # use topcoat::{context::Cx, icon::icon, view::{BoxView, ViewExt, attributes, view}};
/// # fn created(cx: &Cx) -> BoxView<'_> {
/// # view! { cx =>
/// toaster(
///     toast(attrs: attributes! { data-type="success" },
///         toast_icon(icon(data: icons::CIRCLE_CHECK))
///         toast_content(
///             toast_title("Created")
///             toast_description("The record is live.")
///         )
///         toast_close()
///     )
/// )
/// # }
/// # .boxed()
/// # }
/// ```
#[component]
pub async fn toaster(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <section
            aria-label="Notifications"
            tabindex="-1"
            aria-live="polite"
            aria-relevant="additions text"
            aria-atomic="false"
        >
            <ol
                class=(class!(TOASTER, attrs.remove("class")))
                data-sonner-toaster=""
                data-y-position="bottom"
                data-x-position="right"
                (attrs)
            >
                (child)
            </ol>
        </section>
    })
}

/// Renders a toast surface.
#[component]
pub async fn toast(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <li
            tabindex="0"
            class=(class!(TOAST, attrs.remove("class")))
            data-sonner-toast=""
            data-styled="true"
            data-visible="true"
            data-y-position="bottom"
            data-x-position="right"
            data-front="true"
            (attrs)
        >
            (child)
        </li>
    })
}

/// Renders the status icon slot.
#[component]
pub async fn toast_icon(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(ICON, attrs.remove("class")))
            data-icon=""
            aria-hidden="true"
            (attrs)
        >
            (child)
        </div>
    })
}

/// Renders the content column.
#[component]
pub async fn toast_content(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(CONTENT, attrs.remove("class"))) data-content="" (attrs)>
            (child)
        </div>
    })
}

/// Renders the toast title.
#[component]
pub async fn toast_title(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div class=(class!(TITLE, attrs.remove("class"))) data-title="" (attrs)>
            (child)
        </div>
    })
}

/// Renders the toast description.
#[component]
pub async fn toast_description(
    #[default] mut attrs: Attributes,
    #[default] child: Child<'_>,
) -> Result<impl View> {
    Ok(view! {
        <div
            class=(class!(DESCRIPTION, attrs.remove("class")))
            data-description=""
            (attrs)
        >
            (child)
        </div>
    })
}

/// Renders the close control: a checkbox whose checked state hides its toast.
#[component]
pub async fn toast_close(#[default] mut attrs: Attributes) -> Result<impl View> {
    Ok(view! {
        <label
            class=(class!(
                CLOSE,
                "has-focus-visible:ring-2 has-focus-visible:ring-ring/50",
                attrs.remove("class"),
            ))
            data-close-button=""
            (attrs)
        >
            <input
                type="checkbox"
                class="sr-only"
                aria-label="Close toast"
                data-dismiss=""
            >
            icon(data: icons::X, attrs: attributes! { class="size-3" })
        </label>
    })
}
