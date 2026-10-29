//! A creatable choice's link and the dialog framing the related resource's create page.
//!
//! The link opens the create page in a new tab, which works without JavaScript; `choice_create.js`
//! opens it in the dialog instead, and makes the record the dialog saves the choice.

use tablo_ui::{ButtonSize, ButtonVariant, button_variants, dialog, dialog_content};
use topcoat::{context::Cx, icon::icon, view::*};

/// The select `control` with the link creating a related record beside it, and the closed dialog
/// the link opens: `select` is the select's DOM id, `label` the field's, and `url` the related
/// resource's create page, which the dialog frames in pick mode.
pub(super) fn with_create_link<'a>(
    cx: &'a Cx,
    control: BoxView<'a>,
    select: &str,
    label: crate::Text,
    url: String,
) -> BoxView<'a> {
    let dialog_id = format!("{select}-create");
    let select = select.to_string();
    let title = crate::t!(cx, "tablo-field-create-option-title", label = label);
    view! {
        cx =>
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
                attrs: attributes! { class="[&]:max-w-3xl p-0 overflow-hidden" },
                <iframe
                    title=(title)
                    class="h-[80vh] w-full border-0"
                    data-create-option-frame=""
                ></iframe>
            )
        )
    }
    .boxed()
}
