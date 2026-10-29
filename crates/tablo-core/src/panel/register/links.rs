//! Linking each registered resource's relations to the resources they name, once every resource
//! has registered.

use std::any::TypeId;

use super::Registry;
use crate::{
    DeclarationError, DeclarationErrorKind, Resource, Site, Text, panel::url::RECORD_ROUTE_PARAM,
    resource::RESERVED_RECORD_SEGMENTS,
};

/// One relation of a registered resource, its child type erased.
pub(super) struct Link {
    child: TypeId,
    child_name: &'static str,
    misdeclared: Option<DeclarationErrorKind>,
    /// The child's foreign key, for a `has_one` relation.
    has_one: Option<String>,
    /// Whether [`Relation::tab`](crate::Relation::tab) serves the table on a page of its own.
    tab: bool,
    /// The relation's [`label`](crate::Relation::label), else the child's plural label.
    label: Option<Text>,
}

impl Link {
    /// `relation` of `R`, whose many-to-many join model resolves through `schema` when there is
    /// one.
    pub(super) fn of<R: Resource>(
        relation: &crate::Relation<R::Model>,
        schema: Option<&crate::toasty_compat::model::AppSchema>,
    ) -> Self {
        let unlinkable = relation.links().zip(schema).and_then(|(links, schema)| {
            crate::toasty_compat::join::JoinTable::of::<R::Model>(schema, &links.field)
                .err()
                .map(|fault| DeclarationErrorKind::ManyToMany {
                    field: links.field.clone(),
                    fault,
                })
        });
        Self {
            child: relation.child,
            child_name: relation.child_name,
            misdeclared: relation.misdeclared.clone().or(unlinkable),
            has_one: relation.has_one_key().map(str::to_string),
            tab: relation.tab,
            label: relation.label.clone(),
        }
    }
}

impl Registry {
    /// Resolves every relation against the registered resources: each must name one this panel
    /// registers, its table's row actions and create link go to that resource's routes, and name
    /// it once per owner, since the child's slug prefixes the table's URL parameters. A tab
    /// serves at `{owner list}/{id}/{child slug}`, apart from the owner's record pages.
    pub(in crate::panel) fn link_relations(&mut self) {
        let mut tabs = Vec::new();
        for parent in &self.resources {
            let mut seen = Vec::new();
            for link in &parent.relations {
                let child = self
                    .resources
                    .iter()
                    .find(|registered| registered.resource == link.child);
                let site = Site::Relation(
                    child.map_or_else(|| link.child_name.to_string(), |child| child.slug.clone()),
                );
                let mut refuse = |kind| {
                    self.errors.push(DeclarationError {
                        resource: Some(parent.name),
                        site: site.clone(),
                        kind,
                    });
                };
                if let Some(kind) = &link.misdeclared {
                    refuse(kind.clone());
                }
                let Some(child) = child else {
                    refuse(DeclarationErrorKind::UnregisteredRelation);
                    continue;
                };
                if seen.contains(&link.child) {
                    refuse(DeclarationErrorKind::DuplicateRelation);
                    continue;
                }
                seen.push(link.child);
                if let Some(foreign_key) = &link.has_one {
                    for fault in parent
                        .form
                        .related_faults(&child.form, &child.slug, foreign_key)
                    {
                        refuse(DeclarationErrorKind::RelatedForm(fault));
                    }
                }
                if !link.tab {
                    continue;
                }
                let slug = child.slug.clone();
                if link.has_one.is_some() {
                    refuse(DeclarationErrorKind::HasOneTab);
                } else if RESERVED_RECORD_SEGMENTS.contains(&slug.as_str()) {
                    refuse(DeclarationErrorKind::ReservedSlug { slug });
                } else if parent.record_page_slugs.contains(&slug) {
                    refuse(DeclarationErrorKind::DuplicateSlug { slug });
                } else {
                    let list = format!("{}/{}", self.prefix, parent.slug);
                    let label = link.label.clone().or_else(|| {
                        self.children
                            .get(&link.child)
                            .map(|child| child.plural_label.clone())
                    });
                    tabs.push((list, slug, label, parent.relation_tab));
                }
            }
        }
        for (list, slug, label, handler) in tabs {
            self.page(
                http::Method::GET,
                &format!("{list}/{RECORD_ROUTE_PARAM}/{slug}"),
                handler,
            );
            let trail = self
                .nav_items
                .iter_mut()
                .filter_map(|entry| entry.resource_list.as_mut())
                .find(|trail| trail.list == list);
            if let (Some(trail), Some(label)) = (trail, label) {
                trail.pages.push((slug, label));
            }
        }
    }
}
