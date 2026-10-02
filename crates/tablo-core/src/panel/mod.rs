//! `Panel` — an admin panel an app mounts into its router.

mod actions;
mod build;
mod detail;
mod forms;
pub(crate) mod gate;
mod headers;
mod list;
mod pages;
mod relations;
mod search;
mod shell;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod url;
mod write;

use std::{any::TypeId, collections::HashMap, path::PathBuf};

use topcoat::{
    asset::Asset,
    font::Font,
    router::{LayoutRenderFn, PageFn, RouteFn},
};

#[cfg(test)]
pub(crate) use self::search::TABLE_SEARCH_PATH;
use self::{
    actions::{
        resource_bulk_action, resource_bulk_delete, resource_delete, resource_export,
        resource_options, resource_row_action,
    },
    build::{ResourceCheck, check_resource, is_directory_pattern, validate_route_segment},
    detail::resource_view,
    forms::{resource_create, resource_create_post, resource_edit, resource_edit_post},
    list::resource_list,
    pages::page_handler,
    search::{SearchFn, search_handler_for},
    shell::ShellAssets,
};
pub use self::{build::RouterBuilderPanelExt, shell::Brand};
pub(crate) use self::{
    build::route_path,
    forms::parse_form_body,
    gate::panel_prefix,
    relations::relation_table,
    search::{
        RelationRequest, RelationSearchFn, relation_search_handler_for, table_relation_search,
        table_search,
    },
};
use crate::{
    Page,
    form::RecordForm,
    resource::{
        ACTION_ROUTE_PARAM, ACTIONS_ROUTE_SEGMENT, BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT,
        DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT, NavigationItem, RECORD_ROUTE_PARAM, Resource,
    },
};

/// Returns the panel's list table for `R` for a page that owns its table; pair it with
/// [`TablePage::load`](crate::resource::TablePage::load) and
/// [`Table::render_with_state`](crate::resource::Table::render_with_state).
pub fn wired_table<R: Resource>(cx: &topcoat::context::Cx) -> crate::resource::Table<R::Model> {
    self::list::wire_table_actions::<R>(cx, false)
}

/// An admin panel: resources and pages under one prefix, framed by one shell
/// and gated by one [`Auth`](crate::auth::Auth).
///
/// The app owns the router and mounts the panel into it with
/// [`RouterBuilderPanelExt::panel`]:
///
/// ```ignore
/// let router = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .build();
/// ```
pub struct Panel {
    prefix: String,
    shell_assets: Option<ShellAssets>,
    brand: Option<Brand>,
    dark_mode: Option<bool>,
    nav_items: Vec<NavigationItem>,
    pages: Vec<PageFn>,
    routes: Vec<RouteFn>,
    root: Option<Root>,
    /// Frames the panel's pages; `None` renders the shipped shell.
    layout: Option<LayoutRenderFn>,
    /// The URL each resource and page serves at, by type.
    urls: HashMap<TypeId, String>,
    /// Every slug a resource or page mounts at: one namespace.
    slugs: Vec<String>,
    resource_slugs: Vec<String>,
    /// Each registered resource's relation keys, by resource type name.
    relations: Vec<(&'static str, Vec<String>)>,
    search_handlers: HashMap<String, SearchFn>,
    /// Each relation's live-search loader, by (parent slug, child slug).
    relation_handlers: HashMap<(String, String), RelationSearchFn>,
    /// `Content-Security-Policy: frame-ancestors …` for every response under the prefix; `None` opts out, the default is `'self'`.
    frame_ancestors: Option<String>,
    /// Per-resource declaration checks run at mount before anything is served.
    resource_checks: Vec<ResourceCheck>,
    /// Registration failures for mount to report.
    registration_errors: Vec<String>,
    /// Where file field bytes go; `None` stores the sanitized basename.
    uploads: Option<crate::upload::InstalledUploader>,
    /// App-owned filesystem directories served with hardening headers.
    served_dirs: Vec<(String, PathBuf)>,
    login_hint: Option<String>,
    auth: crate::auth::Auth,
}

impl Panel {
    /// Creates a `Panel` mounted at `prefix`, defaulting an empty prefix to `"/admin"`.
    pub fn new(prefix: impl Into<String>) -> Self {
        let raw = prefix.into();
        let trimmed = raw.trim().trim_matches('/').to_string();
        let prefix = if trimmed.is_empty() {
            "/admin".to_string()
        } else {
            format!("/{trimmed}")
        };
        let registration_errors: Vec<String> = prefix
            .trim_matches('/')
            .split('/')
            .filter_map(|segment| validate_route_segment("panel prefix", segment).err())
            .collect();
        Self {
            prefix,
            shell_assets: None,
            brand: None,
            dark_mode: None,
            nav_items: Vec::new(),
            pages: Vec::new(),
            routes: Vec::new(),
            root: None,
            layout: None,
            urls: HashMap::new(),
            slugs: Vec::new(),
            resource_slugs: Vec::new(),
            relations: Vec::new(),
            search_handlers: HashMap::new(),
            relation_handlers: HashMap::new(),
            frame_ancestors: Some(headers::DEFAULT_FRAME_ANCESTORS.to_string()),
            registration_errors,
            resource_checks: Vec::new(),
            uploads: None,
            served_dirs: Vec::new(),
            login_hint: None,
            auth: crate::auth::Auth::default(),
        }
    }

    /// Returns the mount prefix, e.g. `"/admin"`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Installs the [`Uploader`](crate::Uploader) this panel's file fields store through; without one a file field stores the sanitized client filename.
    pub fn uploads(mut self, uploader: impl crate::Uploader) -> Self {
        self.uploads = Some(crate::upload::InstalledUploader::new(uploader));
        self
    }

    /// Serves the directory `dir` at route pattern `path`, which must end in a catch-all and sits outside the auth gate with hardening headers.
    pub fn serve_dir(mut self, path: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if !is_directory_pattern(&path) {
            self.registration_errors.push(format!(
                "serve_dir path '{path}': must end in a catch-all like '/uploads/{{*file}}'"
            ));
        }
        self.served_dirs.push((path, dir.into()));
        self
    }

    /// Registers the stylesheet and font the default shell links, resolved through the router's asset bundle.
    pub fn shell_assets(mut self, stylesheet: Asset, font: Font) -> Self {
        self.shell_assets = Some(ShellAssets { stylesheet, font });
        self
    }

    /// Declares a `Resource` for this panel at `{prefix}/{slug}` with its routes, navigation entry, and mount-time declaration checks.
    pub fn resource<R: Resource>(mut self) -> Self {
        let Some(url) = self.register_common::<R>() else {
            return self;
        };
        if <R::Form as RecordForm>::HAS_FORM {
            self.register_form_routes::<R>(&url);
        }
        self.finish_registration::<R>(url);
        let declared = R::relations();
        let keys = declared
            .iter()
            .map(|relation| relation.key().to_string())
            .collect::<Vec<_>>();
        for relation in &declared {
            if let Some(error) = relation.misdeclared() {
                self.registration_errors.push(format!(
                    "resource `{}`'s relation `{}`: {error}",
                    std::any::type_name::<R>(),
                    relation.key()
                ));
            }
            self.relation_handlers.insert(
                (R::slug(), relation.key().to_string()),
                relation.search_handler(),
            );
        }
        if !keys.is_empty() {
            self.relations.push((std::any::type_name::<R>(), keys));
        }
        self
    }

    /// Registers the create page, edit page, and relationship-options endpoint of a resource with a record form.
    fn register_form_routes<R: Resource>(&mut self, url: &str) {
        let create_url = format!("{url}/{CREATE_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&create_url),
            resource_create::<R>,
        ));
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&create_url),
            resource_create_post::<R>,
        ));
        let edit_url = format!("{url}/{RECORD_ROUTE_PARAM}/{EDIT_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&edit_url),
            resource_edit::<R>,
        ));
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&edit_url),
            resource_edit_post::<R>,
        ));
        let options_url = format!("{url}/options");
        self.routes.push(RouteFn::new(
            http::Method::GET,
            route_path(&options_url),
            resource_options::<R>,
        ));
    }

    /// Registers a resource's shared routes and returns its list URL, or `None` when the slug is refused.
    fn register_common<R: Resource>(&mut self) -> Option<String> {
        let url = self.claim_slug::<R>("Resource::slug", R::slug())?;
        self.resource_slugs.push(R::slug());
        self.resource_checks.push(check_resource::<R>);
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&url),
            resource_list::<R>,
        ));
        // The handler 404s a resource that declares no view; `matchit` prefers the static
        // `create` segment over the `{id}` parameter, so registration order does not matter.
        let detail_url = format!("{url}/{RECORD_ROUTE_PARAM}");
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&detail_url),
            resource_view::<R>,
        ));
        let delete_url = format!("{url}/{RECORD_ROUTE_PARAM}/{DELETE_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&delete_url),
            resource_delete::<R>,
        ));
        let bulk_delete_url = format!("{url}/{BULK_DELETE_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&bulk_delete_url),
            resource_bulk_delete::<R>,
        ));
        // Registered only for a resource that declares some: the static `actions` segment
        // would otherwise shadow the edit and delete routes of a record whose key is `actions`.
        if !R::actions().entries().is_empty() {
            let row_action_url =
                format!("{url}/{RECORD_ROUTE_PARAM}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}");
            self.pages.push(PageFn::new(
                http::Method::POST,
                route_path(&row_action_url),
                resource_row_action::<R>,
            ));
            let bulk_action_url = format!("{url}/{ACTIONS_ROUTE_SEGMENT}/{ACTION_ROUTE_PARAM}");
            self.pages.push(PageFn::new(
                http::Method::POST,
                route_path(&bulk_action_url),
                resource_bulk_action::<R>,
            ));
        }
        let export_url = format!("{url}/export");
        self.routes.push(RouteFn::new(
            http::Method::GET,
            route_path(&export_url),
            resource_export::<R>,
        ));
        Some(url)
    }

    /// Finishes a resource's registration.
    fn finish_registration<R: Resource>(&mut self, url: String) {
        self.search_handlers
            .insert(url.clone(), search_handler_for::<R>());
        if self.root.is_none() {
            self.root = Some(Root::Redirect(url));
        }
        let nav_item = self.nav_item::<R>();
        self.nav_items.push(nav_item);
    }

    /// Claims `{prefix}/{slug}` for `T`, recording a refusal and returning `None` when the slug is unavailable.
    fn claim_slug<T: 'static>(&mut self, kind: &str, slug: String) -> Option<String> {
        let owner = std::any::type_name::<T>();
        let refused = if let Err(error) = validate_route_segment(kind, &slug) {
            Some(error)
        } else if RESERVED_SLUGS.contains(&slug.as_str()) {
            Some(format!(
                "slug '{slug}' of `{owner}`: the panel routes `{}/{slug}` itself",
                self.prefix
            ))
        } else if self.slugs.contains(&slug) {
            Some(format!(
                "duplicate slug '{slug}': `{owner}` mounts where another resource or page does \
                 — each needs a distinct `slug()`"
            ))
        } else {
            None
        };
        if let Some(error) = refused {
            self.registration_errors.push(error);
            return None;
        }
        let url = format!("{}/{slug}", self.prefix);
        self.slugs.push(slug);
        self.urls.insert(TypeId::of::<T>(), url.clone());
        Some(url)
    }

    /// Declares a [`Page`] at `{prefix}/{slug}` with its sidebar entry; pages share the resources' slug namespace.
    pub fn page<P: Page>(mut self) -> Self {
        if let Some(url) = self.claim_slug::<P>("Page::slug", P::slug()) {
            let item = self.mount_page::<P>(&url);
            self.nav_items.push(item);
        }
        self
    }

    /// Declares the panel's home page at the panel prefix, replacing the redirect to the first resource's list.
    pub fn home<P: Page>(mut self) -> Self {
        if matches!(self.root, Some(Root::Home)) {
            self.registration_errors.push(format!(
                "Panel::home: a home page is already registered; `{}` would shadow it",
                std::any::type_name::<P>()
            ));
            return self;
        }
        self.root = Some(Root::Home);
        self.urls.insert(TypeId::of::<P>(), self.prefix.clone());
        let item = self.mount_page::<P>(&self.prefix.clone());
        self.nav_items.insert(0, item);
        self
    }

    /// Routes `P`'s `GET` at `url` and returns its sidebar entry at the same URL.
    fn mount_page<P: Page>(&mut self, url: &str) -> NavigationItem {
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(url),
            page_handler::<P>,
        ));
        P::navigation().resolved(url)
    }

    /// Frames the panel's pages with `render` instead of the shipped shell; `render` usually wraps [`Panel::layout_shell`].
    pub fn layout(mut self, render: LayoutRenderFn) -> Self {
        self.layout = Some(render);
        self
    }

    /// Sets branding for the shell (sidebar header, login card, and the topbar below `md`).
    pub fn brand(mut self, brand: Brand) -> Self {
        self.brand = Some(brand);
        self
    }

    /// Sets the `frame-ancestors` directive the panel sends on every response under its prefix, defaulting to `'self'`.
    pub fn frame_ancestors(mut self, ancestors: impl Into<String>) -> Self {
        self.frame_ancestors = Some(ancestors.into());
        self
    }

    /// Sends no `frame-ancestors` directive for deployments whose proxy owns the whole CSP.
    pub fn without_frame_ancestors(mut self) -> Self {
        self.frame_ancestors = None;
        self
    }

    /// Sets the theme a visitor who has not chosen one sees, dark when `true`.
    pub fn dark_mode(mut self, enabled: bool) -> Self {
        self.dark_mode = Some(enabled);
        self
    }

    /// Configures this panel's authentication, defaulting to password auth; sessions belong to the panel that signed them in.
    pub fn auth(mut self, auth: crate::auth::Auth) -> Self {
        self.auth = auth;
        self
    }

    /// Renders a line under the login form for demo credentials or deployment hints.
    pub fn login_hint(mut self, hint: impl Into<String>) -> Self {
        self.login_hint = Some(hint.into());
        self
    }
}

/// The segments the panel routes under its prefix itself, which no resource or page may take as its slug.
const RESERVED_SLUGS: &[&str] = &["login", "logout"];

/// What the panel serves at its prefix.
enum Root {
    /// A redirect to the first declared resource's list.
    Redirect(String),
    /// The [`Panel::home`] page.
    Home,
}

impl Panel {
    /// Derives `R`'s [`NavigationItem`] at `{prefix}/{slug}`, taking an explicit [`NavTarget::Url`] as written.
    pub(crate) fn nav_item<R: Resource>(&self) -> NavigationItem {
        R::navigation().resolved(&format!("{}/{}", self.prefix, R::slug()))
    }
}

#[cfg(test)]
mod tests;
