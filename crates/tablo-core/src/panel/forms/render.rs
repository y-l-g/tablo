//! The shared create/edit form page and the create page.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, ViewExt, attributes, internal::ThenView, view},
};

use super::super::gate::{gate, list_url};
use crate::{form::FieldErrors, resource::Resource};

/// Shared create/edit page shell (GH #73 multipart enctype, CSRF hidden
/// input, inline error slot). Title and submit label are the only deltas.
///
/// `carried` names the upload fields whose value is an uploader's answer rather
/// than the record's: the shell renders each one's path as a hidden
/// `keep_<field>` control, so the submit a corrected form makes can keep a file
/// the browser's empty file input cannot resend.
//
// The public link rides alongside the form state: one more argument rather
// than a second render entry point.
#[allow(clippy::too_many_arguments)]
pub(super) async fn render_form_page<'a, R: Resource>(
    cx: &'a Cx,
    title: String,
    submit_label: &'static str,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    carried: &HashSet<String>,
    public_url: Option<String>,
) -> Result<BoxView<'a>> {
    let schema = R::form(cx);
    let form_html = schema
        .render(cx, crate::schema::Source::form(values, errors))
        .await?;
    let action = topcoat::router::request::uri(cx).path().to_string();
    // Browsers only send `<input type="file">` content as multipart.
    let enctype: Option<String> = schema
        .fields()
        .any(|field| field.is_file())
        .then(|| "multipart/form-data".to_string());
    let csrf = crate::csrf::current_token(cx);
    // The candidate paths, one hidden control each: the framework re-verifies
    // them against the installed store before it uses one.
    let mut carried_fields: Vec<BoxView<'a>> = Vec::new();
    let mut carried_names: Vec<&String> = carried.iter().collect();
    carried_names.sort();
    for name in carried_names {
        let Some(path) = values.get(name) else {
            continue;
        };
        let control = format!("keep_{name}");
        let path = path.clone();
        carried_fields
            .push(view! { cx => <input type="hidden" name=(control) value=(path)> }.boxed());
    }
    Ok(view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((title.clone()))
                if let Some(public) = public_url {
                    <a href=(public) class="text-sm text-muted-foreground underline">
                        "View public post"
                    </a>
                }
            )
            tablo_ui::page_content(
                <form
                    method="post"
                    action=(action)
                    enctype=(enctype)
                    class="flex flex-col gap-4"
                >
                    (crate::csrf::field(cx, &csrf))
                    for carried in carried_fields {
                        (carried)
                    }
                    (form_html)
                    <div class="flex gap-2">
                        tablo_ui::button(
                            variant: tablo_ui::ButtonVariant::Primary,
                            attrs: attributes! { type="submit" },
                            (submit_label)
                        )
                        <a
                            (crate::resource::runtime_link(cx, &list_url(cx, &R::slug())))
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Outline,
                                tablo_ui::ButtonSize::Md,
                            ))
                        >
                            "Cancel"
                        </a>
                    </div>
                </form>
            )
        )
    }
    .boxed())
}

/// Create page GET.
pub(crate) fn resource_create<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::can_create(cx) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let html = render_form_page::<R>(
            cx,
            format!("Create {}", R::label()),
            "Create",
            &HashMap::new(),
            &FieldErrors::new(),
            &HashSet::new(),
            None,
        )
        .await?;
        Ok(html)
    })))
}
#[cfg(test)]
mod tests;
