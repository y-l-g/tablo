//! Renders each [`Relation`](crate::resource::Relation) of a resource on its records' detail page,
//! as the related resource's table narrowed to the record.

use toasty::stmt::Expr;
use topcoat::{
    context::Cx,
    icon::icon,
    view::{BoxView, ViewExt, view},
};

use super::{
    gate::enforce_tenant,
    list::{declared_chrome, load_scoped_page, table_error_view, wire_table},
    state::current,
};
use crate::{
    form::RecordForm,
    navigation::runtime_link,
    policy::Ability,
    resource::{Mounted, Resource, mounted},
    table::{RETURN_PARAM, TableSignals, create_page_url},
    topcoat_compat::async_page,
};

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
    /// The child's form key for the owner, and the owner's value for it.
    pub(crate) seed: (String, String),
    /// The path of the record's detail page, which renders the table.
    pub(crate) page: String,
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
    resource
        .relations
        .iter()
        .filter_map(|relation| {
            let child = panel.children.get(&relation.child)?;
            let (scope, value) = relation.bind(owner);
            Some((child.render)(
                cx,
                BoundRelation {
                    key: child.slug.clone(),
                    label: relation
                        .label
                        .clone()
                        .unwrap_or_else(|| child.plural_label.clone()),
                    scope,
                    seed: (relation.foreign_key.clone(), value),
                    page: page.clone(),
                },
            ))
        })
        .collect()
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
            page,
        } = relation;
        let table = wire_table(cx, &resource, declared_chrome(cx, &resource));
        let signals = TableSignals::new(cx, Some(&key));
        let state = table.normalize_state(&signals.state(Some(&key)));
        let table = table.returning_to(state.list_url(&page));
        let create_url = (<C::Form as RecordForm>::HAS_FORM && resource.can(cx, Ability::Create))
            .then(|| {
                let query = form_urlencoded::Serializer::new(String::new())
                    .append_pair(&seed.0, &seed.1)
                    .append_pair(RETURN_PARAM, &state.list_url(&page))
                    .finish();
                format!("{}?{query}", create_page_url(&resource.url))
            });
        let body = match load_scoped_page(cx, &resource, &table, &state, scope).await {
            Ok(rows) => table.render_page(cx, rows, &state, &page, &signals).await?,
            Err(error) => table_error_view(cx, &resource, &state, &error, &page),
        };
        let header = relation_header(cx, &resource, label, create_url);
        Ok(view! {
            cx =>
            <section class="flex flex-col gap-3" data-relation=(key)>
                (header)
                (body)
            </section>
        }
        .boxed())
    })
}

/// Renders a relation section's heading row with its create-child link.
fn relation_header<'a, C: Resource>(
    cx: &'a Cx,
    resource: &Mounted<C>,
    label: String,
    create_url: Option<String>,
) -> BoxView<'a> {
    let create_label = format!("New {}", resource.label);
    view! {
        cx =>
        <div class="flex items-center justify-between gap-4">
            <h2 class="text-lg font-semibold tracking-tight text-foreground">
                (label)
            </h2>
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
    }
    .boxed()
}
