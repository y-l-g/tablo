//! A record's values shown read-only: a detail page's entries, and a blank value's dash.

use tablo_ui::{
    field as ui_field, field_content as ui_field_content, field_title as ui_field_title,
};
use topcoat::{context::Cx, view::*};

use crate::Text;

/// A read-only value: `label` over `value`, which wraps as text. A detail page's entry, and an
/// embedded leaf on one.
pub(crate) fn read_only<'a>(cx: &'a Cx, label: &Text, value: BoxView<'a>) -> BoxView<'a> {
    let label = label.clone();
    view! {
        cx =>
        ui_field(
            attrs: attributes! { class="ac-field" },
            ui_field_content(
                ui_field_title(
                    attrs: attributes! { class="text-muted-foreground" },
                    (label)
                )
                <div class="text-sm text-foreground wrap-anywhere whitespace-pre-wrap">
                    (value)
                </div>
            )
        )
    }
    .boxed()
}

/// A record's value as read-only text: a blank value reads as a muted dash.
pub(crate) fn value_cell<'a>(cx: &'a Cx, text: &str) -> BoxView<'a> {
    if text.trim().is_empty() {
        return view! { cx => <span class="text-muted-foreground">"—"</span> }.boxed();
    }
    let text = text.to_string();
    view! { cx => (text) }.boxed()
}
