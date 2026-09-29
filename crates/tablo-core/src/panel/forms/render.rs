//! The shared create/edit form page and the create page.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, ViewExt, attributes, internal::ThenView, view},
};

use super::super::gate::{gate, list_url};
use crate::resource::Resource;

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
    errors: &HashMap<String, Vec<String>>,
    carried: &HashSet<String>,
    public_url: Option<String>,
) -> Result<BoxView<'a>> {
    let schema = R::form(cx);
    let form_html = schema.render_with(cx, values, errors).await?;
    let action = topcoat::router::request::uri(cx).path().to_string();
    // Browsers only send `<input type="file">` content as multipart.
    let enctype: Option<String> = schema
        .has_file_upload()
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
                            href=(list_url(cx, &R::slug()))
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
            format!("Create {}", R::navigation_label()),
            "Create",
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            None,
        )
        .await?;
        Ok(html)
    })))
}
#[cfg(test)]
mod tests {
    #[test]
    fn create_form_multipart_predicate_follows_file_upload() {
        // GH #136 layer rule: core owns the `has_file_upload` predicate
        // (see also `has_file_upload_detects_nested` for nested containers);
        // the showcase (`posts_create_form_is_multipart` /
        // `users_create_form_stays_urlencoded`) owns the HTTP enctype wiring
        // (`render_form_page` maps this predicate to
        // `enctype="multipart/form-data"` one-to-one).
        use crate::schema::{FileUpload, Schema, TextInput};

        #[derive(Debug, toasty::Model, Clone)]
        struct Doc {
            #[key]
            #[auto]
            id: uuid::Uuid,
            path: String,
            title: String,
        }
        let with_file = Schema::new(FileUpload::r#for(Doc::fields().path()));
        let without_file = Schema::new(TextInput::r#for(Doc::fields().title()));
        assert!(
            with_file.has_file_upload(),
            "file schema must report an upload"
        );
        assert!(
            !without_file.has_file_upload(),
            "plain schema must report no upload"
        );
    }
}
