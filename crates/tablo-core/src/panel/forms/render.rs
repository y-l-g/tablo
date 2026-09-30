//! The shared create/edit form page and the create page.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, ViewExt, attributes, internal::ThenView, view},
};

use super::super::{
    gate::{gate, list_url, return_target},
    relations::render_relations,
};
use crate::{form::FieldErrors, resource::Resource};

/// What a form page shows around its form: the create page and the edit page
/// differ only here.
pub(super) struct FormChrome<'a> {
    title: String,
    submit_label: &'static str,
    /// The record's public page, linked from the header.
    public_url: Option<String>,
    /// The record's relation tables, below the form.
    relations: Vec<BoxView<'a>>,
}

impl<'a> FormChrome<'a> {
    /// The create page: no record yet, so no public link and no relations.
    pub(super) fn create<R: Resource>() -> Self {
        Self {
            title: format!("Create {}", R::label()),
            submit_label: "Create",
            public_url: None,
            relations: Vec::new(),
        }
    }

    /// The edit page of `record`.
    pub(super) fn edit<R: Resource>(cx: &'a Cx, record: &R::Model) -> Self {
        Self {
            title: format!("Edit {}", R::label()),
            submit_label: "Save",
            public_url: R::public_url(cx, record),
            relations: render_relations::<R>(cx, record),
        }
    }
}

/// Shared create/edit page shell (GH #73 multipart enctype, CSRF hidden
/// input, inline error slot), with the page's [`FormChrome`] around it.
///
/// `carried` names the upload fields whose value is an uploader's answer rather
/// than the record's: the shell renders each one's path as a hidden
/// `keep_<field>` control, so the submit a corrected form makes can keep a file
/// the browser's empty file input cannot resend.
pub(super) async fn render_form_page<'a, R: Resource>(
    cx: &'a Cx,
    chrome: FormChrome<'a>,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    carried: &HashSet<String>,
) -> Result<BoxView<'a>> {
    let FormChrome {
        title,
        submit_label,
        public_url,
        relations,
    } = chrome;
    let schema = R::form(cx);
    let form_html = schema
        .render(cx, crate::schema::Source::form(values, errors))
        .await?;
    // The form posts where it was served, keeping a validated `?return=` so
    // the write lands where the page was opened from, and Cancel goes there.
    let path = topcoat::router::request::uri(cx).path();
    let return_to = return_target(cx);
    let action = match &return_to {
        Some(target) => crate::resource::with_return(path, target),
        None => path.to_string(),
    };
    let cancel = return_to.unwrap_or_else(|| list_url(cx, &R::slug()));
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
                            (crate::resource::runtime_link(cx, &cancel))
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Outline,
                                tablo_ui::ButtonSize::Md,
                            ))
                        >
                            "Cancel"
                        </a>
                    </div>
                </form>
                for relation in relations {
                    (relation)
                }
            )
        )
    }
    .boxed())
}

/// Create page GET.
///
/// A query parameter that names a declared control seeds that control
/// (`?post_id=…`): a relation's create link opens the child's form with the
/// owner already chosen. It is a prefill only — the POST parses and checks
/// what is submitted, like any other value the user could have typed.
pub(crate) fn resource_create<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::can_create(cx) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let html = render_form_page::<R>(
            cx,
            FormChrome::create::<R>(),
            &seeded_values::<R>(cx),
            &FieldErrors::new(),
            &HashSet::new(),
        )
        .await?;
        Ok(html)
    })))
}
/// The create form's initial values: the request's query parameters that
/// name a control of `R`'s form, first occurrence wins.
fn seeded_values<R: Resource>(cx: &Cx) -> HashMap<String, String> {
    let controls = R::form(cx).controls();
    let query = topcoat::router::request::uri(cx)
        .query()
        .unwrap_or_default();
    let mut values = HashMap::new();
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        if controls.iter().any(|control| control.name == name) {
            values
                .entry(name.into_owned())
                .or_insert_with(|| value.into_owned());
        }
    }
    values
}

#[cfg(test)]
mod tests;
