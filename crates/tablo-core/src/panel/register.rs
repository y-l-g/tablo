//! Registering a panel's resources and pages as it mounts: each resource's def is built once and
//! bound to the app schema, and every slug, route and sidebar entry follows from it.

use std::{any::TypeId, collections::HashMap, sync::Arc};

use topcoat::router::{PageFn, RouteFn};

use super::{
    Root,
    actions::{
        resource_bulk_action, resource_bulk_delete, resource_delete, resource_export,
        resource_options, resource_row_action,
    },
    build::{ResourceCheck, check_resource, route_path, validate_route_segment},
    detail::resource_view,
    forms::{resource_create, resource_create_post, resource_edit, resource_edit_post},
    list::resource_list,
    pages::page_handler,
    relations::{Child, relation_table},
};
use crate::{
    DeclarationError, DeclarationErrorKind, Page, Site,
    form::RecordForm,
    navigation::NavigationItem,
    resource::{Mounted, Mounts, Resource, ResourceDef},
    schema::FieldResolver,
    table::{
        ACTION_ROUTE_PARAM, ACTIONS_ROUTE_SEGMENT, BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT,
        DASH_ROUTE_SEGMENT, DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT, RECORD_ROUTE_PARAM,
    },
};

/// A resource or page waiting for its panel to mount.
pub(super) trait Registration: Send {
    fn register(self: Box<Self>, registry: &mut Registry);
}

/// Adjusts a resource's def for one panel.
type Customize<R> = Box<dyn FnOnce(ResourceDef<R>) -> ResourceDef<R> + Send>;

/// [`Panel::resource_with`](super::Panel::resource_with)'s pending resource.
pub(super) struct ResourceRegistration<R: Resource>(pub(super) Customize<R>);

/// [`Panel::page`](super::Panel::page)'s pending page, or with `home` set,
/// [`Panel::home`](super::Panel::home)'s.
pub(super) struct PageRegistration<P> {
    pub(super) home: bool,
    pub(super) _page: std::marker::PhantomData<fn() -> P>,
}

/// Everything a panel's registrations add up to.
pub(super) struct Registry {
    pub(super) prefix: String,
    schema: Option<Arc<toasty_core::Schema>>,
    /// Every slug a resource or page mounts at: one namespace.
    slugs: Vec<String>,
    /// The URL each resource and page serves at, by type.
    pub(super) urls: HashMap<TypeId, String>,
    pub(super) nav_items: Vec<NavigationItem>,
    pub(super) pages: Vec<PageFn>,
    pub(super) routes: Vec<RouteFn>,
    pub(super) root: Option<Root>,
    pub(super) mounts: Mounts,
    pub(super) resources: Vec<Registered>,
    /// Each resource's relation table, by resource type.
    pub(super) children: HashMap<TypeId, Child>,
    pub(super) errors: Vec<DeclarationError>,
}

/// One registered resource, its type erased.
pub(super) struct Registered {
    resource: TypeId,
    name: &'static str,
    slug: String,
    /// The declaration checks mount runs with the app's values.
    pub(super) check: ResourceCheck,
    relations: Vec<Link>,
}

/// One relation of a registered resource, its child type erased.
struct Link {
    child: TypeId,
    child_name: &'static str,
    misdeclared: Option<DeclarationErrorKind>,
}

impl Registry {
    pub(super) fn new(prefix: String, schema: Option<Arc<toasty_core::Schema>>) -> Self {
        Self {
            prefix,
            schema,
            slugs: Vec::new(),
            urls: HashMap::new(),
            nav_items: Vec::new(),
            pages: Vec::new(),
            routes: Vec::new(),
            root: None,
            mounts: Mounts::default(),
            resources: Vec::new(),
            children: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Claims `{prefix}/{slug}` for `T`, recording a refusal and returning `None` when the slug is
    /// unavailable.
    fn claim_slug<T: 'static>(&mut self, item: &'static str, slug: &str) -> Option<String> {
        let refused = if let Err(error) = validate_route_segment(item, slug) {
            Some(error)
        } else if RESERVED_SLUGS.contains(&slug) {
            Some(DeclarationErrorKind::ReservedSlug {
                slug: slug.to_string(),
            })
        } else if self.slugs.iter().any(|taken| taken == slug) {
            Some(DeclarationErrorKind::DuplicateSlug {
                slug: slug.to_string(),
            })
        } else {
            None
        };
        if let Some(error) = refused {
            self.errors
                .push(DeclarationError::of::<T>(Site::Registration, error));
            return None;
        }
        let url = format!("{}/{slug}", self.prefix);
        self.slugs.push(slug.to_string());
        self.urls.insert(TypeId::of::<T>(), url.clone());
        Some(url)
    }

    fn page(&mut self, method: http::Method, url: &str, handler: topcoat::router::PageRenderFn) {
        self.pages
            .push(PageFn::new(method, route_path(url), handler));
    }

    fn route(&mut self, method: http::Method, url: &str, handler: topcoat::router::RouteHandlerFn) {
        self.routes
            .push(RouteFn::new(method, route_path(url), handler));
    }

    /// Resolves every relation against the registered resources: each must name one this panel
    /// registers, its table's row actions and create link go to that resource's routes, and name
    /// it once per owner, since the child's slug prefixes the table's URL parameters.
    pub(super) fn link_relations(&mut self) {
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
                if child.is_none() {
                    refuse(DeclarationErrorKind::UnregisteredRelation);
                    continue;
                }
                if seen.contains(&link.child) {
                    refuse(DeclarationErrorKind::DuplicateRelation);
                    continue;
                }
                seen.push(link.child);
            }
        }
    }
}

/// The segments the panel routes under its prefix itself, which no resource or page may take as its
/// slug.
const RESERVED_SLUGS: &[&str] = &["login", "logout"];

impl<R: Resource> Registration for ResourceRegistration<R> {
    fn register(self: Box<Self>, registry: &mut Registry) {
        if registry.mounts.contains(TypeId::of::<R>()) {
            registry.errors.push(DeclarationError::of::<R>(
                Site::Registration,
                DeclarationErrorKind::DuplicateResource,
            ));
            return;
        }
        let Self(customize) = *self;
        let prefix = registry.prefix.clone();
        let mounted = Mounted::new(
            customize(R::declare()),
            &prefix,
            &FieldResolver::new(registry.schema.clone()),
        );
        let Some(url) = registry.claim_slug::<R>("ResourceDef::slug", &mounted.slug) else {
            return;
        };
        register_routes::<R>(registry, &url, !mounted.actions.entries().is_empty());
        if registry.root.is_none() {
            registry.root = Some(Root::Redirect(url));
        }
        registry.nav_items.push(mounted.navigation.clone());
        registry.children.insert(
            TypeId::of::<R>(),
            Child {
                slug: mounted.slug.clone(),
                plural_label: mounted.plural_label.clone(),
                render: relation_table::<R>,
            },
        );
        registry.resources.push(Registered {
            resource: TypeId::of::<R>(),
            name: std::any::type_name::<R>(),
            slug: mounted.slug.clone(),
            check: check_resource::<R>,
            relations: mounted
                .relations
                .iter()
                .map(|relation| Link {
                    child: relation.child,
                    child_name: relation.child_name,
                    misdeclared: relation.misdeclared.clone(),
                })
                .collect(),
        });
        registry.mounts.insert(Arc::new(mounted));
    }
}

/// Registers a resource's routes under its list `url`.
fn register_routes<R: Resource>(registry: &mut Registry, url: &str, has_actions: bool) {
    use http::Method;

    registry.page(Method::GET, url, resource_list::<R>);
    // The handler 404s a resource that declares no view; `matchit` prefers the static `create`
    // segment over the `{id}` parameter, so registration order does not matter.
    registry.page(
        Method::GET,
        &format!("{url}/{RECORD_ROUTE_PARAM}"),
        resource_view::<R>,
    );
    registry.page(
        Method::POST,
        &format!("{url}/{RECORD_ROUTE_PARAM}/{DELETE_ROUTE_SEGMENT}"),
        resource_delete::<R>,
    );
    registry.page(
        Method::POST,
        &format!("{url}/{BULK_DELETE_ROUTE_SEGMENT}"),
        resource_bulk_delete::<R>,
    );
    // The `-` segment keeps `actions` from shadowing the edit and delete
    // routes of a record whose key is `actions`.
    if has_actions {
        registry.page(
            Method::POST,
            &format!(
                "{url}/{RECORD_ROUTE_PARAM}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}"
            ),
            resource_row_action::<R>,
        );
        registry.page(
            Method::POST,
            &format!("{url}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}"),
            resource_bulk_action::<R>,
        );
    }
    registry.route(Method::GET, &format!("{url}/export"), resource_export::<R>);
    if <R::Form as RecordForm>::HAS_FORM {
        let create_url = format!("{url}/{CREATE_ROUTE_SEGMENT}");
        registry.page(Method::GET, &create_url, resource_create::<R>);
        registry.page(Method::POST, &create_url, resource_create_post::<R>);
        let edit_url = format!("{url}/{RECORD_ROUTE_PARAM}/{EDIT_ROUTE_SEGMENT}");
        registry.page(Method::GET, &edit_url, resource_edit::<R>);
        registry.page(Method::POST, &edit_url, resource_edit_post::<R>);
        registry.route(
            Method::GET,
            &format!("{url}/options"),
            resource_options::<R>,
        );
    }
}

impl<P: Page> Registration for PageRegistration<P> {
    fn register(self: Box<Self>, registry: &mut Registry) {
        let url = if self.home {
            if matches!(registry.root, Some(Root::Home)) {
                registry.errors.push(DeclarationError::of::<P>(
                    Site::Registration,
                    DeclarationErrorKind::SecondHome,
                ));
                return;
            }
            registry.root = Some(Root::Home);
            registry
                .urls
                .insert(TypeId::of::<P>(), registry.prefix.clone());
            registry.prefix.clone()
        } else {
            let Some(url) = registry.claim_slug::<P>("Page::slug", &P::slug()) else {
                return;
            };
            url
        };
        registry.page(http::Method::GET, &url, page_handler::<P>);
        let item = P::navigation().resolved(&url);
        if self.home {
            registry.nav_items.insert(0, item);
        } else {
            registry.nav_items.push(item);
        }
    }
}

#[cfg(test)]
impl super::Panel {
    /// Runs the panel's registrations with no app schema, as mounting does first.
    pub(super) fn registered(self) -> Registry {
        let mut registry = Registry::new(self.prefix, None);
        for registration in self.registrations {
            registration.register(&mut registry);
        }
        registry
    }
}
