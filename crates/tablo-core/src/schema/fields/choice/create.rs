//! A creatable choice's link and the dialog framing the related resource's create page.
//!
//! The link opens the create page in a new tab, which works without JavaScript; `choice_create.js`
//! opens it in the dialog instead, and makes the record the dialog saves the choice.

use tablo_ui::{
    ButtonSize, ButtonVariant, button, button_variants, dialog, dialog_content, dialog_title,
};
use topcoat::{context::Cx, icon::icon, runtime::Event, view::*};

/// The select `control` with the link creating a related record beside it, and the closed dialog
/// the link opens: `select` is the select's DOM id, `label` the field's, and `url` the related
/// resource's create page, which the dialog frames in pick mode. The dialog's own close button
/// works whatever the frame shows: an error, a sign-in page, or nothing yet.
pub(super) fn with_create_link<'a>(
    cx: &'a Cx,
    control: BoxView<'a>,
    select: &str,
    label: crate::Text,
    url: String,
) -> BoxView<'a> {
    let dialog_id = format!("{select}-create");
    let close_id = dialog_id.clone();
    let select = select.to_string();
    let title = crate::t!(cx, "tablo-field-create-option-title", label = label);
    view! {
        <div class="flex items-start gap-2">
            <div class="flex min-w-0 flex-1 flex-col gap-2">(control)</div>
            <a
                href=(url)
                target="_blank"
                rel="noopener"
                class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
                aria-label=(title.clone())
                aria-haspopup="dialog"
                data-create-option=(dialog_id.clone())
            >
                icon(data: tablo_ui::icons::PLUS)
                (crate::t!(cx, "tablo-field-create-option"))
            </a>
        </div>
        dialog(
            open: false,
            attrs: attributes! {
                id=(dialog_id)
                aria-label=(title.clone())
                data-create-option-dialog=""
                data-create-option-select=(select)
            },
            dialog_content(
                attrs: attributes! {
                    class="[&]:max-h-[calc(100dvh-2rem)] [&]:gap-0 [&]:p-0 \
                           [&]:shadow-xl overflow-hidden"
                },
                // Not a `form method="dialog"`: the dialog sits in the record form, a nested
                // form's end tag closes that one, and the fields after the choice leave it.
                <div class="px-6 pt-6 pe-14">
                    dialog_title(
                        attrs: attributes! { class="text-base" },
                        (title.clone())
                    )
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        attrs: attributes! {
                            type="button"
                            class="absolute end-2.5 top-2.5 text-muted-foreground"
                            aria-label=(crate::t!(cx, "tablo-close"))
                            @click=$(|_e: Event| {
                                raw!(
                                    "document.getElementById(String(${close_id})).close()",
                                    (),
                                );
                            })
                        },
                        icon(data: tablo_ui::icons::X)
                    )
                </div>
                // `choice_create.js` fits the height to the framed page once it has loaded; the
                // panel's maximum height shrinks the frame, and the page scrolls inside it.
                <iframe
                    title=(title.clone())
                    class="h-64 min-h-0 w-full shrink border-0 opacity-0 transition-opacity \
                           data-loaded:opacity-100"
                    data-create-option-frame=""
                ></iframe>
            )
        )
    }
    .boxed()
}
