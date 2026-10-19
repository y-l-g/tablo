//! Mounting a panel: [`RouterBuilderPanelExt::panel`], the declaration
//! checks it runs, and the route-path helpers.

use std::sync::Arc;

use toasty::Db;
use topcoat::{
    Result,
    asset::AssetConfig,
    context::Cx,
    cookie::RouterBuilderCookieExt,
    router::{
        Body, LayoutFn, Methods, PageFn, Path, RouteFn, RouteFuture, RouterBuilder,
        RouterBuilderDirectoryExt, error::redirect,
    },
    runtime::{PrefetchMode, RouterBuilderRuntimeExt, RuntimeSetup},
    session::{RouterBuilderSessionExt, SessionConfig},
};

use super::{
    Panel, Root, error_page,
    forms::MAX_FORM_BYTES,
    headers,
    register::Registry,
    state::{CurrentPanel, PanelState, Panels, current, under_prefix},
};
use crate::{
    DeclarationError, DeclarationErrorKind, MountError, Site,
    auth::{PanelGate, RuntimeGate, SESSION_LIFETIME},
    declaration::segment_fault,
    resource::{MountScope, Mounts},
    tenancy::TenantSource,
    toasty_compat::model::AppSchema,
    topcoat_compat::RUNTIME_PREFIX,
};

/// Mounts a [`Panel`] on a router the app owns.
///
/// ```rust,no_run
/// # use tablo_core::{Auth, NoForm, Panel, PanelUser, Resource, RouterBuilderPanelExt, auth::Authenticator};
/// # use topcoat::{context::Cx, router::{Router, RouterBuilderDiscoverExt}};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, total: i64 }
/// # struct UserResource;
/// # impl Resource for UserResource { type Model = User; type Form = NoForm<User>; }
/// # struct OrderResource;
/// # impl Resource for OrderResource { type Model = Order; type Form = NoForm<Order>; }
/// # struct Member;
/// # impl PanelUser for Member {
/// #     fn user_id(&self) -> String { String::new() }
/// #     fn display_name(&self) -> &str { "" }
/// # }
/// # struct Members;
/// # impl Authenticator for Members {
/// #     type User = Member;
/// #     async fn verify(&self, _: &Cx, _: &str, _: &str) -> topcoat::Result<Option<Member>> { Ok(None) }
/// #     async fn find_by_id(&self, _: &Cx, _: &str) -> topcoat::Result<Option<Member>> { Ok(None) }
/// # }
/// # fn main() -> topcoat::Result<()> {
/// # let db: toasty::Db = todo!();
/// let router = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .panel(Panel::new("portal").auth(Auth::custom(Members)).resource::<OrderResource>())?
///     .build();
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
pub trait RouterBuilderPanelExt: Sized {
    /// Mounts `panel` at its prefix with its routes, shell layout, and gating layers.
    fn panel(self, panel: Panel) -> Result<Self>;

    /// The panel mounted at `prefix`, for code that runs outside a request; `None` when no panel
    /// is mounted there. `prefix` is spelled as for [`Panel::new`]: `"admin"` and `"/admin/"`
    /// name the same panel.
    fn panel_handle(&self, prefix: &str) -> Option<PanelHandle>;
}

impl RouterBuilderPanelExt for RouterBuilder {
    fn panel(self, panel: Panel) -> Result<Self> {
        panel.mount(self)
    }

    fn panel_handle(&self, prefix: &str) -> Option<PanelHandle> {
        let panel = self
            .get_app_context::<Panels>()?
            .by_prefix(&super::normalize_prefix(prefix))?;
        Some(PanelHandle {
            db: self.get_app_context::<Db>()?.clone(),
            mounts: Arc::clone(&panel.mounts),
            panel: Some(Arc::clone(panel)),
        })
    }
}

/// A panel's mounted resources and its database, which a background job or a test keeps to build
/// a context per run without declaring the panel again. A router's mounted panel gives it through
/// [`panel_handle`](RouterBuilderPanelExt::panel_handle), a panel no router mounts through
/// [`Panel::handle`]. Cloning it is cheap.
///
/// ```rust,no_run
/// # use tablo_core::{NoForm, Panel, Resource, RouterBuilderPanelExt, Tenant, scoped_query};
/// # use topcoat::router::{Router, RouterBuilderDiscoverExt};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// # struct PostResource;
/// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
/// # async fn run(db: toasty::Db, tenant: uuid::Uuid) -> topcoat::Result<()> {
/// let builder = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<PostResource>())?;
/// let admin = builder.panel_handle("admin").expect("mounted above");
/// let router = builder.build();
///
/// // In the job, once per run:
/// let cx = admin.context().with(Tenant(tenant));
/// let posts = scoped_query::<PostResource>(&cx)?;
/// # let _ = (router, posts);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct PanelHandle {
    db: Db,
    mounts: Arc<Mounts>,
    /// The mounted panel, for a handle a router gives; `None` for [`Panel::handle`]'s.
    panel: Option<Arc<PanelState>>,
}

impl PanelHandle {
    /// A context outside any request in which the panel's resources answer as the panel mounted
    /// them: what a job or a test passes to [`scoped_query`](crate::scoped_query),
    /// [`can`](crate::can), [`write_create`](crate::write_create) and the other entry points that
    /// answer from a mounted def. It holds the database and the resources, and no request,
    /// session or tenant. Add a tenant with `cx.with(Tenant(id))`.
    ///
    /// The context is one unit of work, as a request is: loads it memoizes stay cached for its
    /// lifetime, so a job builds one per run.
    pub fn context(&self) -> Cx {
        validation_cx(&self.db, &self.mounts)
    }

    /// `cx` answering as this panel, for a request outside every panel's prefix, such as a
    /// public page: [`scoped_query`](crate::scoped_query), [`can`](crate::can) and the other
    /// entry points that answer from a mounted def answer as this panel mounted its resources,
    /// and, for a handle a router gives, the [`url`](crate::url) helpers answer with its URLs.
    /// A request outside every prefix answers as no panel until it enters one, however many
    /// panels the router mounts. It runs no auth gate: the signed-in user stays unknown.
    ///
    /// ```rust,no_run
    /// # use tablo_core::{NoForm, PanelHandle, Resource, scoped_query};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
    /// async fn public_posts(cx: &Cx, admin: &PanelHandle) -> Result<Vec<Post>> {
    ///     let cx = admin.enter(cx);
    ///     Ok(scoped_query::<PostResource>(&cx)?
    ///         .exec(&mut tablo_core::db::db(&cx))
    ///         .await?)
    /// }
    /// ```
    pub fn enter(&self, cx: &Cx) -> Cx {
        match &self.panel {
            Some(panel) => cx.with(CurrentPanel(Arc::clone(panel))),
            None => cx.with(EnteredPanel(Arc::clone(&self.mounts))),
        }
    }
}

/// The resources [`PanelHandle::enter`] put in scope for a request outside every panel.
struct EnteredPanel(Arc<Mounts>);

/// The resources the request answers from: its own panel's, or the ones
/// [`PanelHandle::enter`] put in scope.
fn request_mounts(cx: &Cx) -> Option<&Mounts> {
    match current(cx) {
        Some(panel) => Some(&*panel.mounts),
        None => topcoat::context::try_request_context::<EnteredPanel>(cx)
            .map(|EnteredPanel(mounts)| &**mounts),
    }
}

impl Panel {
    /// The panel's resources declared and checked against `db` without a router: what a test, or
    /// a job that runs with no router, builds its contexts from with [`PanelHandle::context`].
    /// A job that runs beside a router takes the mounted panel's handle from
    /// [`panel_handle`](RouterBuilderPanelExt::panel_handle) instead.
    ///
    /// # Errors
    ///
    /// The declaration errors [`panel`](RouterBuilderPanelExt::panel) refuses the resources with.
    pub fn handle(self, db: &Db) -> Result<PanelHandle> {
        let Panel {
            prefix,
            registrations,
            configuration_errors,
            uploads,
            ..
        } = self;
        let mut registry = Registry::new(prefix.clone(), Some(AppSchema::of_db(db)));
        let (mounts, mut errors) = registry.register_all(registrations, configuration_errors);
        registry.check_all(db, &mounts, uploads.is_some(), &mut errors);
        if !errors.is_empty() {
            return Err(MountError::new(&prefix, errors).into());
        }
        Ok(PanelHandle {
            db: db.clone(),
            mounts,
            panel: None,
        })
    }

    fn mount(self, mut builder: RouterBuilder) -> Result<RouterBuilder> {
        let db = builder.get_app_context::<Db>().cloned();
        let has_mail = crate::auth::router_sends_mail(&builder);
        let mut registry = Registry::new(self.prefix.clone(), db.as_ref().map(AppSchema::of_db));
        let Panel {
            prefix,
            shell_assets,
            brand,
            dark_mode,
            layout,
            registrations,
            frame_ancestors,
            configuration_errors,
            uploads,
            served_dirs,
            login_hint,
            user_menu,
            auth,
            endpoints,
        } = self;
        let (mounts, mut errors) = registry.register_all(registrations, configuration_errors);
        let auth = crate::auth::or_default(auth).unwrap_or_else(|error| {
            errors.push(DeclarationError::panel(error));
            // Never served: the error refuses the mount below.
            crate::auth::Auth::disabled()
        });
        errors.extend(mount_errors(
            &builder,
            &prefix,
            shell_assets.is_some(),
            &served_dirs,
        ));
        match &db {
            Some(db) => {
                registry.check_all(db, &mounts, uploads.is_some(), &mut errors);
                let toggles = registry.resources.iter().any(|resource| resource.toggles);
                errors.extend(
                    crate::auth::mount::declaration_errors(db, &auth, toggles, has_mail)
                        .into_iter()
                        .map(DeclarationError::panel),
                );
            }
            None => errors.push(DeclarationError::panel(DeclarationErrorKind::MissingDb)),
        }
        if !errors.is_empty() {
            return Err(MountError::new(&prefix, errors).into());
        }
        if builder.get_app_context::<Panels>().is_none() {
            builder = install_shared(builder);
        }
        let Registry {
            urls,
            mounted_pages,
            nav_items,
            pages,
            routes,
            root,
            children,
            ..
        } = registry;
        let root_redirect = match root {
            Some(Root::Redirect(target)) => Some(target),
            Some(Root::Home) | None => None,
        };
        let served_paths = served_dirs.iter().map(|(path, _)| path.clone()).collect();
        let state = Arc::new(PanelState {
            prefix: prefix.clone(),
            nav_items,
            brand,
            dark_mode: dark_mode.unwrap_or(false),
            shell_assets,
            children,
            mounts,
            root_redirect: root_redirect.clone(),
            auth,
            login_hint,
            user_menu,
            uploads,
            served_paths,
            urls,
            mounted_pages,
            endpoints,
        });
        let prefix_path = route_path(&prefix);
        builder =
            builder.layer(topcoat::router::BodyLimit::max(MAX_FORM_BYTES).at(prefix_path.clone()));
        if let Some(directive) = frame_ancestors {
            builder = builder.layer(headers::FrameAncestors::new(directive, prefix.clone()));
        }
        // Registered last of the prefix's layers, so it runs first.
        builder = builder.layer(PanelGate::new(Arc::clone(&state)));
        // The auth form routes carry their own cap scoped by path.
        builder = crate::auth::mount::routes(builder, &prefix, &state.auth);
        builder = builder.layout(LayoutFn::new(
            prefix_path.clone(),
            layout.unwrap_or(Panel::layout_shell),
        ));
        // A failed page load shows the error page inside the shell, and a path no route serves
        // reaches the gate to fail there.
        builder = builder
            .page(PageFn::new(
                http::Method::GET,
                route_path(&format!("{prefix}/{}", error_page::ERROR_SEGMENT)),
                error_page::error_page,
            ))
            .route(RouteFn::new(
                Methods::Any,
                route_path(&format!("{prefix}/{{*rest}}")),
                error_page::unmatched,
            ));
        for (path, dir) in served_dirs {
            builder = builder
                .layer(headers::ServedFileHeaders::new(&path))
                .serve_dir(route_path(&path), dir);
        }
        for page in pages {
            builder = builder.page(page);
        }
        for route in routes {
            builder = builder.route(route);
        }
        if root_redirect.is_some() {
            builder = builder.route(RouteFn::new(
                http::Method::GET,
                prefix_path,
                panel_root_redirect,
            ));
        }
        builder
            .get_app_context_mut::<Panels>()
            .expect("the shared panel state is installed above")
            .0
            .push(state);
        Ok(builder)
    }
}

impl Registry {
    /// Registers `registrations` and links their relations, returning the mounts and every error,
    /// `configuration_errors` first.
    fn register_all(
        &mut self,
        registrations: Vec<Box<dyn super::register::Registration>>,
        configuration_errors: Vec<DeclarationError>,
    ) -> (Arc<Mounts>, Vec<DeclarationError>) {
        for registration in registrations {
            registration.register(self);
        }
        self.link_relations();
        let mut errors = configuration_errors;
        errors.append(&mut self.errors);
        (Arc::new(std::mem::take(&mut self.mounts)), errors)
    }

    /// Checks every registered resource against `db` and whether the panel installs an
    /// uploader.
    fn check_all(
        &self,
        db: &Db,
        mounts: &Arc<Mounts>,
        has_uploader: bool,
        errors: &mut Vec<DeclarationError>,
    ) {
        let cx = validation_cx(db, mounts);
        for check in &self.page_checks {
            check(&cx, errors);
        }
        for registered in &self.resources {
            (registered.check)(&cx, errors);
            // Without an uploader a file field would accept a file and keep only its name.
            if !has_uploader {
                errors.extend(registered.file_fields.iter().map(|field| DeclarationError {
                    resource: Some(registered.name),
                    site: Site::Form,
                    kind: DeclarationErrorKind::FileFieldWithoutUploader {
                        field: field.clone(),
                    },
                }));
            }
        }
    }
}

/// What refuses a panel at `prefix` before its resources are checked.
fn mount_errors(
    builder: &RouterBuilder,
    prefix: &str,
    shell_assets: bool,
    served_dirs: &[(String, std::path::PathBuf)],
) -> Vec<DeclarationError> {
    let mut errors = Vec::new();
    let mut refuse = |kind| errors.push(DeclarationError::panel(kind));
    if shell_assets && builder.get_app_context::<AssetConfig>().is_none() {
        refuse(DeclarationErrorKind::ShellAssetsWithoutBundle);
    }
    if under_prefix(RUNTIME_PREFIX, prefix) || under_prefix(prefix, RUNTIME_PREFIX) {
        refuse(DeclarationErrorKind::PrefixOverlapsRuntime {
            prefix: prefix.to_string(),
        });
    }
    for (index, (path, _)) in served_dirs.iter().enumerate() {
        let root = served_root(path);
        if served_dirs[..index]
            .iter()
            .any(|(seen, _)| served_root(seen) == root)
        {
            refuse(DeclarationErrorKind::ServeDirTwice { path: path.clone() });
        }
    }
    if let Some(panels) = builder.get_app_context::<Panels>() {
        for other in &panels.0 {
            for (path, _) in served_dirs {
                if other
                    .served_paths
                    .iter()
                    .any(|served| served_root(served) == served_root(path))
                {
                    refuse(DeclarationErrorKind::ServeDirTaken {
                        path: path.clone(),
                        panel: other.prefix.clone(),
                    });
                }
            }
            if under_prefix(&other.prefix, prefix) || under_prefix(prefix, &other.prefix) {
                refuse(DeclarationErrorKind::PrefixOverlapsPanel {
                    other: other.prefix.clone(),
                });
            }
        }
    }
    errors
}

/// Installs what every panel on a router shares.
fn install_shared(mut builder: RouterBuilder) -> RouterBuilder {
    builder = builder.cookies();
    if builder.get_app_context::<SessionConfig>().is_none() {
        builder = builder.sessions(SessionConfig::builder().lifetime(SESSION_LIFETIME).build());
    }
    builder = builder
        .layer(RuntimeGate::new())
        .app_context(Panels::default())
        .app_context(MountScope(request_mounts))
        .app_context(TenantSource(crate::auth::session_tenant));
    // The runtime layer has no path, so a page re-run reaches the panel's layers already rewritten
    // to a `GET`.
    if builder.get_app_context::<RuntimeSetup>().is_none() {
        builder = builder.runtime();
    }
    if builder.get_app_context::<PrefetchMode>().is_none() {
        builder = builder.prefetch(PrefetchMode::Never);
    }
    builder
}

/// Redirects the panel root of a panel with no [`home`](Panel::home) page to the first declared
/// resource's list.
pub(crate) fn panel_root_redirect(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Re-checks the resolved user so a mis-mounted gate cannot leak the slug.
        crate::auth::guard(cx)?;
        let target = current(cx)
            .and_then(|panel| panel.root_redirect.clone())
            .ok_or_else(topcoat::router::error::not_found)?;
        Err(redirect(target).into())
    })
}

/// Reports whether `path` is a route pattern ending in a catch-all.
pub(super) fn is_directory_pattern(path: &str) -> bool {
    Path::from_str(path)
        .ok()
        .and_then(|parsed| parsed.segments().next_back())
        .is_some_and(|segment| segment.as_catch_all().is_some())
}

/// Validates one path segment a panel derives routes from, refusing anything that cannot serve as a
/// literal URL segment.
pub(super) fn validate_route_segment(
    item: &'static str,
    segment: &str,
) -> Result<(), DeclarationErrorKind> {
    match segment_fault(segment) {
        Some(fault) => Err(DeclarationErrorKind::InvalidSegment {
            item,
            segment: segment.to_string(),
            fault,
        }),
        None => Ok(()),
    }
}

/// Builds the context for the mount-time declaration checks and [`PanelHandle::context`] from `db`
/// and
/// the panel's `mounts`, with no request.
fn validation_cx(db: &Db, mounts: &Arc<Mounts>) -> Cx {
    let mut app_context = topcoat::context::AppContext::new();
    app_context.insert(db.clone());
    app_context.insert(Arc::clone(mounts));
    app_context.insert(MountScope(|cx| {
        topcoat::context::try_app_context::<Arc<Mounts>>(cx).map(|mounts| &**mounts)
    }));
    Cx::new(Arc::new(app_context))
}

/// A served directory's pattern without its catch-all's name: the router treats
/// `/uploads/{*file}` and `/uploads/{*path}` as one route.
fn served_root(path: &str) -> &str {
    path.rsplit_once("{*").map_or(path, |(root, _)| root)
}

/// Parses a panel route path, panicking on malformed input.
pub(crate) fn route_path(path: &str) -> topcoat::router::PathBuf {
    Path::from_str(path)
        .expect("panel route paths are well-formed")
        .to_owned()
}

#[cfg(test)]
mod tests;
