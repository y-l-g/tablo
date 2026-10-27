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
    headers::{self, ServedDir},
    host::PanelHost,
    register::Registry,
    state::{PanelState, Panels, current, under_prefix},
};
use crate::{
    DeclarationError, DeclarationErrorKind, MountError, Site,
    auth::{AuthHost, PanelGate, Realm, RuntimeGate, SESSION_LIFETIME},
    bus::NotificationBus,
    declaration::segment_fault,
    naming::route_path,
    resource::{MountScope, Mounts},
    tenancy::TenantSource,
    toasty_compat::model::AppSchema,
    topcoat_compat::RUNTIME_PREFIX,
};

mod handle;

pub use self::handle::PanelHandle;
use self::handle::request_mounts;

/// Mounts a [`Panel`] on a router the app owns.
///
/// ```rust,no_run
/// # use tablo_core::{Auth, Authenticator, NoForm, Panel, PanelUser, Resource, RouterBuilderPanelExt};
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
///     .with(db)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .panel(Panel::new("portal").auth(Auth::custom(Members)).resource::<OrderResource>())?
///     .build();
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
pub trait RouterBuilderPanelExt: Sized {
    /// Mounts `panel` at its prefix with its routes, shell layout, and gating layers.
    ///
    /// The panel reads the router's `Db` and asset bundle, so `.with(db)` and
    /// `.assets(..)` come first. The first panel mounted also installs what every panel shares:
    /// cookies, sessions unless the router already configures them, the gate over Topcoat's
    /// runtime endpoints, and Topcoat's runtime layer with link prefetching off unless the router
    /// already set those up. The runtime layer has no path, so mount panels after the app's own
    /// pathless layers: a page re-run must reach those layers already rewritten to a `GET`.
    ///
    /// # Errors
    ///
    /// A [`MountError`](crate::MountError) listing every declaration mistake, each a
    /// [`DeclarationErrorKind`](crate::DeclarationErrorKind).
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
            .try_context::<Panels>()?
            .by_prefix(&crate::naming::normalize_prefix(prefix))?;
        Some(PanelHandle {
            db: self.try_context::<Db>()?.clone(),
            mounts: Arc::clone(&panel.mounts),
            panel: Some(Arc::clone(panel)),
            prefix: panel.realm.prefix.clone(),
            bus: self
                .try_context::<NotificationBus>()
                .cloned()
                .unwrap_or_else(NotificationBus::new),
        })
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
            prefix,
            bus: NotificationBus::new(),
        })
    }

    fn mount(self, mut builder: RouterBuilder) -> Result<RouterBuilder> {
        let db = builder.try_context::<Db>().cloned();
        let services = crate::auth::mount::Services {
            mail: builder.try_context::<topcoat::mail::MailConfig>().is_some(),
            cookie_key: builder.try_context::<topcoat::cookie::Key>().is_some(),
        };
        let mut registry = Registry::new(self.prefix.clone(), db.as_ref().map(AppSchema::of_db));
        let Panel {
            prefix,
            shell_assets,
            brand,
            dark_mode,
            layout,
            registrations,
            content_security_policy,
            strict_transport_security,
            configuration_errors,
            uploads,
            served_dirs,
            login_hint,
            user_menu,
            auth,
            mut endpoints,
            notifications,
            time_zone,
            locales,
            translations,
            global_search,
        } = self;
        let (mounts, mut errors) = registry.register_all(registrations, configuration_errors);
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
                    crate::auth::mount::declaration_errors(
                        db,
                        &auth,
                        toggles,
                        notifications,
                        services,
                    )
                    .into_iter()
                    .map(DeclarationError::panel),
                );
            }
            None => errors.push(DeclarationError::panel(DeclarationErrorKind::MissingDb)),
        }
        let catalog = match crate::i18n::Catalog::new(&locales, &translations) {
            Ok(catalog) => Some(catalog),
            Err(faults) => {
                errors.extend(
                    faults
                        .into_iter()
                        .map(|fault| DeclarationError::panel(DeclarationErrorKind::Locale(fault))),
                );
                None
            }
        };
        let panel_headers = match headers::PanelHeaders::new(
            prefix.clone(),
            content_security_policy.resolve().as_deref(),
            strict_transport_security,
        ) {
            Ok(layer) => Some(layer),
            Err(kind) => {
                errors.push(DeclarationError::panel(kind));
                None
            }
        };
        if !errors.is_empty() {
            return Err(MountError::new(&prefix, errors).into());
        }
        let catalog = catalog.expect("a catalog that failed to build fails the mount");
        if builder.try_context::<Panels>().is_none() {
            builder = install_shared(builder);
        }
        let Registry {
            urls,
            mounted_pages,
            nav_items,
            searches,
            mut pages,
            routes,
            root,
            children,
            ..
        } = registry;
        let searches = if global_search {
            super::search::in_sidebar_order(searches, &nav_items)
        } else {
            Vec::new()
        };
        if !searches.is_empty() {
            pages.push(PageFn::new(
                http::Method::GET,
                route_path(&format!("{prefix}/{}", super::search::SEARCH_SEGMENT)),
                super::search::search_page,
            ));
            endpoints.push(super::search::shard_path());
        }
        let root_redirect = match root {
            Some(Root::Redirect(target)) => Some(target),
            Some(Root::Home) | None => None,
        };
        let served_paths = served_dirs
            .iter()
            .map(|served| served.path.clone())
            .collect();
        let signed_roots = served_dirs
            .iter()
            .filter(|served| served.signed)
            .map(|served| served_root(&served.path).to_owned())
            .collect();
        let serves_tenants = auth.tenant_registration_page().is_some()
            || auth.tenant_editor().is_some()
            || mounts.has_tenant_models();
        let state = Arc::new(PanelState {
            realm: Arc::new(Realm {
                prefix: prefix.clone(),
                auth,
                login_hint,
                time_zone,
                catalog,
                notifications,
                serves_tenants,
            }),
            nav_items,
            brand,
            dark_mode: dark_mode.unwrap_or(false),
            shell_assets,
            children,
            mounts,
            root_redirect: root_redirect.clone(),
            user_menu,
            uploads,
            served_paths,
            signed_roots,
            urls,
            mounted_pages,
            endpoints,
            searches,
        });
        let prefix_path = route_path(&prefix);
        builder =
            builder.layer(topcoat::router::BodyLimit::max(MAX_FORM_BYTES).at(prefix_path.clone()));
        builder = builder
            .layer(panel_headers.expect("a policy the header codec rejects fails the mount"));
        // Registered last of the prefix's layers, so it runs first.
        builder = builder.layer(PanelGate::new(Arc::clone(&state.realm)));
        // The auth form routes carry their own cap scoped by path.
        builder = crate::auth::mount::routes(builder, &state.realm);
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
        for served in served_dirs {
            builder = builder
                .layer(headers::ServedFileHeaders::new(&served.path, served.signed))
                .serve_dir(route_path(&served.path), served.dir);
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
        // The builder lends no mutable access: copy the list of handles and register it again.
        let mut panels = builder.context::<Panels>().clone();
        panels.0.push(state);
        Ok(builder.with(panels))
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
    served_dirs: &[headers::ServedDir],
) -> Vec<DeclarationError> {
    let mut errors = Vec::new();
    let mut refuse = |kind| errors.push(DeclarationError::panel(kind));
    if shell_assets && builder.try_context::<AssetConfig>().is_none() {
        refuse(DeclarationErrorKind::ShellAssetsWithoutBundle);
    }
    if under_prefix(RUNTIME_PREFIX, prefix) || under_prefix(prefix, RUNTIME_PREFIX) {
        refuse(DeclarationErrorKind::PrefixOverlapsRuntime {
            prefix: prefix.to_string(),
        });
    }
    for (index, served) in served_dirs.iter().enumerate() {
        let root = served_root(&served.path);
        if served_dirs[..index]
            .iter()
            .any(|seen| served_root(&seen.path) == root)
        {
            refuse(DeclarationErrorKind::ServeDirTwice {
                path: served.path.clone(),
            });
        }
    }
    if let Some(panels) = builder.try_context::<Panels>() {
        for other in &panels.0 {
            for ServedDir { path, .. } in served_dirs {
                if other
                    .served_paths
                    .iter()
                    .any(|served| served_root(served) == served_root(path))
                {
                    refuse(DeclarationErrorKind::ServeDirTaken {
                        path: path.clone(),
                        panel: other.realm.prefix.clone(),
                    });
                }
            }
            let other = &other.realm.prefix;
            if under_prefix(other, prefix) || under_prefix(prefix, other) {
                refuse(DeclarationErrorKind::PrefixOverlapsPanel {
                    other: other.clone(),
                });
            }
        }
    }
    errors
}

/// Installs what every panel on a router shares.
fn install_shared(mut builder: RouterBuilder) -> RouterBuilder {
    builder = builder.cookies();
    if builder.try_context::<SessionConfig>().is_none() {
        builder = builder.sessions(SessionConfig::builder().lifetime(SESSION_LIFETIME).build());
    }
    builder = builder
        .layer(RuntimeGate::new())
        .with(Panels::default())
        .with(AuthHost(&PanelHost))
        .with(MountScope(request_mounts))
        .with(TenantSource(crate::auth::session_tenant))
        .with(crate::values::ZoneSource(crate::auth::session_time_zone))
        .with(crate::schema::FileHref(super::url::file))
        .with(crate::i18n::CatalogSource(crate::auth::session_catalog))
        .with(NotificationBus::new());
    // The runtime layer has no path, so a page re-run reaches the panel's layers already rewritten
    // to a `GET`.
    if builder.try_context::<RuntimeSetup>().is_none() {
        builder = builder.runtime();
    }
    if builder.try_context::<PrefetchMode>().is_none() {
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
    let mut values = topcoat::context::ContextMap::new();
    values.insert(db.clone());
    values.insert(Arc::clone(mounts));
    values.insert(AuthHost(&PanelHost));
    values.insert(MountScope(|cx| {
        topcoat::context::try_context::<Arc<Mounts>>(cx).map(|mounts| &**mounts)
    }));
    Cx::new(Arc::new(values))
}

/// A served directory's pattern without its catch-all's name: the router treats
/// `/uploads/{*file}` and `/uploads/{*path}` as one route.
pub(super) fn served_root(path: &str) -> &str {
    path.rsplit_once("{*").map_or(path, |(root, _)| root)
}

#[cfg(test)]
mod tests;
