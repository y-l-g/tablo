//! Renders each [`Relation`](crate::resource::Relation) of a resource on its records' detail page,
//! as the related resource's table narrowed to the record. A many-to-many relation's attach and
//! detach post to the owner's routes, which `actions::link` serves.

use std::sync::Arc;

use toasty::stmt::Expr;
use topcoat::{
    context::Cx,
    icon::icon,
    router::path_param_segment,
    view::{BoxView, ViewExt, view},
};

use super::{
    bar::ActionBar,
    gate::enforce_tenant,
    list::{declared_chrome, load_scoped_page, table_error_view, wire_table},
    state::current,
};
use crate::{
    form::RecordForm,
    navigation::runtime_link,
    policy::Ability,
    resource::{InputSpec, Links, Mounted, RelationKind, Resource, mounted},
    table::{
        RETURN_PARAM, TableAction, action_options_url, bulk_action_url, create_page_url,
        relation_url, with_return,
    },
    topcoat_compat::async_page,
};

/// The action a many-to-many relation's table links a record with, from its header.
pub(crate) const ATTACH: &str = "attach";

/// The action a many-to-many relation's table unlinks records with, from a row or the bulk bar.
pub(crate) const DETACH: &str = "detach";

/// A registered resource as the child of other resources' relations.
pub(crate) struct Child {
    pub(crate) slug: String,
    pub(crate) plural_label: String,
    /// Renders the child's table narrowed to one owner.
    pub(crate) render: for<'a> fn(&'a Cx, BoundRelation) -> BoxView<'a>,
}

/// A relation resolved against one owner record.
pub(crate) struct BoundRelation {
    /// The child's slug: the prefix of the table's URL parameters.
    pub(crate) key: String,
    /// The section title.
    pub(crate) label: String,
    /// The child's rows that belong to the owner.
    pub(crate) scope: Expr<bool>,
    /// The child's form key for the owner, and the owner's value for it, which a create from the
    /// table seeds; `None` for a many-to-many relation, whose new child would link nothing.
    pub(crate) seed: Option<(String, String)>,
    /// Where a many-to-many relation's table attaches and detaches, when the user may.
    pub(crate) linking: Option<Linking>,
    /// The path of the record's detail page, which renders the table.
    pub(crate) page: String,
}

/// A many-to-many relation's writes, as its table posts them.
pub(crate) struct Linking {
    /// The relation's routes: `{list}/{key}/-/relations/{slug}`.
    pub(crate) url: String,
    /// The attach dialog's input.
    pub(crate) input: InputSpec,
}

/// Renders the relation tables of `resource`'s record `owner`.
pub(crate) fn render_relations<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    owner: &R::Model,
) -> Vec<BoxView<'a>> {
    let Some(panel) = current(cx) else {
        return Vec::new();
    };
    let page = topcoat::router::request::uri(cx).path().to_string();
    // Linking writes the owner's relations, which its edit asks `Update` for; a composite key
    // has no URL to post under.
    let links = resource.table.is_addressable()
        && resource.can(cx, Ability::View(owner))
        && resource.can(cx, Ability::Update(owner));
    resource
        .relations
        .iter()
        .filter_map(|relation| {
            let child = panel.children.get(&relation.child)?;
            let (scope, value) = relation.bind(owner);
            let (seed, linking) = match &relation.kind {
                RelationKind::HasMany { foreign_key } => (Some((foreign_key.clone(), value)), None),
                RelationKind::ManyToMany(Links { input, .. }) => (
                    None,
                    links.then(|| Linking {
                        url: relation_url(
                            &resource.url,
                            &resource.table.key_of(owner),
                            &child.slug,
                        ),
                        input: *input,
                    }),
                ),
            };
            Some((child.render)(
                cx,
                BoundRelation {
                    key: child.slug.clone(),
                    label: relation
                        .label
                        .clone()
                        .unwrap_or_else(|| child.plural_label.clone()),
                    scope,
                    seed,
                    linking,
                    page: page.clone(),
                },
            ))
        })
        .collect()
}

/// The many-to-many relation of `resource` the `{relation}` path segment names, by its related
/// resource's slug.
pub(crate) fn linked_relation<'r, R: Resource>(
    cx: &Cx,
    resource: &'r Mounted<R>,
) -> Option<&'r Links<R::Model>> {
    let panel = current(cx)?;
    let slug = path_param_segment(cx, "relation");
    resource.relations.iter().find_map(|relation| {
        let child = panel.children.get(&relation.child)?;
        (child.slug == slug).then_some(())?;
        relation.links()
    })
}

/// Renders one relation's section as `C`'s list table over the rows the owner holds.
///
/// The table's parameters carry the child's slug, so each relation on the page keeps its own
/// state, and a change reruns the detail page in place. The edit page renders none: a rerun
/// resets the form fields the reader has not saved.
pub(crate) fn relation_table<C: Resource>(cx: &Cx, relation: BoundRelation) -> BoxView<'_> {
    async_page(async move {
        let Some(resource) = mounted::<C>(cx) else {
            return Ok(().boxed());
        };
        if enforce_tenant(cx, &resource).is_err() || !resource.can(cx, Ability::ViewAny) {
            return Ok(().boxed());
        }
        let BoundRelation {
            key,
            label,
            scope,
            seed,
            linking,
            page,
        } = relation;
        let mut chrome = declared_chrome(cx, &resource);
        // A table wires its custom actions under one route; a many-to-many relation's posts
        // Detach to the owner's, so it wires none of the related resource's own.
        chrome.actions = seed.is_some();
        let mut table = wire_table(cx, &resource, chrome).prefixed(&key);
        if let Some(linking) = &linking {
            table =
                table.with_custom_actions(linking.url.clone(), vec![detach_action(cx, &resource)]);
        }
        let (signals, state) = table.browser_state(cx);
        let back = state.list_url(&page);
        let table = table.returning_to(back.clone());
        let create_url = seed
            .filter(|_| <C::Form as RecordForm>::HAS_FORM && resource.can(cx, Ability::Create))
            .map(|(column, value)| {
                let query = form_urlencoded::Serializer::new(String::new())
                    .append_pair(&column, &value)
                    .append_pair(RETURN_PARAM, &back)
                    .finish();
                format!("{}?{query}", create_page_url(&resource.url))
            });
        let attach = linking.map(|linking| {
            ActionBar::input(
                format!("Attach {}", resource.label),
                with_return(&bulk_action_url(&linking.url, ATTACH), &back),
                linking.input,
                action_options_url(&linking.url, ATTACH),
            )
        });
        let body = match load_scoped_page(cx, &resource, &table, &state, scope).await {
            Ok(rows) => table.render_page(cx, rows, &state, &page, &signals).await?,
            Err(error) => table_error_view(
                cx,
                &resource.slug,
                &resource.plural_label,
                &state,
                &error,
                &page,
            ),
        };
        Ok(relation_section(
            cx,
            key,
            label,
            &resource.label,
            create_url,
            attach.map(|bar| bar.render(cx)),
            body,
        ))
    })
}

/// The row and bulk Detach of a many-to-many relation's table over `C`, on every row the user may
/// view.
fn detach_action<C: Resource>(cx: &Cx, resource: &Arc<Mounted<C>>) -> TableAction<C::Model> {
    let (policy_cx, policy) = (cx.clone(), Arc::clone(resource));
    TableAction {
        name: DETACH,
        label: "Detach".to_string(),
        row: true,
        bulk: true,
        confirm: false,
        input: None,
        allowed: Arc::new(move |record: &C::Model| policy.can(&policy_cx, Ability::View(record))),
    }
}

/// Renders a relation section titled `label` around its table `body`, with a link to create a
/// child, labeled after `child_label`, at `create_url`, and the `attach` button.
fn relation_section<'a>(
    cx: &'a Cx,
    key: String,
    label: String,
    child_label: &str,
    create_url: Option<String>,
    attach: Option<BoxView<'a>>,
    body: BoxView<'a>,
) -> BoxView<'a> {
    let create_label = format!("New {child_label}");
    view! {
        cx =>
        <section class="flex flex-col gap-3" data-relation=(key)>
            <div class="flex items-center justify-between gap-4">
                <h2 class="text-lg font-semibold tracking-tight text-foreground">
                    (label)
                </h2>
                <div class="flex items-center gap-2">
                    if let Some(attach) = attach {
                        (attach)
                    }
                    if let Some(url) = create_url {
                        <a
                            (runtime_link(cx, &url))
                            class=(tablo_ui::button_variants(
                                tablo_ui::ButtonVariant::Outline,
                                tablo_ui::ButtonSize::Sm,
                            ))
                        >
                            icon(data: tablo_ui::icons::PLUS)
                            (create_label)
                        </a>
                    }
                </div>
            </div>
            (body)
        </section>
    }
    .boxed()
}
