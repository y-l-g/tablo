//! `Panel` — the admin application shell.
//!
//! Owns the [`Router`](topcoat::router::Router) and the `Db` in `app_context`, and registers each
//! declared [`Resource`]'s list page at `{prefix}/{slug}` (Filament-style
//! routes — ADR-0008). See `CONTEXT.md`.
//!
//! Layout: the [`Panel`] builder and its navigation seam live here; assembly
//! (`build`, declaration checks, route paths) in `build`; the auth/tenant
//! gate and prefix URLs in `gate`; shell rendering in `shell`; list + live
//! shard support in `list`; form decoding and create/edit in `forms`; the
//! record detail page in `detail`; delete/bulk/export/options in `actions`;
//! the live-search registry + shard dispatch in `search`; and response
//! hardening headers in `headers`.

mod actions;
mod build;
mod detail;
mod forms;
mod gate;
mod headers;
mod list;
mod search;
mod shell;
#[cfg(test)]
mod test_support;

use std::{collections::HashMap, path::PathBuf};

use toasty::Db;
use topcoat::{
    asset::{Asset, AssetConfig},
    font::Font,
    router::{PageFn, PageRenderFn, RouteFn},
};

#[cfg(feature = "auth")]
pub(crate) use self::forms::parse_form_body;
#[cfg(feature = "auth")]
pub(crate) use self::gate::{LoginHint, PanelPrefix};
#[cfg(test)]
pub(crate) use self::search::TABLE_SEARCH_PATH;
pub use self::shell::{Brand, DarkMode};
use self::{
    actions::{resource_bulk_delete, resource_delete, resource_export, resource_options},
    build::{
        ResourceCheck, check_form_resource, check_list_resource, check_resource,
        is_directory_pattern, validate_route_segment,
    },
    detail::{FormValues, ViewValues, resource_view},
    forms::{resource_create, resource_create_post, resource_edit, resource_edit_post},
    list::resource_list,
    search::{SearchFn, search_handler_for},
    shell::ShellAssets,
};
pub(crate) use self::{build::route_path, search::table_search};
use crate::{
    form::FormResource,
    resource::{
        BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT, DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT,
        NavigationItem, RECORD_ROUTE_PARAM, Resource,
    },
};

/// The admin application.
///
/// ```ignore
/// Panel::new("admin")
///     .app_context(db)
///     .resource::<UserResource>()
///     .build().expect("panel builds")
/// ```
pub struct Panel {
    prefix: String,
    db: Option<Db>,
    assets: Option<AssetConfig>,
    shell_assets: Option<ShellAssets>,
    brand: Option<Brand>,
    dark_mode: Option<bool>,
    nav_items: Vec<NavigationItem>,
    pages: Vec<PageFn>,
    routes: Vec<RouteFn>,
    root_target: Option<String>,
    slugs: Vec<String>,
    search_handlers: HashMap<String, SearchFn>,
    /// `Content-Security-Policy: frame-ancestors …` for every response
    /// `None` opts out. Defaults to `'self'`.
    frame_ancestors: Option<String>,
    /// Per-resource declaration checks, monomorphized at
    /// `resource::<R>()` and run by `build` before anything is served.
    resource_checks: Vec<ResourceCheck>,
    /// Registration failures collected by the declarative builders
    /// `Panel::resource` cannot return `Result`, so a bad `slug`
    /// or a duplicate is recorded here and reported by `build`.
    registration_errors: Vec<String>,
    /// Where `FileUpload` bytes go; `None` stores the sanitized basename.
    uploads: Option<crate::upload::InstalledUploader>,
    /// App-owned filesystem directories served from this panel's router
    /// `(route pattern, directory)`.
    served_dirs: Vec<(String, PathBuf)>,
    #[cfg(feature = "auth")]
    login_hint: Option<String>,
    #[cfg(feature = "auth")]
    auth: crate::auth::Auth,
    /// `true` once the app acknowledged the feature-off build with
    /// [`Panel::auth`]; [`Panel::build`] refuses the panel otherwise.
    #[cfg(not(feature = "auth"))]
    auth_disabled: bool,
}

impl Panel {
    /// Create a `Panel` mounted at `prefix` (e.g. `"admin"` → `"/admin"`).
    pub fn new(prefix: impl Into<String>) -> Self {
        let raw = prefix.into();
        let trimmed = raw.trim_matches('/').trim().to_string();
        let prefix = if trimmed.is_empty() {
            "/admin".to_string()
        } else {
            format!("/{trimmed}")
        };
        // The prefix is free-form too, and every route path is built from it
        // validate it once here rather than panicking at the first
        // `route_path` call.
        let registration_errors: Vec<String> = prefix
            .trim_matches('/')
            .split('/')
            .filter_map(|segment| validate_route_segment("panel prefix", segment).err())
            .collect();
        Self {
            prefix,
            db: None,
            assets: None,
            shell_assets: None,
            brand: None,
            dark_mode: None,
            nav_items: Vec::new(),
            pages: Vec::new(),
            routes: Vec::new(),
            root_target: None,
            slugs: Vec::new(),
            search_handlers: HashMap::new(),
            frame_ancestors: Some(headers::DEFAULT_FRAME_ANCESTORS.to_string()),
            registration_errors,
            resource_checks: Vec::new(),
            uploads: None,
            served_dirs: Vec::new(),
            #[cfg(feature = "auth")]
            login_hint: None,
            #[cfg(feature = "auth")]
            auth: crate::auth::Auth::default(),
            #[cfg(not(feature = "auth"))]
            auth_disabled: false,
        }
    }

    /// Returns the mount prefix, e.g. `"/admin"`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Register the pooled `Db` on the `app_context`.
    pub fn app_context(mut self, db: Db) -> Self {
        self.db = Some(db);
        self
    }

    /// Install the [`Uploader`](crate::Uploader) every `FileUpload` stores
    /// through.
    ///
    /// One per panel, on the app context the way `Db` is, because where bytes
    /// live is an app-level dependency: an object store, a directory on disk, a
    /// CDN. Without it a `FileUpload` stores the sanitized client filename, so
    /// an app that never installs one is unaffected.
    pub fn uploads(mut self, uploader: impl crate::Uploader) -> Self {
        self.uploads = Some(crate::upload::InstalledUploader::new(uploader));
        self
    }

    /// Serve a directory of files from this panel's router.
    ///
    /// `path` is a route pattern ending in a catch-all (e.g.
    /// `"/uploads/{*file}"`), and `dir` the directory those URLs read from —
    /// see Topcoat's `DirectoryRoute` for the resolution rules. The path is
    /// **not** panel-relative: a served directory holds files a record points
    /// at (an upload store's output), which are not a page of the panel and
    /// must not move when the panel is mounted elsewhere. This is **public**:
    /// the auth gate covers only the panel prefix and `/_topcoat/runtime`
    /// (ADR-0013), so a served directory sits outside it and its URLs answer
    /// whoever asks, with no session — an app that needs protected files owns
    /// that route itself (ADR-0017).
    ///
    /// Every file the directory route serves carries `nosniff`, a sandboxing
    /// `Content-Security-Policy`, and `Content-Disposition: attachment` for
    /// anything but common raster images, audio/video and plain text, so an
    /// uploaded document cannot run script on the panel's origin. A
    /// 404 keeps Topcoat's `text/plain` error page; a 405 carries only `Allow`
    /// and an empty body. Neither carries user content.
    ///
    /// The Panel owns the [`Router`](topcoat::router::Router), so this is the app's only way to
    /// mount a route the framework does not own.
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

    /// Register the asset bundle used by the Panel's shell and UI components.
    ///
    /// Loading the bundle is an application concern; applications should fail
    /// loudly at startup when their generated bundle is missing rather than
    /// silently serving an unstyled shell.
    pub fn assets(mut self, assets: impl Into<AssetConfig>) -> Self {
        self.assets = Some(assets.into());
        self
    }

    /// Register the generated stylesheet and font linked by the default shell.
    ///
    /// `stylesheet` is normally `tailwind::stylesheet!()` and `font` is
    /// normally a `fontsource_font!(.., host: Asset)` value from the app.
    pub fn shell_assets(mut self, stylesheet: Asset, font: Font) -> Self {
        self.shell_assets = Some(ShellAssets { stylesheet, font });
        self
    }

    /// Declare a list-only `Resource` for this panel (the declarative seam,
    /// ADR-0008).
    ///
    /// Registers the resource's **list page** at `{prefix}/{slug}` (e.g.
    /// `Panel::new("admin").resource::<AuditResource>()` serves `/admin/audits`),
    /// its detail page, delete, bulk delete, and CSV export, and derives its
    /// [`NavigationItem`] from the same slug, so the sidebar and the router can
    /// never disagree. The panel root redirects to the first declared
    /// resource's list. Multiple calls compose. Sidebar order comes from the
    /// resource's [`Resource::navigation`] override, defaulting to declaration
    /// order (#165).
    ///
    /// A resource with a create or edit form registers with
    /// [`Self::form_resource`] instead: [`Panel::build`] refuses one registered
    /// here whose `can_create` or `editable()` is on.
    ///
    /// A duplicate slug or a slug that is not one URL segment
    /// is recorded here and reported by [`Panel::build`], which
    /// returns `Err` instead of panicking: two resources over one slug would
    /// shadow each other's routes, and a hostile `slug()` must not reach a
    /// route path or a response header.
    pub fn resource<R: Resource>(mut self) -> Self {
        if let Some(url) = self.register_common::<R>(resource_view::<R, ViewValues>) {
            self.resource_checks.push(check_list_resource::<R>);
            self.finish_registration::<R>(url);
        }
        self
    }

    /// Declare a `Resource` with a create and edit form for this panel.
    ///
    /// Registers everything [`Self::resource`] does, plus the create page, the
    /// edit page, and the relationship-options endpoint. The detail page reads
    /// the form's projection ([`RecordForm::hydrate`](crate::RecordForm::hydrate)),
    /// so the page and the form agree about what a field holds.
    ///
    /// [`Panel::build`] checks that the form's struct and its `Schema` agree:
    /// every control is bound by exactly one field and every field's key is a
    /// declared control; an optional control, or one inside a `Repeater`, binds
    /// a field with a blank answer; and a gated resource's form does not claim
    /// its tenant column.
    pub fn form_resource<R: FormResource>(mut self) -> Self {
        let Some(url) = self.register_common::<R>(resource_view::<R, FormValues>) else {
            return self;
        };
        self.resource_checks.push(check_form_resource::<R>);
        // Create page — GET renders form, POST handles submission.
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
        // Edit page — GET renders hydrated form, POST handles update.
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
        // Relationship option search — GET for searchable selects past the cap
        // `{list_url}/options?field=&q=` reusing the related
        // table's searchable columns, bounded, policy-checked.
        let options_url = format!("{url}/options");
        self.routes.push(RouteFn::new(
            http::Method::GET,
            route_path(&options_url),
            resource_options::<R>,
        ));
        self.finish_registration::<R>(url);
        self
    }

    /// The routes every resource registers: the list, the detail page (read
    /// through `detail`), delete, bulk delete, and export. Returns the list URL,
    /// or `None` when the slug was refused.
    fn register_common<R: Resource>(&mut self, detail: PageRenderFn) -> Option<String> {
        let slug = R::slug();
        if let Err(error) = validate_route_segment("Resource::slug", &slug) {
            self.registration_errors.push(error);
            return None;
        }
        if self.slugs.iter().any(|s| s == &slug) {
            self.registration_errors.push(format!(
                "duplicate resource slug '{slug}': each Resource needs a distinct slug (see Resource::slug)"
            ));
            return None;
        }
        self.slugs.push(slug);
        self.resource_checks.push(check_resource::<R>);
        let url = format!("{}/{}", self.prefix, R::slug());
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&url),
            resource_list::<R>,
        ));
        // Detail page — GET renders the record read-only. Registered
        // unconditionally, unlike the row link: registration runs before a
        // request exists, so `R::view(cx)` is not declarable here. The handler
        // 404s a resource that declares no view, which is the same answer as an
        // unknown id and costs one comparison.
        //
        // `RECORD_ROUTE_PARAM` shares its position with the literal `create`
        // segment: topcoat routes through `matchit`, which prefers a static
        // segment over a parameter one, so the create page keeps being reached
        // regardless of registration order.
        let detail_url = format!("{url}/{RECORD_ROUTE_PARAM}");
        self.pages.push(PageFn::new(
            http::Method::GET,
            route_path(&detail_url),
            detail,
        ));
        // Delete action — POST via row button (requires confirmation).
        let delete_url = format!("{url}/{RECORD_ROUTE_PARAM}/{DELETE_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&delete_url),
            resource_delete::<R>,
        ));
        // Bulk delete — POST with `ids` form field (comma-separated).
        let bulk_delete_url = format!("{url}/{BULK_DELETE_ROUTE_SEGMENT}");
        self.pages.push(PageFn::new(
            http::Method::POST,
            route_path(&bulk_delete_url),
            resource_bulk_delete::<R>,
        ));
        // CSV export — GET over the tenant-scoped export query + Table
        // filters/sort (ADR-0012).
        let export_url = format!("{url}/export");
        self.routes.push(RouteFn::new(
            http::Method::GET,
            route_path(&export_url),
            resource_export::<R>,
        ));
        Some(url)
    }

    /// The registration tail every resource shares: its live-search handler,
    /// the panel root, and its sidebar entry.
    fn finish_registration<R: Resource>(&mut self, url: String) {
        // Live-search handler: the slug-dispatched `#[shard]` cannot be generic
        // (inventory only discovers concrete fns), so each resource
        // monomorphizes its table loader here, keyed by list path.
        self.search_handlers
            .insert(url.clone(), search_handler_for::<R>());
        if self.root_target.is_none() {
            self.root_target = Some(url);
        }
        let nav_item = self.nav_item::<R>();
        self.nav_items.push(nav_item);
    }

    /// Set branding for the shell (header + sidebar). Additive `class` stays the only Shell seam.
    pub fn brand(mut self, brand: Brand) -> Self {
        self.brand = Some(brand);
        self
    }

    /// Set the `frame-ancestors` directive the panel sends.
    ///
    /// Defaults to `'self'`: the admin only frames itself, so a hostile page
    /// cannot clickjack it. Pass what your deployment needs — `"'self'
    /// https://intranet.example"` to allow an internal portal, or `"'none'"`
    /// to forbid framing outright.
    ///
    /// The layer only fills the gap, so an app that sets its own
    /// `Content-Security-Policy` (its own layer or route) keeps it. Use
    /// [`Self::without_frame_ancestors`] to send nothing at all and leave the
    /// decision to a proxy.
    pub fn frame_ancestors(mut self, ancestors: impl Into<String>) -> Self {
        self.frame_ancestors = Some(ancestors.into());
        self
    }

    /// Send no `frame-ancestors` directive: the escape hatch for
    /// deployments whose proxy owns the whole CSP.
    ///
    /// Off by default in the sense that nothing is *added* — the panel's
    /// `'self'` default is what this opts out of.
    pub fn without_frame_ancestors(mut self) -> Self {
        self.frame_ancestors = None;
        self
    }

    /// Enable dark mode toggle persistence (cookie + localStorage via `theme.js`).
    pub fn dark_mode(mut self, enabled: bool) -> Self {
        self.dark_mode = Some(enabled);
        self
    }

    /// Configure authentication (ADR-0013).
    ///
    /// The default is the shipped [`Auth::password`](crate::auth::Auth::password)
    /// over [`AdminUser`](crate::auth::AdminUser); swap in an app-owned
    /// authenticator with `Panel::auth(Auth::custom(..))`, or opt a public
    /// demo out explicitly with `Panel::auth(Auth::disabled())`.
    #[cfg(feature = "auth")]
    pub fn auth(mut self, auth: crate::auth::Auth) -> Self {
        self.auth = auth;
        self
    }

    /// Acknowledge that this build has no authentication (ADR-0013): with the
    /// `auth` feature off, [`build`](Self::build) refuses a panel that has not
    /// been handed [`Auth::disabled`](crate::Auth::disabled).
    #[cfg(not(feature = "auth"))]
    pub fn auth(mut self, _auth: crate::Auth) -> Self {
        self.auth_disabled = true;
        self
    }

    /// A muted line rendered under the login form, for demo credentials or
    /// deployment hints (e.g. `"Demo: admin@example.com / password"`).
    #[cfg(feature = "auth")]
    pub fn login_hint(mut self, hint: impl Into<String>) -> Self {
        self.login_hint = Some(hint.into());
        self
    }
}

impl Panel {
    /// Derive a [`NavigationItem`] for `R` using this panel's mount prefix.
    ///
    /// The one panel-aware navigation seam for a [`Resource`]:
    /// [`Panel::resource`](Self::resource) calls it, so a resource's
    /// [`Resource::navigation`] override reaches the sidebar instead of being
    /// dead API. The override owns the **label, ordering and
    /// grouping**; the panel owns the **URL**, because it is the only party
    /// that knows where the resource is mounted. Concretely: an explicit
    /// [`NavTarget::Url`] in `R::navigation()` is taken as returned, and only
    /// a [`NavTarget::Derived`] target — the default, which names no URL
    /// because [`Resource::navigation`] takes no prefix — is resolved to
    /// `{prefix}/{slug}`.
    ///
    /// So `Panel::new("backoffice")` yields `"/backoffice/{slug}"` — never a
    /// hard-coded `"/admin"` — for default and overridden items alike, a URL an
    /// override spelled out stays exactly as written, and an override's `order`
    /// decides sidebar order.
    pub(crate) fn nav_item<R: Resource>(&self) -> NavigationItem {
        R::navigation().resolved(&self.prefix, &R::slug())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::test_support::Dummy;

    /// GH #165: `Resource::navigation()` reaches the sidebar, and its order is
    /// what the rendered shell sorts by.
    #[test]
    fn panel_navigation_item_honours_override_order_with_prefix_adjusted_url() {
        use crate::resource::{NavigationItem, Resource};

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;

            fn navigation() -> NavigationItem {
                // The override cannot know the panel prefix, so it decorates
                // the default item: order here, URL from the panel.
                NavigationItem {
                    order: -1,
                    ..NavigationItem::for_resource::<Self>()
                }
            }
        }
        struct PlainResource;
        impl Resource for PlainResource {
            type Model = Dummy;

            fn slug() -> String {
                "plain".to_string()
            }
        }

        // Non-`/admin` panel + overridden navigation: the order survives and
        // the URL is resolved under this panel's prefix, not `/admin`.
        let panel = Panel::new("backoffice");
        let item = panel.nav_item::<DummyResource>();
        assert_eq!(item.order, -1);
        assert_eq!(item.label, "Dummies");
        assert_eq!(item.url(), Some("/backoffice/dummies"));
        // A resource without an override keeps the default (declaration order).
        assert_eq!(panel.nav_item::<PlainResource>().order, 0);
    }

    /// GH #165: a URL an override spells out is the author's, not the panel's —
    /// only a `Derived` target is resolved. A cross-panel link, a query view, or
    /// a custom path segment must survive untouched on a non-`/admin` panel,
    /// *including* one that looks like the origin mount.
    #[test]
    fn panel_navigation_item_keeps_urls_the_override_spells_out() {
        use crate::resource::{NavTarget, NavigationItem, Resource};

        struct DraftsResource;
        impl Resource for DraftsResource {
            type Model = Dummy;

            fn slug() -> String {
                "drafts".to_string()
            }

            fn navigation_label() -> String {
                "Drafts".to_string()
            }

            fn navigation() -> NavigationItem {
                // Order only, no URL: still the panel's to resolve.
                NavigationItem {
                    order: 3,
                    ..NavigationItem::for_resource::<Self>()
                }
            }
        }

        // `label`/`order` decorate the default item without touching its URL,
        // so the panel still owns (and resolves) the URL.
        let decorated = Panel::new("backoffice").nav_item::<DraftsResource>();
        assert_eq!(decorated.label, "Drafts");
        assert_eq!(decorated.order, 3);
        assert_eq!(decorated.url(), Some("/backoffice/drafts"));

        // A spelled-out URL is left alone — `/admin/posts?…` on a `/backoffice`
        // panel is a deliberate link, not a stale mount.
        struct ReportsResource;
        impl Resource for ReportsResource {
            type Model = Dummy;

            fn slug() -> String {
                "reports".to_string()
            }

            fn navigation() -> NavigationItem {
                NavigationItem::at("Draft posts", "/admin/posts?filters=status:draft")
            }
        }
        let spelled_out = Panel::new("backoffice").nav_item::<ReportsResource>();
        assert_eq!(spelled_out.url(), Some("/admin/posts?filters=status:draft"));
        assert_eq!(spelled_out.label, "Draft posts");
        assert!(matches!(spelled_out.target, NavTarget::Url(_)));

        // The same URL spelled out on the resource's *own* slug is the author's
        // too: `Derived` is what the Panel resolves, never a URL that happens to
        // match the origin mount.
        struct OwnSlugResource;
        impl Resource for OwnSlugResource {
            type Model = Dummy;

            fn slug() -> String {
                "users".to_string()
            }

            fn navigation() -> NavigationItem {
                NavigationItem::at("Users (legacy)", "/admin/users")
            }
        }
        let own_slug = Panel::new("backoffice").nav_item::<OwnSlugResource>();
        assert_eq!(own_slug.url(), Some("/admin/users"));
    }

    #[test]
    fn panel_navigation_item_respects_prefix() {
        use crate::resource::Resource;

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;
        }

        let panel = Panel::new("backoffice");
        let item = panel.nav_item::<DummyResource>();
        // Label: pluralized model name ("Dummy" → "Dummies"); URL: prefix +
        // resource slug ("DummyResource" → "dummies"), resolved by the panel.
        assert_eq!(item.label, "Dummies");
        assert_eq!(item.url(), Some("/backoffice/dummies"));

        let default = Panel::new("admin").nav_item::<DummyResource>();
        assert_eq!(default.url(), Some("/admin/dummies"));
        // Mount normalisation is `Panel::new`'s (slashes trimmed, `/admin` when
        // empty), and the resolved URL follows it.
        let slashed = Panel::new("/backoffice/").nav_item::<DummyResource>();
        assert_eq!(slashed.url(), Some("/backoffice/dummies"));
        let bare = Panel::new("").nav_item::<DummyResource>();
        assert_eq!(bare.url(), Some("/admin/dummies"));
    }

    #[test]
    fn panel_navigation_items_are_distinct_for_multiple_resources() {
        use crate::resource::Resource;

        struct UserResource;
        impl Resource for UserResource {
            type Model = Dummy;
        }
        struct CategoryResource;
        impl Resource for CategoryResource {
            type Model = Dummy;

            fn slug() -> String {
                "categories".to_string()
            }

            fn navigation_label() -> String {
                "Categories".to_string()
            }
        }

        let panel = Panel::new("admin");
        let users = panel.nav_item::<UserResource>();
        let categories = panel.nav_item::<CategoryResource>();
        assert_eq!(users.url(), Some("/admin/users"));
        assert_eq!(categories.url(), Some("/admin/categories"));
        assert_ne!(users.url(), categories.url());
    }

    #[test]
    fn panel_normalizes_prefix() {
        assert_eq!(Panel::new("admin").prefix(), "/admin");
        assert_eq!(Panel::new("/admin").prefix(), "/admin");
        assert_eq!(Panel::new("admin/").prefix(), "/admin");
        assert_eq!(Panel::new("/admin/").prefix(), "/admin");
        assert_eq!(Panel::new("").prefix(), "/admin");
    }

    /// GH #165: the override reaches *rendered* sidebar order.
    /// Rendered on a non-`/admin` panel, so the same test also pins the URL
    /// half: the sidebar links under `/backoffice`, never the origin `/admin`.
    #[tokio::test]
    async fn panel_sidebar_renders_overridden_navigation_order_first() {
        use topcoat::{
            context::CxTestBuilder,
            view::{ViewExt, view},
        };

        use crate::resource::{NavigationItem, Resource};

        struct PinnedResource;
        impl Resource for PinnedResource {
            type Model = Dummy;

            fn slug() -> String {
                "pinned".to_string()
            }

            fn navigation() -> NavigationItem {
                // GH #165 regression shape: an override that only sets order.
                // Before the fix the sidebar kept declaration order and the
                // resource's `order: -1` had no effect at all.
                NavigationItem {
                    order: -1,
                    ..NavigationItem::for_resource::<Self>()
                }
            }
        }
        struct OtherResource;
        impl Resource for OtherResource {
            type Model = Dummy;

            fn slug() -> String {
                "other".to_string()
            }

            fn navigation_label() -> String {
                "Other".to_string()
            }
        }

        // `PinnedResource` is declared last, so only the override can move it up.
        let panel = Panel::new("backoffice");
        let nav_items = vec![
            panel.nav_item::<OtherResource>(),
            panel.nav_item::<PinnedResource>(),
        ];
        let (parts, ()) = http::Request::builder()
            .uri("/backoffice/other")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new().request_context(parts).build();
        let cx_ref = &cx;
        let slot = view! { cx_ref => "hello" }.boxed().into();
        let html = Panel::render_shell(&cx, &nav_items, "/backoffice/other", slot, None)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        let pinned_at = html
            .find("/backoffice/pinned")
            .unwrap_or_else(|| panic!("pinned item must link under the panel prefix, got {html}"));
        let other_at = html.find("/backoffice/other").expect("other item renders");
        assert!(
            !html.contains("/admin/pinned"),
            "navigation must not link at the origin mount, got {html}"
        );
        assert!(
            pinned_at < other_at,
            "an overridden order: -1 must render first, got {html}"
        );
    }
}
