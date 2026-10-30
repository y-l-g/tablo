//! The shared create/edit form page and the create page.

use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, error::forbidden},
    view::{BoxView, ViewExt, attributes, view},
};

use super::{
    super::{
        bar::{ActionBar, record_bar},
        gate::{gate, return_target},
    },
    related::{open_related, render_related},
};
pub(super) use crate::form::VERSION_FIELD;
use crate::{
    form::{FieldErrors, FormValues},
    panel::record_nav::{RecordNavLink, record_navigation, render_record_navigation},
    resource::{Mounted, Places, PublicLink, Resource},
    schema::{FormMask, Schema},
    t,
    topcoat_compat::async_page,
};

/// What a form page shows around its form: the create page and the edit page differ only here.
pub(crate) struct FormChrome<'a> {
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
    /// Where a searchable choice fetches its options: an action's input route, or `None` for the
    /// resource's own.
    options: Option<String>,
    /// Whether the form edits a stored record.
    editing: bool,
    /// Whether a second submit button saves and lands on a fresh create form.
    another: bool,
    /// The relationship seeds of the create page's query, which the form posts back to and a
    /// create-another save lands on.
    seed: Vec<(String, String)>,
    /// The record's relation tables, under the form.
    relations: Vec<BoxView<'a>>,
    /// The record's navigation, under the header.
    navigation: Vec<RecordNavLink>,
    /// The sections of the record's related forms, inside the form after its own fields.
    related: Vec<BoxView<'a>>,
    /// Whether the page refuses a save because another changed the record since the form opened.
    stale: bool,
    /// The message a record fn declined the write with, shown above the fields.
    declined: Option<String>,
    /// The keys the request's field access hides or locks.
    mask: FormMask,
    /// Whether a creatable choice's dialog frames the create page: Cancel closes the dialog, the
    /// form does not save another, and it posts back in pick mode.
    picking: bool,
}

impl<'a> FormChrome<'a> {
    pub(super) fn create<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> Self {
        let picking = crate::panel::url::picking(cx);
        Self {
            title: t!(
                cx,
                "tablo-form-create-title",
                label = resource.label.clone()
            ),
            submit_label: t!(cx, "tablo-form-create"),
            destructive: false,
            public_link: None,
            actions: ActionBar::default(),
            cancel: resource.url.clone(),
            hidden: Vec::new(),
            options: None,
            editing: false,
            another: !picking,
            seed: seed_pairs(cx, resource),
            relations: Vec::new(),
            navigation: Vec::new(),
            related: Vec::new(),
            stale: false,
            declined: None,
            mask: resource.form_mask(cx),
            picking,
        }
    }

    /// The edit page's: its header carries the record's actions placed on [`Places::EDIT`], each
    /// landing back on the page, and its Delete, and its form the record's version.
    pub(super) fn edit<R: Resource>(cx: &Cx, resource: &Mounted<R>, record: &R::Model) -> Self {
        use crate::ModelColumns as _;
        let hidden = record
            .version()
            .map(|version| (VERSION_FIELD.to_string(), version.to_string()))
            .into_iter()
            .collect();
        Self {
            title: t!(cx, "tablo-form-edit-title", label = resource.label.clone()),
            submit_label: t!(cx, "tablo-form-save"),
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
            hidden,
            options: None,
            editing: true,
            another: false,
            seed: Vec::new(),
            relations: Vec::new(),
            navigation: record_navigation(cx, resource, record),
            related: Vec::new(),
            stale: false,
            declined: None,
            mask: resource.form_mask(cx),
            picking: false,
        }
    }

    /// Marks the page as refusing a save another save overtook: the form shows the alert, and
    /// carries the version it was rendered from, so saving again overwrites.
    pub(super) fn stale(mut self) -> Self {
        self.stale = true;
        self
    }

    /// Shows `message`, the reason a record fn declined the write, above the fields.
    pub(super) fn declined(mut self, message: String) -> Self {
        self.declined = Some(message);
        self
    }

    /// Renders `relations` under the form.
    pub(super) fn with_relations(mut self, relations: Vec<BoxView<'a>>) -> Self {
        self.relations = relations;
        self
    }

    /// Renders `related`, the sections of the record's related forms, inside the form.
    pub(super) fn with_related(mut self, related: Vec<BoxView<'a>>) -> Self {
        self.related = related;
        self
    }

    /// A settings page's, such as the profile: titled `title`, saved by `Save`, cancelled to
    /// `home`.
    pub(crate) fn settings(cx: &Cx, title: String, home: String) -> Self {
        Self {
            title,
            submit_label: t!(cx, "tablo-form-save"),
            destructive: false,
            public_link: None,
            actions: ActionBar::default(),
            cancel: home,
            hidden: Vec::new(),
            options: None,
            editing: true,
            another: false,
            seed: Vec::new(),
            relations: Vec::new(),
            navigation: Vec::new(),
            related: Vec::new(),
            stale: false,
            declined: None,
            mask: FormMask::default(),
            picking: false,
        }
    }

    /// The chrome with the hidden `(name, value)` control the submit carries besides the schema's.
    pub(crate) fn with_hidden(mut self, name: &str, value: String) -> Self {
        self.hidden.push((name.to_string(), value));
        self
    }

    /// An action's input page: titled `title`, submitted by the action's `label`, carrying
    /// `hidden` back to the action's POST, cancelled to `cancel`, and searching its choices'
    /// options at `options`.
    pub(crate) fn action(
        title: String,
        label: String,
        destructive: bool,
        hidden: Vec<(String, String)>,
        cancel: String,
        options: String,
    ) -> Self {
        Self {
            title,
            submit_label: label,
            destructive,
            public_link: None,
            actions: ActionBar::default(),
            cancel,
            hidden,
            options: Some(options),
            editing: false,
            another: false,
            seed: Vec::new(),
            relations: Vec::new(),
            navigation: Vec::new(),
            related: Vec::new(),
            stale: false,
            declined: None,
            mask: FormMask::default(),
            picking: false,
        }
    }
}

/// Each upload a refused submission carries to the next one: its field's name and its path.
pub(crate) type Carried = std::collections::BTreeSet<(String, String)>;

/// Renders the shared form shell with `FormChrome`, carrying uploader-answered uploads as hidden
/// `keep_<field>` controls.
pub(crate) async fn render_form_page<'a>(
    cx: &'a Cx,
    schema: &Schema,
    chrome: FormChrome<'a>,
    values: &FormValues,
    errors: &FieldErrors,
    carried: &Carried,
) -> Result<BoxView<'a>> {
    let FormChrome {
        title,
        submit_label,
        destructive,
        public_link,
        actions,
        cancel,
        hidden,
        options,
        editing,
        another,
        seed,
        relations,
        related,
        stale,
        declined,
        mask,
        picking,
        navigation,
    } = chrome;
    // A hidden field's value never reaches the page, nor its error, which could tell it: an
    // alert says only that the save cannot pass.
    let unseen = errors.iter().any(|error| mask.hides(&error.key));
    let values = &mask.shown(values);
    let mut source = crate::schema::Source::form(values, errors)
        .editing(editing)
        .masked(mask);
    if let Some(url) = options {
        source = source.options_at(url);
    }
    let form_html = schema.render(cx, source).await?;
    let action = form_action(cx, &seed, picking);
    let cancel = return_target(cx).unwrap_or(cancel);
    // A refused save keeps what the user typed, which leaving the page would lose.
    let dirty = !errors.is_empty() || stale || declined.is_some();
    let has_actions = public_link.is_some() || !actions.is_empty();
    let actions = actions.render(cx);
    let enctype: Option<String> = schema
        .fields()
        .any(|field| field.is_file())
        .then(|| "multipart/form-data".to_string());
    let csrf = crate::csrf::current_token(cx);
    // Carried paths are signed for their field, and re-verified against the store before use.
    let mut carried_fields: Vec<BoxView<'a>> = Vec::new();
    for (name, path) in carried {
        let control = format!("keep_{name}");
        let path = crate::upload::carry::seal(cx, name, path);
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
                    tablo_ui::page_description((t!(cx, "tablo-confirm-irreversible")))
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
            (render_record_navigation(cx, navigation))
            tablo_ui::page_content(
                <form
                    method="post"
                    action=(action)
                    enctype=(enctype)
                    class="flex flex-col gap-4"
                    data-form-guard=(t!(cx, "tablo-form-unsaved"))
                    data-form-dirty=(dirty)
                >
                    (crate::csrf::field(cx, &csrf))
                    if stale {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: attributes! { role="alert" data-stale="" },
                            tablo_ui::alert_title((t!(cx, "tablo-form-stale")))
                            tablo_ui::alert_description(
                                (t!(cx, "tablo-form-stale-description"))
                            )
                        )
                    }
                    if let Some(message) = declined {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: attributes! { role="alert" data-declined="" },
                            tablo_ui::alert_description((message))
                        )
                    }
                    if unseen {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: attributes! { role="alert" data-unseen-error="" },
                            tablo_ui::alert_title((t!(cx, "tablo-form-unseen")))
                            tablo_ui::alert_description(
                                (t!(cx, "tablo-form-unseen-description"))
                            )
                        )
                    }
                    for carried in carried_fields {
                        (carried)
                    }
                    (form_html)
                    for related in related {
                        (related)
                    }
                    <div class="flex items-center gap-2">
                        tablo_ui::button(
                            variant: submit_variant,
                            attrs: attributes! { type="submit" },
                            (submit_label)
                        )
                        if another {
                            tablo_ui::button(
                                variant: tablo_ui::ButtonVariant::Outline,
                                attrs: attributes! {
                                    type="submit"
                                    name=(crate::form::ANOTHER_FIELD)
                                    value="1"
                                },
                                (t!(cx, "tablo-form-create-another"))
                            )
                        }
                        if picking {
                            tablo_ui::button(
                                variant: tablo_ui::ButtonVariant::Outline,
                                attrs: attributes! { type="button" data-pick-cancel="" },
                                (t!(cx, "tablo-cancel"))
                            )
                        } else {
                            <a
                                (crate::navigation::runtime_link(cx, &cancel))
                                class=(tablo_ui::button_variants(
                                    tablo_ui::ButtonVariant::Outline,
                                    tablo_ui::ButtonSize::Md,
                                ))
                            >
                                (t!(cx, "tablo-cancel"))
                            </a>
                        }
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

/// Where the form posts, and where a create-another save lands: the current path with `seed`,
/// the create page's relationship seeds, pick mode when `picking`, and a validated `?return=`
/// target.
pub(super) fn form_action(cx: &Cx, seed: &[(String, String)], picking: bool) -> String {
    let path = topcoat::router::request::uri(cx).path();
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.extend_pairs(seed);
    if picking {
        query.append_pair(crate::panel::url::PICK_PARAM, "1");
    }
    if let Some(target) = return_target(cx) {
        query.append_pair(crate::panel::url::RETURN_PARAM, &target);
    }
    let query = query.finish();
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

/// Renders the create page, seeding only relationship controls from query parameters.
pub(crate) fn resource_create<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can_create(cx) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let related = open_related(cx, &resource, None, &mut crate::db::db(cx)?).await?;
        let html = render_form_page(
            cx,
            &resource.form,
            FormChrome::create(cx, &resource).with_related(render_related(cx, &related)),
            &seeded_values(cx, &resource),
            &FieldErrors::new(),
            &Carried::new(),
        )
        .await?;
        Ok(html)
    })
}

/// The create form's values: each field's default, under what the query parameters post for a
/// relationship control, read as the form posts it: a multiple choice's every value, a single
/// one's last.
fn seeded_values<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> FormValues {
    let schema = &resource.form;
    let mut values = schema.defaults();
    // Only relationship choices post here, and only a repeater's key refuses a read.
    values.extend(
        schema
            .read(cx, &seed_pairs(cx, resource))
            .unwrap_or_default(),
    );
    values
}

/// The query parameters that seed a relationship control of the create form: one a relation
/// table's create link sets to its owner. A control that is or may be disabled stores its
/// default, which the form must show, and a masked one takes what the user may not choose, so
/// neither is seedable.
pub(super) fn seed_pairs<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> Vec<(String, String)> {
    let mask = resource.form_mask(cx);
    let seedable: Vec<&str> = resource
        .form
        .fields()
        .filter(|field| {
            !field.may_post_nothing_on_create()
                && mask.writes(field.name()).is_none()
                && field
                    .as_choice()
                    .is_some_and(|choice| choice.is_relationship())
        })
        .map(|field| field.name())
        .collect();
    query_pairs(cx, &seedable)
}

/// The current query's `(name, value)` pairs whose name is one of `names`, in order.
fn query_pairs(cx: &Cx, names: &[&str]) -> Vec<(String, String)> {
    let query = topcoat::router::request::uri(cx)
        .query()
        .unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .filter(|(name, _)| names.contains(&name.as_ref()))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}
