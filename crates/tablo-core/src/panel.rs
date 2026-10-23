//! `Panel` — resources and pages under one prefix, which an app mounts into its router.

mod actions;
mod bar;
mod build;
mod detail;
pub(crate) mod error_page;
mod forms;
pub(crate) mod gate;
mod headers;
mod host;
mod input;
mod list;
pub(crate) mod pages;
mod register;
mod relations;
mod search;
mod shell;
pub(crate) mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod url;
mod write;

use std::path::PathBuf;

use topcoat::{asset::Asset, font::Font, router::LayoutRenderFn};

pub(crate) use self::bar::header_bar;
use self::{
    build::is_directory_pattern,
    register::{PageRegistration, Registration, ResourceRegistration},
    shell::ShellAssets,
};
pub use self::{
    build::{PanelHandle, RouterBuilderPanelExt},
    forms::MAX_IMPORT_ROWS,
    gate::{can_list, can_open},
    list::WiredTable,
    shell::Brand,
};
use crate::{
    DeclarationError, DeclarationErrorKind, Page, PageDef,
    resource::{Resource, ResourceDef},
};

/// Returns the panel's list table for `R`, with the request's row actions wired on, for a page
/// that owns its table; render it over the page's own query with
/// [`WiredTable::render`](WiredTable::render).
///
/// # Errors
///
/// A declaration error when the context's panel does not mount `R`.
pub fn wired_table<R: Resource>(
    cx: &topcoat::context::Cx,
) -> topcoat::Result<WiredTable<R::Model>> {
    let resource = crate::resource::require_mounted::<R>(cx)?;
    Ok(self::list::wire_table_actions(cx, &resource))
}

/// `R`'s sidebar entry in the context's panel, with its label, URL and icon; `None` when the
/// panel does not register `R`.
pub fn navigation_item<R: Resource>(cx: &topcoat::context::Cx) -> Option<crate::NavigationItem> {
    crate::resource::mounted::<R>(cx).map(|resource| resource.navigation.clone())
}

/// An admin panel: resources and pages under one prefix, framed by one shell
/// and gated by one [`Auth`](crate::auth::Auth).
///
/// The app owns the router and mounts the panel into it with
/// [`RouterBuilderPanelExt::panel`]:
///
/// ```rust,no_run
/// # use tablo_core::{NoForm, Panel, Resource, RouterBuilderPanelExt};
/// # use topcoat::router::{Router, RouterBuilderDiscoverExt};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # struct UserResource;
/// # impl Resource for UserResource {
/// #     type Model = User;
/// #     type Form = NoForm<User>;
/// # }
/// # fn main() -> topcoat::Result<()> {
/// # let db: toasty::Db = todo!();
/// let router = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .build();
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
///
/// Registering is declarative: mounting builds each resource's [`ResourceDef`] and checks every
/// slug, route and declaration before the panel serves anything.
///
/// Besides its resources' and pages' routes, a panel at `/admin` serves:
///
/// - `GET /admin`: the [home page](Self::home), or a redirect to the first registered resource's
///   list when there is none;
/// - `GET`, `POST /admin/login`, `POST /admin/logout` and `POST /admin/tenant`, the tenant
///   switcher, unless authentication is disabled;
/// - `/admin/register`, `/admin/profile`, `/admin/forgot-password` and `/admin/reset-password` when
///   [`Auth`](crate::auth::Auth) installs the feature each serves;
/// - `POST /admin/{page}/-/actions/{action}` for a page's header actions, and
///   `/admin/-/actions/{action}` for the home page's;
/// - `GET /admin/-/search?q=`, the [global search](Self::without_global_search)'s results page,
///   when the panel searches a resource;
/// - every other path under `/admin`, with any method, as a 404 behind the auth gate, so a visitor
///   signs in before learning a path serves nothing. The app's own routes under the prefix still
///   take precedence.
pub struct Panel {
    prefix: String,
    shell_assets: Option<ShellAssets>,
    brand: Option<Brand>,
    dark_mode: Option<bool>,
    /// Frames the panel's pages; `None` renders the shipped shell.
    layout: Option<LayoutRenderFn>,
    /// The resources and pages, in registration order.
    registrations: Vec<Box<dyn Registration>>,
    /// The `Content-Security-Policy` of every response under the prefix.
    content_security_policy: headers::Policy,
    /// `Strict-Transport-Security`'s `max-age`, when the panel sends it.
    strict_transport_security: Option<std::time::Duration>,
    /// Mistakes in the panel's own configuration, for mount to report.
    configuration_errors: Vec<DeclarationError>,
    /// Where file field bytes go; `None` refuses to mount a form with a file field.
    uploads: Option<crate::upload::InstalledUploader>,
    /// App-owned filesystem directories served with hardening headers.
    served_dirs: Vec<headers::ServedDir>,
    login_hint: Option<crate::Text>,
    /// The app's entries in the user menu, above Sign out.
    user_menu: Vec<crate::NavigationItem>,
    auth: crate::auth::Auth,
    /// The paths of the runtime endpoints that answer only this panel's sessions.
    endpoints: Vec<String>,
    /// Whether the topbar shows the bell over the user's stored notifications.
    notifications: bool,
    /// The zone a user without their own reads and enters wall-clock times in.
    time_zone: jiff::tz::TimeZone,
    /// The locales the panel offers, the default first.
    locales: Vec<String>,
    /// The app's catalogs, by locale, over Tablo's.
    translations: Vec<(String, String)>,
    /// Whether the topbar shows the global search box.
    global_search: bool,
}

impl Panel {
    /// Creates a `Panel` mounted at `prefix`, defaulting an empty prefix to `"/admin"`.
    pub fn new(prefix: impl Into<String>) -> Self {
        let prefix = crate::naming::normalize_prefix(&prefix.into());
        let configuration_errors = prefix
            .trim_matches('/')
            .split('/')
            .filter_map(|segment| build::validate_route_segment("panel prefix", segment).err())
            .map(DeclarationError::panel)
            .collect();
        Self {
            prefix,
            shell_assets: None,
            brand: None,
            dark_mode: None,
            layout: None,
            registrations: Vec::new(),
            content_security_policy: headers::Policy::Default,
            strict_transport_security: None,
            configuration_errors,
            uploads: None,
            served_dirs: Vec::new(),
            login_hint: None,
            user_menu: Vec::new(),
            auth: crate::auth::Auth::default(),
            endpoints: Vec::new(),
            notifications: false,
            time_zone: jiff::tz::TimeZone::UTC,
            locales: vec!["en".to_owned()],
            translations: Vec::new(),
            global_search: true,
        }
    }

    /// Returns the mount prefix, e.g. `"/admin"`.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Installs the [`Uploader`](crate::Uploader) this panel's file fields store through; without
    /// one a panel whose resource form declares a file field refuses to mount.
    pub fn uploads(mut self, uploader: impl crate::Uploader) -> Self {
        self.uploads = Some(crate::upload::InstalledUploader::new(uploader));
        self
    }

    /// Stores file fields' uploads in `dir` and serves them at `url_prefix` (`"/uploads"`), through
    /// a [`DirUploader`](crate::upload::DirUploader).
    ///
    /// ```no_run
    /// # use tablo_core::Panel;
    /// Panel::new("admin").uploads_dir("/uploads", "var/uploads");
    /// ```
    ///
    /// The directory answers only at a signed URL that expires, so a file's bare URL opens
    /// nothing and a leaked link stops working within two hours. The panel signs a file field's
    /// link, a [`FileColumn`](crate::FileColumn)'s and an [`ImageColumn`](crate::ImageColumn)'s
    /// thumbnail when it renders them, on a page whose tenant and policy let the user see the
    /// record; a page of the app's own signs a stored path with [`url::file`](crate::url::file).
    /// The key derives from the router's cookie `Key`, else from one drawn per process, so
    /// without a shared `Key` a link opens only on the instance that rendered it. Any other
    /// request answers 404. The files carry [`serve_dir`](Self::serve_dir)'s hardening headers.
    ///
    /// A file a record form stored for a submission that never saved is deleted after 24 hours:
    /// [`DirUploader::expire_pending`](crate::upload::DirUploader::expire_pending).
    pub fn uploads_dir(self, url_prefix: &str, dir: impl Into<PathBuf>) -> Self {
        self.uploads_dir_with(url_prefix, dir, |uploader| uploader)
    }

    /// [`uploads_dir`](Self::uploads_dir), with the [`DirUploader`](crate::upload::DirUploader)
    /// wrapped in an app's own [`Uploader`](crate::Uploader), such as one that checks the bytes
    /// before it stores them: the files still answer only at signed URLs, and pending ones still
    /// expire. The wrapper forwards [`keep`](crate::Uploader::keep), and
    /// [`delete`](crate::Uploader::delete) to delete what a write lets go.
    pub fn uploads_dir_with<U: crate::Uploader>(
        self,
        url_prefix: &str,
        dir: impl Into<PathBuf>,
        wrap: impl FnOnce(crate::upload::DirUploader) -> U,
    ) -> Self {
        let url_prefix = url_prefix.trim_end_matches('/');
        let dir = dir.into();
        self.serve(format!("{url_prefix}/{{*file}}"), dir.clone(), true)
            .uploads(wrap(
                crate::upload::DirUploader::new(url_prefix, dir)
                    .expire_pending(std::time::Duration::from_secs(24 * 3600)),
            ))
    }

    /// Serves the directory `dir` at route pattern `path`, which must end in a catch-all.
    ///
    /// The files share the panel's origin, so every file response carries hardening headers:
    ///
    /// - `X-Content-Type-Options: nosniff`;
    /// - a sandboxing `Content-Security-Policy`, which is fixed;
    /// - `Content-Disposition: attachment`, unless the file is a common image, audio or video type
    ///   or plain text, so an uploaded HTML or SVG file downloads instead of running script.
    ///
    /// Serve active documents from another origin. The directory is **public**: the auth gate does
    /// not cover it, unless `path` sits under the panel prefix, which the gate covers. Serve
    /// uploads with [`uploads_dir`](Self::uploads_dir) instead, whose URLs expire.
    pub fn serve_dir(self, path: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        self.serve(path.into(), dir.into(), false)
    }

    /// Serves `dir` at `path`, at signed URLs only when `signed`.
    fn serve(mut self, path: String, dir: PathBuf, signed: bool) -> Self {
        if !is_directory_pattern(&path) {
            self.configuration_errors.push(DeclarationError::panel(
                DeclarationErrorKind::ServeDirWithoutCatchAll { path: path.clone() },
            ));
        }
        self.served_dirs
            .push(headers::ServedDir { path, dir, signed });
        self
    }

    /// Registers the stylesheet and font the default shell links, resolved through the router's
    /// asset bundle.
    pub fn shell_assets(mut self, stylesheet: Asset, font: Font) -> Self {
        self.shell_assets = Some(ShellAssets { stylesheet, font });
        self
    }

    /// Registers the [`Resource`] `R` at `{prefix}/{slug}` with its routes and sidebar entry, as
    /// [`Resource::declare`] declares it.
    ///
    /// A resource with the slug `users`, on a panel at `/admin`, serves:
    ///
    /// | Method | Path | Serves |
    /// | --- | --- | --- |
    /// | `GET` | `/admin/users` | the list |
    /// | `GET` | `/admin/users/{id}` | the detail page; 404 when the resource's view is empty |
    /// | `POST` | `/admin/users/{id}/delete` | delete one record |
    /// | `POST` | `/admin/users/-/bulk-delete` | delete the selected records |
    /// | `GET` | `/admin/users/-/export` | the list as CSV |
    /// | `GET`, `POST` | `/admin/users/-/create` | the create form ¹ |
    /// | `GET`, `POST` | `/admin/users/{id}/edit` | the edit form ¹ |
    /// | `GET` | `/admin/users/-/options` | a searchable or dependent choice's options ¹ |
    /// | `POST` | `/admin/users/{id}/-/actions/{action}` | a record action on one record ² |
    /// | `POST` | `/admin/users/-/actions/{action}` | a header action, or a bulk action ³ |
    /// | `GET` | `/admin/users/-/actions/{action}/options` | an action input's options ² ³ |
    /// | `POST` | `/admin/users/{id}/-/relations/{relation}/-/actions/{action}` | attach, or detach the selection ⁴ |
    /// | `POST` | `/admin/users/{id}/-/relations/{relation}/{related}/-/actions/{action}` | detach one record ⁴ |
    /// | `GET` | `/admin/users/{id}/-/relations/{relation}/-/actions/{action}/options` | the attach dialog's options ⁴ |
    ///
    /// ¹ Only for a resource whose `Form` is a record form, not [`NoForm`](crate::NoForm).
    /// ² Only when an action is placed on a row, the detail page or the edit page.
    /// ³ Only when the resource has a header action, or an action placed on the bulk bar.
    /// ⁴ Only when the resource has a [many-to-many relation](crate::Relation::belongs_to_many).
    ///
    /// `{id}` is the record's primary key. The `-` segment keeps the list's own routes apart from
    /// a record's: a record whose key is `create` still has its detail page at
    /// `/admin/users/create`. Each route checks the resource's policy.
    pub fn resource<R: Resource>(self) -> Self {
        self.resource_with::<R>(|def| def)
    }

    /// Registers the [`Resource`] `R` as [`resource`](Self::resource) does, with `customize`
    /// adjusting its def for this panel only:
    ///
    /// ```rust
    /// # use tablo_core::{NoForm, Panel, ReadOnly, Resource};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// Panel::new("portal").resource_with::<PostResource>(|def| def.policy(ReadOnly));
    /// ```
    pub fn resource_with<R: Resource>(
        mut self,
        customize: impl FnOnce(ResourceDef<R>) -> ResourceDef<R> + Send + 'static,
    ) -> Self {
        self.registrations
            .push(Box::new(ResourceRegistration::<R>(Box::new(customize))));
        self
    }

    /// Declares a [`Page`] at `{prefix}/{slug}` with its sidebar entry, as [`Page::declare`]
    /// declares it; pages share the resources' slug namespace.
    pub fn page<P: Page>(self) -> Self {
        self.page_with::<P>(|def| def)
    }

    /// Declares the [`Page`] `P` as [`page`](Self::page) does, with `customize` adjusting its def
    /// for this panel only:
    ///
    /// ```rust
    /// # use tablo_core::{Page, Panel};
    /// # use topcoat::{Result, context::Cx, view::View};
    /// # struct ReportsPage;
    /// # impl Page for ReportsPage {
    /// #     async fn render(_cx: &Cx) -> Result<impl View> { Ok(()) }
    /// # }
    /// # fn is_manager(_cx: &Cx) -> bool { true }
    /// Panel::new("portal").page_with::<ReportsPage>(|def| def.access(is_manager));
    /// ```
    pub fn page_with<P: Page>(
        mut self,
        customize: impl FnOnce(PageDef) -> PageDef + Send + 'static,
    ) -> Self {
        self.registrations.push(Box::new(PageRegistration::<P> {
            home: false,
            customize: Box::new(customize),
            _page: std::marker::PhantomData,
        }));
        self
    }

    /// Declares the panel's home page at the panel prefix, replacing the redirect to the first
    /// resource's list. The page mounts with its def as [`Page::declare`] returns it.
    pub fn home<P: Page>(mut self) -> Self {
        self.registrations.push(Box::new(PageRegistration::<P> {
            home: true,
            customize: Box::new(|def| def),
            _page: std::marker::PhantomData,
        }));
        self
    }

    /// Frames the panel's pages with `render` instead of the shipped shell; `render` usually wraps
    /// [`Panel::layout_shell`].
    ///
    /// The panel registers its layout at its prefix itself, with or without this call: a
    /// `#[layout]` of the app's own there would nest a second document inside the first.
    pub fn layout(mut self, render: LayoutRenderFn) -> Self {
        self.layout = Some(render);
        self
    }

    /// Sets branding for the shell (sidebar header and the topbar below `md`) and for the sign-in
    /// and sign-up pages, above their card.
    pub fn brand(mut self, brand: Brand) -> Self {
        self.brand = Some(brand);
        self
    }

    /// Sets the theme a visitor who has not chosen one sees, dark when `true`.
    pub fn dark_mode(mut self, enabled: bool) -> Self {
        self.dark_mode = Some(enabled);
        self
    }

    /// Configures this panel's authentication, defaulting to password auth; sessions belong to the
    /// panel that signed them in.
    pub fn auth(mut self, auth: crate::auth::Auth) -> Self {
        self.auth = auth;
        self
    }

    /// Declares a shard or procedure the app serves on this panel's pages: a request to it
    /// answers as a request under the prefix does.
    ///
    /// Topcoat serves shards and procedures under its runtime path, outside every panel's
    /// prefix. A request to an endpoint no panel declares runs as the panel whose session sent
    /// it, whichever panel that is; a panel without sign-in has no session, so its request runs as
    /// no panel, and [`scoped_query`](crate::scoped_query) and the [`url`](crate::url) helpers
    /// find no resource. A declared one runs as this panel: a session another panel
    /// signed in is no session here, so it answers 401 when the panel requires sign-in, and
    /// [`auth::guard`](crate::auth::guard), [`auth::user`](crate::auth::user), the tenant and
    /// [`can_list`] answer for this panel. Several panels may declare the same
    /// endpoint; each of their sessions reaches it.
    ///
    /// The router still registers the endpoint, through discovery or `.route(..)`. A route
    /// outside Topcoat's runtime path refuses to mount: the panel's prefix or the app's own
    /// layers guard it.
    ///
    /// ```rust
    /// # use tablo_core::{Panel, auth};
    /// # use topcoat::{Result, context::Cx, runtime::shard, view::{View, view}};
    /// #[shard]
    /// async fn revenue(cx: &Cx) -> Result<impl View> {
    ///     auth::guard(cx)?;
    ///     Ok(view! { "…" })
    /// }
    ///
    /// Panel::new("admin").endpoint(revenue);
    /// ```
    pub fn endpoint(mut self, endpoint: impl topcoat::router::Route) -> Self {
        let path = endpoint.path().as_str().to_owned();
        if !state::under_prefix(crate::topcoat_compat::RUNTIME_PREFIX, &path) {
            self.configuration_errors.push(DeclarationError::panel(
                DeclarationErrorKind::EndpointOutsideRuntime { path: path.clone() },
            ));
        }
        self.endpoints.push(path);
        self
    }

    /// Adds `item` to the user menu in the sidebar's footer, above Sign out: a profile or a
    /// settings page. The item names its URL with
    /// [`NavigationItem::at`](crate::NavigationItem::at); its badge shows in the sidebar only.
    pub fn user_menu_item(mut self, item: crate::NavigationItem) -> Self {
        self.user_menu.push(item);
        self
    }

    /// Shows a bell in the topbar over the signed-in user's
    /// [stored notifications](crate::Notifications): an unread count, and a sheet
    /// that lists them newest first, to mark read or unread and remove. Once the page loads, the
    /// bell keeps updating over Topcoat's runtime connection, and a notification sent while the
    /// page is open also shows as a toast.
    ///
    /// The bell lists what was sent on this panel for the tenant the user acts for, and what was
    /// sent for no tenant; its procedures mark and remove only those, so an id it does not list
    /// changes nothing. An open bell checks the session again before it shows anything new, and
    /// [`auth::revoke_sessions_for_user`](crate::auth::revoke_sessions_for_user) stops the user's
    /// open bells at once. A send wakes the bells open on the process that sent it: behind a load
    /// balancer, a bell on another process shows the notification on its next page load.
    ///
    /// The panel needs auth and the [`StoredNotification`] model, which
    /// [`models!`](crate::models!) registers. Nothing deletes stored notifications on its own.
    ///
    /// ```rust
    /// # use tablo_core::Panel;
    /// Panel::new("admin").notifications();
    /// ```
    ///
    /// [`StoredNotification`]: crate::notification::StoredNotification
    pub fn notifications(mut self) -> Self {
        self.notifications = true;
        self
    }

    /// Sets the time zone a user whose [`PanelUser::time_zone`](crate::PanelUser::time_zone) names
    /// none reads and enters wall-clock times in, and that an unsigned request uses: UTC by
    /// default. See [`time_zone`](crate::time_zone).
    ///
    /// ```rust
    /// # use tablo_core::Panel;
    /// let zone = jiff::tz::TimeZone::get("Europe/Paris").expect("the zone database names it");
    /// Panel::new("admin").time_zone(zone);
    /// ```
    pub fn time_zone(mut self, zone: jiff::tz::TimeZone) -> Self {
        self.time_zone = zone;
        self
    }

    /// Sets the locales the panel offers, the default first: `["en"]` by default. Each request
    /// reads one, its [`locale`](crate::locale): the signed-in user's
    /// [`PanelUser::locale`](crate::PanelUser::locale), else the browser's best match, else the
    /// default.
    ///
    /// Tablo translates its own words into `en` and `fr`; a locale it does not translate needs the
    /// app's catalog, from [`translations`](Self::translations). The pseudo-locale `en-XA` spells
    /// every catalog's English with accented letters, so a word that reads plain on its page is one
    /// no catalog spells.
    ///
    /// ```rust
    /// # use tablo_core::Panel;
    /// Panel::new("admin").locales(["fr", "en"]);
    /// ```
    ///
    /// The panel refuses to mount an empty list, a tag that is not BCP 47, a repeated locale, and
    /// a locale no catalog translates.
    pub fn locales<I, S>(mut self, locales: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.locales = locales.into_iter().map(Into::into).collect();
        self
    }

    /// Adds the app's Fluent catalog for `locale`: the messages its [`Text`](crate::Text)s name,
    /// and any of Tablo's it spells otherwise.
    ///
    /// A message the request's locale leaves out reads from the default locale's catalogs, then
    /// from English; an English locale reads English alone. A catalog for a language, such as `fr`,
    /// serves its regions too: `fr-CA` reads it under its own.
    ///
    /// ```rust,ignore
    /// Panel::new("admin")
    ///     .locales(["fr", "en"])
    ///     .translations("fr", include_str!("../locales/fr.ftl"))
    ///     .translations("en", include_str!("../locales/en.ftl"));
    /// ```
    ///
    /// The panel refuses to mount a catalog that is not valid Fluent, and one for a locale it does
    /// not offer, `en` aside.
    pub fn translations(mut self, locale: impl Into<String>, catalog: impl Into<String>) -> Self {
        self.translations.push((locale.into(), catalog.into()));
        self
    }

    /// Leaves out the global search the panel otherwise shows: the box in its topbar and its
    /// results page.
    ///
    /// By default the topbar holds a search box over every resource the panel mounts, unless a
    /// resource opts out with
    /// [`ResourceDef::without_global_search`](crate::ResourceDef::without_global_search). As the
    /// user types, it lists up to 5 results of each resource, headed by the resource's plural
    /// label in the sidebar's order, and links the page at `{prefix}/-/search?q=`, which lists up
    /// to 25 of each and which Enter opens, with or without JavaScript. The term matches a record
    /// when each of its first 8 distinct words does;
    /// [`ResourceDef::global_search`](crate::ResourceDef::global_search) says what a word matches,
    /// which records show and what each opens.
    ///
    /// A resource is not searched when it has no searchable column and no title column, or when no
    /// result could open a page: its records have no URL, or it has neither a detail page nor a
    /// form. The box shows to a request that may list one of the searched resources, and a panel
    /// that searches none serves no results page.
    ///
    /// The box's results are a shard the panel declares as its [endpoint](Self::endpoint), so they
    /// answer for a panel without sign-in too. Topcoat serves it at one path for every panel, and
    /// runs it as the panel that signed the session in, else as the first such panel without
    /// sign-in. On any other panel, such as the second of two without sign-in, the box lists no
    /// results as the user types and links the results page alone.
    ///
    /// ```rust
    /// # use tablo_core::Panel;
    /// Panel::new("admin").without_global_search();
    /// ```
    pub fn without_global_search(mut self) -> Self {
        self.global_search = false;
        self
    }

    /// Renders a line under the sign-in card for demo credentials or deployment hints.
    pub fn login_hint(mut self, hint: impl Into<crate::Text>) -> Self {
        self.login_hint = Some(hint.into());
        self
    }
}

/// What the panel serves at its prefix.
enum Root {
    /// A redirect to the first declared resource's list.
    Redirect(String),
    /// The [`Panel::home`] page.
    Home,
}

#[cfg(test)]
mod tests;
