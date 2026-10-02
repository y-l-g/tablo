//! The shared create/edit form page and the create page.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, ViewExt, attributes, internal::ThenView, view},
};

use super::super::{
    gate::{gate, list_url, return_target},
    relations::render_relations,
};
use crate::{
    form::FieldErrors,
    policy::{Ability, can},
    resource::{Resource, declared},
};

/// What a form page shows around its form: the create page and the edit page differ only here.
pub(super) struct FormChrome<'a> {
    title: String,
    submit_label: &'static str,
    public_url: Option<String>,
    relations: Vec<BoxView<'a>>,
}

impl<'a> FormChrome<'a> {
    pub(super) fn create<R: Resource>() -> Self {
        Self {
            title: format!("Create {}", R::label()),
            submit_label: "Create",
            public_url: None,
            relations: Vec::new(),
        }
    }

    pub(super) fn edit<R: Resource>(cx: &'a Cx, record: &R::Model) -> Self {
        Self {
            title: format!("Edit {}", R::label()),
            submit_label: "Save",
            public_url: R::public_url(cx, record),
            relations: render_relations::<R>(cx, record, false),
        }
    }
}

/// Renders the shared create/edit shell with `FormChrome`, carrying uploader-answered uploads as hidden `keep_<field>` controls.
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
    let declared = declared::<R>(cx);
    let schema = &declared.form;
    let form_html = schema
        .render(cx, crate::schema::Source::form(values, errors))
        .await?;
    // Posts to the current path, preserving a validated `?return=` target.
    let path = topcoat::router::request::uri(cx).path();
    let return_to = return_target(cx);
    let action = match &return_to {
        Some(target) => crate::resource::with_return(path, target),
        None => path.to_string(),
    };
    let cancel = return_to.unwrap_or_else(|| list_url(cx, &R::slug()));
    let enctype: Option<String> = schema
        .fields()
        .any(|field| field.is_file())
        .then(|| "multipart/form-data".to_string());
    let csrf = crate::csrf::current_token(cx);
    // Carried paths are re-verified against the store before use.
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
                    tablo_ui::page_actions(
                        <a
                            href=(public)
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Outline,
                                tablo_ui::ButtonSize::Md,
                            ))
                        >
                            icon(data: tablo_ui::icons::EXTERNAL_LINK)
                            "View public post"
                        </a>
                    )
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
                    <div class="flex items-center gap-2">
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

/// Renders the create page, seeding only relationship controls from query parameters.
pub(crate) fn resource_create<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !can::<R>(cx, Ability::Create) {
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

/// Collects relationship-control query parameters for the create form, first occurrence wins.
fn seeded_values<R: Resource>(cx: &Cx) -> HashMap<String, String> {
    let declared = declared::<R>(cx);
    let schema = &declared.form;
    let seedable: Vec<&str> = schema
        .fields()
        .filter(|field| {
            field
                .as_choice()
                .is_some_and(|choice| choice.is_relationship())
        })
        .map(|field| field.name())
        .collect();
    let query = topcoat::router::request::uri(cx)
        .query()
        .unwrap_or_default();
    let mut values = HashMap::new();
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        if seedable.contains(&name.as_ref()) {
            values
                .entry(name.into_owned())
                .or_insert_with(|| value.into_owned());
        }
    }
    values
}

#[cfg(test)]
mod tests;
