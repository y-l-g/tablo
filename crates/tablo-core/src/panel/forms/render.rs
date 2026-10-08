//! The shared create/edit form page and the create page.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, error::forbidden},
    view::{BoxView, ViewExt, attributes, view},
};

use super::super::{
    bar::{ActionBar, record_bar},
    gate::{gate, return_target},
};
use crate::{
    form::FieldErrors,
    policy::Ability,
    resource::{Mounted, Places, PublicLink, Resource},
    schema::Schema,
    topcoat_compat::async_page,
};

/// What a form page shows around its form: the create page and the edit page differ only here.
pub(crate) struct FormChrome {
    title: String,
    submit_label: String,
    /// Whether the submit button renders destructive: an action that confirms, whose page says
    /// it cannot be undone.
    destructive: bool,
    public_link: Option<PublicLink>,
    /// The header's action buttons: the edit page's record actions and Delete.
    actions: ActionBar,
    /// Where Cancel leads without a `?return=` target: the resource's list, or the page of a
    /// header action.
    cancel: String,
    /// Hidden `(name, value)` controls the submit carries besides the schema's.
    hidden: Vec<(String, String)>,
}

impl FormChrome {
    pub(super) fn create<R: Resource>(resource: &Mounted<R>) -> Self {
        Self {
            title: format!("Create {}", resource.label),
            submit_label: "Create".to_string(),
            destructive: false,
            public_link: None,
            actions: ActionBar::default(),
            cancel: resource.url.clone(),
            hidden: Vec::new(),
        }
    }

    /// The edit page's: its header carries the record's actions placed on [`Places::EDIT`], each
    /// landing back on the page, and its Delete.
    pub(super) fn edit<R: Resource>(cx: &Cx, resource: &Mounted<R>, record: &R::Model) -> Self {
        Self {
            title: format!("Edit {}", resource.label),
            submit_label: "Save".to_string(),
            destructive: false,
            public_link: resource.public_link(cx, record),
            // An action lands back on the page as it was left, its own `?return=` included.
            actions: {
                let uri = topcoat::router::request::uri(cx);
                let here = uri
                    .path_and_query()
                    .map_or(uri.path(), |full| full.as_str());
                record_bar(cx, resource, record, Places::EDIT, here)
            },
            cancel: resource.url.clone(),
            hidden: Vec::new(),
        }
    }

    /// An action's input page: titled `title`, submitted by the action's `label`, carrying
    /// `hidden` back to the action's POST, and cancelled to `cancel`.
    pub(crate) fn action(
        title: String,
        label: String,
        destructive: bool,
        hidden: Vec<(String, String)>,
        cancel: String,
    ) -> Self {
        Self {
            title,
            submit_label: label,
            destructive,
            public_link: None,
            actions: ActionBar::default(),
            cancel,
            hidden,
        }
    }
}

/// Renders the shared form shell with `FormChrome`, carrying uploader-answered uploads as hidden
/// `keep_<field>` controls.
pub(crate) async fn render_form_page<'a>(
    cx: &'a Cx,
    schema: &Schema,
    chrome: FormChrome,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    carried: &HashSet<String>,
) -> Result<BoxView<'a>> {
    let FormChrome {
        title,
        submit_label,
        destructive,
        public_link,
        actions,
        cancel,
        hidden,
    } = chrome;
    let form_html = schema
        .render(cx, crate::schema::Source::form(values, errors))
        .await?;
    // Posts to the current path, preserving a validated `?return=` target.
    let path = topcoat::router::request::uri(cx).path();
    let return_to = return_target(cx);
    let action = match &return_to {
        Some(target) => crate::table::with_return(path, target),
        None => path.to_string(),
    };
    let cancel = return_to.unwrap_or(cancel);
    let has_actions = public_link.is_some() || !actions.is_empty();
    let actions = actions.render(cx);
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
    for (name, value) in hidden {
        carried_fields
            .push(view! { cx => <input type="hidden" name=(name) value=(value)> }.boxed());
    }
    let submit_variant = if destructive {
        tablo_ui::ButtonVariant::Destructive
    } else {
        tablo_ui::ButtonVariant::Primary
    };
    Ok(view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((title.clone()))
                if destructive {
                    tablo_ui::page_description("This action cannot be undone.")
                }
                if has_actions {
                    tablo_ui::page_actions(
                        if let Some(link) = public_link {
                            <a
                                href=(link.url)
                                class=(tablo_ui::button_variants(
                                    tablo_ui::ButtonVariant::Outline,
                                    tablo_ui::ButtonSize::Md,
                                ))
                            >
                                icon(data: tablo_ui::icons::EXTERNAL_LINK)
                                (link.label)
                            </a>
                        }
                        (actions)
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
                            variant: submit_variant,
                            attrs: attributes! { type="submit" },
                            (submit_label)
                        )
                        <a
                            (crate::navigation::runtime_link(cx, &cancel))
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

/// Renders the create page, seeding only relationship controls from query parameters.
pub(crate) fn resource_create<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::Create) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let html = render_form_page(
            cx,
            &resource.form,
            FormChrome::create(&resource),
            &seeded_values(cx, &resource),
            &FieldErrors::new(),
            &HashSet::new(),
        )
        .await?;
        Ok(html)
    })
}

/// Collects relationship-control query parameters for the create form, first occurrence wins.
fn seeded_values<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> HashMap<String, String> {
    let schema = &resource.form;
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
