//! A registered resource's trail: where its pages sit in the sidebar and the breadcrumbs, and how
//! a crumb reads one of its records.

use std::any::TypeId;

use crate::{
    Ability, Resource,
    form::RecordForm,
    panel::state::{Owned, RecordFacts, ResourceTrail},
    resource::{Mounted, RelationKind, Trashed},
};

/// The trail of `R`, mounted as `mounted` at `url`. Each relation's tab is left for the linking
/// pass to fill, once every resource has registered.
pub(super) fn trail<R: Resource>(mounted: &Mounted<R>, url: String) -> ResourceTrail {
    ResourceTrail {
        resource: TypeId::of::<R>(),
        list: url,
        label: mounted.plural_label.clone(),
        listed: !mounted.navigation_hidden,
        viewed: mounted.viewed(),
        edited: mounted.pages.edit,
        pages: mounted
            .record_pages
            .iter()
            .map(|page| (page.slug.clone(), page.label.clone()))
            .collect(),
        owns: mounted
            .relations
            .iter()
            .map(|relation| Owned {
                child: relation.child,
                foreign_key: match &relation.kind {
                    RelationKind::HasMany { foreign_key }
                    | RelationKind::HasOne { foreign_key } => Some(foreign_key.clone()),
                    RelationKind::ManyToMany(_) => None,
                },
                tab: None,
            })
            .collect(),
        record: record_facts::<R>,
    }
}

/// The title and form values of `R`'s record `key`, for the trail: loaded as its detail page loads
/// it, so a record the request may not view names nothing.
fn record_facts<'a, R: Resource>(
    cx: &'a topcoat::context::Cx,
    key: &'a str,
) -> crate::BoxFuture<'a, Option<RecordFacts>> {
    Box::pin(async move {
        let resource = crate::resource::mounted::<R>(cx)?;
        let mut db = crate::db::db(cx).ok()?;
        let record =
            crate::panel::actions::find_trashed_by_key(cx, &resource, key, Trashed::With, &mut db)
                .await
                .ok()?;
        resource
            .can(cx, Ability::View(&record))
            .then(|| RecordFacts {
                title: resource.record_title(cx, &record, key),
                values: R::Form::hydrate(cx, &record),
            })
    })
}
