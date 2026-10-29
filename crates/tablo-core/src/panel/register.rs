//! Registering a panel's resources and pages as it mounts: each resource's def is built once and
//! filled in, and every slug, route and sidebar entry follows from it.

use std::{any::TypeId, collections::HashMap, sync::Arc};

use topcoat::router::{PageFn, RouteFn};

use super::{
    Root,
    actions::{
        relation_action_options, relation_list_action, relation_record_action,
        resource_action_options, resource_bulk_delete, resource_delete, resource_export,
        resource_list_action, resource_options, resource_record_action,
    },
    build::validate_route_segment,
    detail::resource_view,
    forms::{
        ChildForm, resource_create, resource_create_post, resource_edit, resource_edit_post,
        resource_import, resource_import_post, resource_import_template, resource_picked,
    },
    list::{resource_columns, resource_list},
    pages::{page_action, page_action_options, page_handler},
    record_page::{record_page_view, relation_tab_view},
    relations::{Child, relation_table},
    state::{BadgeFn, NavEntry, ResourceTrail, Searchable},
};
use crate::{
    Ability, DeclarationError, DeclarationErrorKind, Page, PageDef, Site,
    form::RecordForm,
    naming::route_path,
    page::{MountedPage, PageCheck, check_page},
    panel::url::{
        ACTION_ROUTE_PARAM, ACTIONS_ROUTE_SEGMENT, BULK_DELETE_ROUTE_SEGMENT,
        COLUMNS_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT, DASH_ROUTE_SEGMENT, DELETE_ROUTE_SEGMENT,
        EDIT_ROUTE_SEGMENT, EXPORT_ROUTE_SEGMENT, OPTIONS_ROUTE_SEGMENT, PICKED_ROUTE_SEGMENT,
        RECORD_ROUTE_PARAM, RELATED_ROUTE_PARAM, RELATION_ROUTE_PARAM, RELATIONS_ROUTE_SEGMENT,
        import_template_url, import_url, list_route,
    },
    resource::{
        Mounted, Mounts, Places, Resource, ResourceCheck, ResourceDef, check_record_pages,
        check_resource,
    },
};

mod form_facts;
mod links;

use form_facts::FormFacts;
use links::Link;

/// A resource or page waiting for its panel to mount.
pub(super) trait Registration: Send {
    fn register(self: Box<Self>, registry: &mut Registry);
}

/// Adjusts a resource's def for one panel.
type Customize<R> = Box<dyn FnOnce(ResourceDef<R>) -> ResourceDef<R> + Send>;

/// [`Panel::resource_with`](super::Panel::resource_with)'s pending resource.
pub(super) struct ResourceRegistration<R: Resource>(pub(super) Customize<R>);

/// Adjusts a page's def for one panel.
pub(super) type CustomizePage = Box<dyn FnOnce(PageDef) -> PageDef + Send>;

/// [`Panel::page_with`](super::Panel::page_with)'s pending page, or with `home` set,
/// [`Panel::home`](super::Panel::home)'s.
pub(super) struct PageRegistration<P> {
    pub(super) home: bool,
    pub(super) customize: CustomizePage,
    pub(super) _page: std::marker::PhantomData<fn() -> P>,
}

/// Everything a panel's registrations add up to.
pub(super) struct Registry {
    pub(super) prefix: String,
    schema: Option<crate::toasty_compat::model::AppSchema>,
    /// Every slug a resource or page mounts at: one namespace.
    slugs: Vec<String>,
    /// The URL each resource and page serves at, by type.
    pub(super) urls: HashMap<TypeId, String>,
    /// The registered pages as the panel mounted them, by type.
    pub(super) mounted_pages: HashMap<TypeId, Arc<MountedPage>>,
    pub(super) nav_items: Vec<NavEntry>,
    /// The resources the global search searches, in registration order.
    pub(super) searches: Vec<Searchable>,
    pub(super) pages: Vec<PageFn>,
    pub(super) routes: Vec<RouteFn>,
    pub(super) root: Option<Root>,
    pub(super) mounts: Mounts,
    pub(super) resources: Vec<Registered>,
    /// Each registered page's declaration check.
    pub(super) page_checks: Vec<PageCheck>,
    /// Each resource's relation table, by resource type.
    pub(super) children: HashMap<TypeId, Child>,
    pub(super) errors: Vec<DeclarationError>,
}

/// One registered resource, its type erased.
pub(super) struct Registered {
    resource: TypeId,
    pub(super) name: &'static str,
    slug: String,
    /// The declaration checks mount runs with the app's values.
    pub(super) check: ResourceCheck,
    /// The form's file fields, whose bytes only an installed uploader keeps.
    pub(super) file_fields: Vec<String>,
    /// The form's file fields' `max_bytes`, which the panel's body cap must exceed.
    pub(super) file_caps: Vec<(String, usize)>,
    /// Whether the list declares a toggleable column, whose choice a [`ColumnPreference`]
    /// keeps.
    ///
    /// [`ColumnPreference`]: crate::auth::ColumnPreference
    pub(super) toggles: bool,
    relations: Vec<Link>,
    /// The slugs of the resource's record pages, which its relation tabs may not take.
    record_page_slugs: Vec<String>,
    /// Serves the resource's relation tabs.
    relation_tab: topcoat::router::PageRenderFn,
    /// What a `has_one` relation's related form needs of the resource, as owner or as child.
    form: FormFacts,
}

impl Registry {
    pub(super) fn new(
        prefix: String,
        schema: Option<crate::toasty_compat::model::AppSchema>,
    ) -> Self {
        Self {
            prefix,
            schema,
            slugs: Vec::new(),
            urls: HashMap::new(),
            mounted_pages: HashMap::new(),
            nav_items: Vec::new(),
            searches: Vec::new(),
            pages: Vec::new(),
            routes: Vec::new(),
            root: None,
            mounts: Mounts::default(),
            resources: Vec::new(),
            page_checks: Vec::new(),
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

    /// Refuses a panel whose resources all leave the sidebar while no home page takes the root:
    /// the root, and the landing after a sign-in, would answer 404.
    pub(super) fn check_root(&mut self) {
        if self.root.is_none() && !self.resources.is_empty() {
            self.errors
                .push(DeclarationError::panel(DeclarationErrorKind::NoRoot));
        }
    }

    fn page(&mut self, method: http::Method, url: &str, handler: topcoat::router::PageRenderFn) {
        self.pages
            .push(PageFn::new(method, route_path(url), handler));
    }

    fn route(&mut self, method: http::Method, url: &str, handler: topcoat::router::RouteHandlerFn) {
        self.routes
            .push(RouteFn::new(method, route_path(url), handler));
    }
}

/// The segments the panel routes under its prefix itself, which no resource or page may take as its
/// slug: `-` leads the home page's action routes.
const RESERVED_SLUGS: &[&str] = &[
    "login",
    "logout",
    "register",
    "register-tenant",
    "tenant",
    "profile",
    "forgot-password",
    "reset-password",
    "verify-email",
    "email-change",
    "two-factor",
    "two-factor-challenge",
    "locale",
    "-",
];

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
        let mounted = Mounted::new(customize(R::declare()), &prefix);
        let Some(url) = registry.claim_slug::<R>("ResourceDef::slug", &mounted.slug) else {
            return;
        };
        let entries = mounted.actions.entries();
        let routes = OptionalRoutes {
            create: mounted.creates,
            import: mounted.creates && mounted.imports,
            record: entries.iter().any(|a| a.places.intersects(Places::RECORD)),
            list: !mounted.header_actions.is_empty()
                || entries.iter().any(|a| a.places.contains(Places::BULK)),
            links: mounted
                .relations
                .iter()
                .any(|relation| relation.links().is_some()),
            columns: mounted.table.has_toggleable_columns(),
        };
        let toggles = routes.columns;
        register_routes::<R>(registry, &url, routes);
        let pages = &mounted.record_pages;
        let validate = |slug: &str| validate_route_segment("RecordPageDef::slug", slug).err();
        if check_record_pages(pages, validate, &mut registry.errors) {
            for page in pages {
                let route = format!("{url}/{RECORD_ROUTE_PARAM}/{}", page.slug);
                registry.page(http::Method::GET, &route, record_page_view::<R>);
            }
        }
        if registry.root.is_none() && !mounted.navigation_hidden {
            registry.root = Some(Root::Redirect(url.clone()));
        }
        if !mounted.navigation_hidden {
            registry.nav_items.push(NavEntry {
                item: mounted.navigation.clone(),
                visible: Arc::new(|cx| crate::can::<R>(cx, Ability::ViewAny)),
                resource_list: Some(ResourceTrail {
                    list: url,
                    viewed: mounted.viewed(),
                    pages: pages
                        .iter()
                        .map(|page| (page.slug.clone(), page.label.clone()))
                        .collect(),
                }),
                badge: mounted.counts_navigation_badge().then(|| -> BadgeFn {
                    Arc::new(|cx| {
                        Box::pin(async move {
                            crate::resource::mounted::<R>(cx)?
                                .navigation_badge(cx)
                                .await
                        })
                    })
                }),
            });
        }
        if mounted.globally_searched() {
            registry.searches.push(Searchable {
                label: mounted.plural_label.clone(),
                list: mounted.url.clone(),
                visible: crate::resource::searchable::<R>,
                search: crate::resource::search::<R>,
            });
        }
        registry.children.insert(
            TypeId::of::<R>(),
            Child {
                slug: mounted.slug.clone(),
                label: mounted.label.clone(),
                plural_label: mounted.plural_label.clone(),
                render: relation_table::<R>,
                listable: crate::can_list::<R>,
                form: ChildForm::of::<R>(),
            },
        );
        registry.resources.push(Registered {
            resource: TypeId::of::<R>(),
            name: std::any::type_name::<R>(),
            slug: mounted.slug.clone(),
            check: check_resource::<R>,
            toggles,
            file_fields: mounted
                .form
                .fields()
                .filter(|field| field.is_file())
                .map(|field| field.name().to_string())
                .collect(),
            file_caps: mounted.form.file_caps(),
            relations: mounted
                .relations
                .iter()
                .map(|relation| Link::of::<R>(relation, registry.schema.as_ref()))
                .collect(),
            record_page_slugs: mounted
                .record_pages
                .iter()
                .map(|page| page.slug.clone())
                .collect(),
            relation_tab: relation_tab_view::<R>,
            form: FormFacts::of(&mounted),
        });
        registry.mounts.insert(Arc::new(mounted));
    }
}

/// Which optional routes a resource serves.
struct OptionalRoutes {
    /// The create page: a record form, and not [`edit_only`](crate::ResourceDef::edit_only).
    create: bool,
    /// The CSV import beside the create page: [`importable`](crate::ResourceDef::importable).
    import: bool,
    /// One record's: an action placed on a row or a detail or edit page.
    record: bool,
    /// The list's: a header action, or an action placed on the bulk bar.
    list: bool,
    /// A record's many-to-many relations': attach and detach.
    links: bool,
    /// The column picker's: a toggleable column on the list.
    columns: bool,
}

/// Registers a resource's routes under its list `url`.
fn register_routes<R: Resource>(registry: &mut Registry, url: &str, routes: OptionalRoutes) {
    use http::Method;

    registry.page(Method::GET, url, resource_list::<R>);
    // The handler 404s a resource that declares no view. `matchit` prefers a static segment over
    // the `{id}` parameter, so the list's own routes sit under `-` (`{url}/-/create`): a record
    // whose key is `create` keeps its detail page.
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
        &list_route(url, BULK_DELETE_ROUTE_SEGMENT),
        resource_bulk_delete::<R>,
    );
    if routes.columns {
        registry.page(
            Method::POST,
            &list_route(url, COLUMNS_ROUTE_SEGMENT),
            resource_columns::<R>,
        );
    }
    // The `-` segment keeps `actions` from shadowing the edit and delete
    // routes of a record whose key is `actions`.
    if routes.record {
        registry.page(
            Method::POST,
            &format!(
                "{url}/{RECORD_ROUTE_PARAM}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}"
            ),
            resource_record_action::<R>,
        );
    }
    if routes.list {
        registry.page(
            Method::POST,
            &list_actions_route(url),
            resource_list_action::<R>,
        );
    }
    // A record's action asks for the same input as the list's, so one route serves both.
    if routes.record || routes.list {
        registry.route(
            Method::GET,
            &format!("{}/options", list_actions_route(url)),
            resource_action_options::<R>,
        );
    }
    if routes.links {
        // `{url}/{id}/-/relations/{relation}`: the `-` segment keeps it from a record's own
        // routes, as it does the record's actions.
        let relation = format!(
            "{url}/{RECORD_ROUTE_PARAM}/{DASH_ROUTE_SEGMENT}/{RELATIONS_ROUTE_SEGMENT}/{RELATION_ROUTE_PARAM}"
        );
        registry.page(
            Method::POST,
            &list_actions_route(&relation),
            relation_list_action::<R>,
        );
        registry.page(
            Method::POST,
            &format!(
                "{relation}/{RELATED_ROUTE_PARAM}/{DASH_ROUTE_SEGMENT}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}"
            ),
            relation_record_action::<R>,
        );
        registry.route(
            Method::GET,
            &format!("{}/options", list_actions_route(&relation)),
            relation_action_options::<R>,
        );
    }
    registry.route(
        Method::GET,
        &list_route(url, EXPORT_ROUTE_SEGMENT),
        resource_export::<R>,
    );
    if routes.create {
        let create_url = list_route(url, CREATE_ROUTE_SEGMENT);
        registry.page(Method::GET, &create_url, resource_create::<R>);
        registry.page(Method::POST, &create_url, resource_create_post::<R>);
        registry.page(
            Method::GET,
            &list_route(url, PICKED_ROUTE_SEGMENT),
            resource_picked::<R>,
        );
    }
    if routes.import {
        let import_url = import_url(url);
        registry.page(Method::GET, &import_url, resource_import::<R>);
        registry.page(Method::POST, &import_url, resource_import_post::<R>);
        registry.route(
            Method::GET,
            &import_template_url(url),
            resource_import_template::<R>,
        );
    }
    if <R::Form as RecordForm>::HAS_FORM {
        let edit_url = format!("{url}/{RECORD_ROUTE_PARAM}/{EDIT_ROUTE_SEGMENT}");
        registry.page(Method::GET, &edit_url, resource_edit::<R>);
        registry.page(Method::POST, &edit_url, resource_edit_post::<R>);
        registry.route(
            Method::GET,
            &list_route(url, OPTIONS_ROUTE_SEGMENT),
            resource_options::<R>,
        );
    }
}

/// The route of the actions under a list or page `url`: `{url}/-/actions/{action}`.
fn list_actions_route(url: &str) -> String {
    format!(
        "{}/{ACTION_ROUTE_PARAM}",
        list_route(url, ACTIONS_ROUTE_SEGMENT)
    )
}

impl<P: Page> Registration for PageRegistration<P> {
    fn register(self: Box<Self>, registry: &mut Registry) {
        if registry.mounted_pages.contains_key(&TypeId::of::<P>()) {
            registry.errors.push(DeclarationError::of::<P>(
                Site::Registration,
                DeclarationErrorKind::DuplicatePage,
            ));
            return;
        }
        let Self {
            home, customize, ..
        } = *self;
        let page = Arc::new(MountedPage::new::<P>(customize(P::declare())));
        let url = if home {
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
            let Some(url) = registry.claim_slug::<P>("PageDef::slug", &page.slug) else {
                return;
            };
            url
        };
        registry.page(http::Method::GET, &url, page_handler::<P>);
        if !page.header_actions.is_empty() {
            registry.page(
                http::Method::POST,
                &list_actions_route(&url),
                page_action::<P>,
            );
            registry.route(
                http::Method::GET,
                &format!("{}/options", list_actions_route(&url)),
                page_action_options::<P>,
            );
        }
        registry
            .page_checks
            .push(check_page::<P>(Arc::clone(&page)));
        let item = NavEntry {
            item: page.navigation.clone().resolved(&url),
            visible: page.access(),
            resource_list: None,
            badge: None,
        };
        registry.mounted_pages.insert(TypeId::of::<P>(), page);
        if home {
            registry.nav_items.insert(0, item);
        } else {
            registry.nav_items.push(item);
        }
    }
}
