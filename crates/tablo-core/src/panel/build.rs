//! Mounting a panel: [`RouterBuilderPanelExt::panel`], the declaration
//! checks it runs, and the route-path helpers.

use std::sync::Arc;

use toasty::Db;
use topcoat::{
    Result,
    asset::AssetConfig,
    context::Cx,
    cookie::CookieLayer,
    metadata::{Metadata, Robots},
    router::{
        Body, LayoutFn, Methods, PageFn, Path, RouteFn, RouteFuture, RouterBuilder,
        RouterBuilderDirectoryExt, error::redirect,
    },
    runtime::{PrefetchMode, RuntimeLayer},
};

use super::{
    Panel, Root, error_page,
    files::served_root,
    headers::{self, ServedDir},
    host::PanelHost,
    register::Registry,
    shell::{LANGUAGE_SEGMENT, language_post},
    state::{PanelState, Panels, current, under_prefix},
};
use crate::{
    DeclarationError, DeclarationErrorKind, MountError, Site,
    auth::{AuthHost, PagePanelStamp, PanelGate, Realm, RuntimeGate},
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
/// #     fn can_access_panel(&self, _panel: &str) -> bool { true }
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
    /// cookies, the gate over Topcoat's runtime endpoints, and Topcoat's runtime layer with link
    /// prefetching off unless the router already set those up. A panel keeps its sessions in its
    /// own cookie, so the app's Topcoat sessions, if any, stay the app's. The runtime layer has no
    /// path, so mount panels after the app's own pathless layers: a page re-run must reach
    /// those layers already rewritten to a `GET`.
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
            catalog: Arc::clone(&panel.realm.catalog),
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
    fn mount(self, mut builder: RouterBuilder) -> Result<RouterBuilder> {
        let db = builder.try_context::<Db>().cloned();
        let services = crate::auth::mount::Services::of(&builder);
        let mut registry = Registry::new(self.prefix.clone(), db.as_ref().map(AppSchema::of_db));
        let Panel {
            prefix,
            shell_assets,
            brand,
            dark_mode,
            environment,
            layout,
            registrations,
            content_security_policy,
            content_security_policy_sources,
            strict_transport_security,
            configuration_errors,
            uploads,
            served_dirs,
            login_hint,
            user_menu,
            banners,
            auth,
            mut endpoints,
            notifications,
            time_zone,
            locales,
            translations,
            global_search,
            limits,
            commit_hooks,
            audit_log,
            loads,
        } = self;
        let (mounts, mut errors) =
            registry.register_all(registrations, audit_log, configuration_errors);
        let serves_tenants = serves_tenants(&auth, &mounts);
        errors.extend(registry.unoffered_auth_pages(&auth));
        errors.extend(mount_errors(
            &builder,
            &prefix,
            shell_assets.is_some(),
            &served_dirs,
        ));
        match &db {
            Some(db) => {
                registry.check_all(
                    db,
                    &mounts,
                    serves_tenants,
                    uploads.is_some(),
                    &limits,
                    &mut errors,
                );
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
                errors.extend(super::audit::declaration_error(db, &registry.urls));
            }
            None => errors.push(DeclarationError::panel(DeclarationErrorKind::MissingDb)),
        }
        let catalog = build_catalog(&locales, &translations, &mut errors);
        let panel_headers = match content_security_policy
            .resolve(&content_security_policy_sources)
            .and_then(|policy| {
                headers::PanelHeaders::new(
                    prefix.clone(),
                    policy.as_deref(),
                    strict_transport_security,
                )
            }) {
            Ok(layer) => Some(layer),
            Err(kind) => {
                errors.push(DeclarationError::panel(kind));
                None
            }
        };
        if !errors.is_empty() {
            return Err(MountError::new(&prefix, errors).into());
        }
        let catalog = Arc::new(catalog.expect("a catalog that failed to build fails the mount"));
        if builder.try_context::<Panels>().is_none() {
            builder = install_shared(builder);
        }
        let pages = super::area::settings_paths(&prefix, &registry.auth_pages);
        let Registry {
            urls,
            mounted_pages,
            nav_items,
            areas,
            searches,
            pages: mut page_routes,
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
            page_routes.push(PageFn::new(
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
        let (served_paths, signed_roots) = super::files::served_paths(&served_dirs);
        let state = Arc::new(PanelState {
            realm: Arc::new(Realm {
                prefix: prefix.clone(),
                auth,
                login_hint,
                time_zone,
                catalog,
                notifications,
                serves_tenants,
                pages,
                audit_log: urls.contains_key(&std::any::TypeId::of::<super::AuditLog>()),
            }),
            nav_items,
            areas,
            brand,
            dark_mode,
            environment,
            shell_assets,
            children,
            mounts,
            root_redirect: root_redirect.clone(),
            user_menu,
            banners,
            uploads,
            served_paths,
            signed_roots,
            urls,
            mounted_pages,
            endpoints,
            searches,
            limits,
            commit_hooks,
            loads,
        });
        let prefix_path = route_path(&prefix);
        builder = builder.layer(limits.body_cap(prefix_path.clone()));
        builder = builder
            .layer(panel_headers.expect("a policy the header codec rejects fails the mount"));
        // Registered last of the prefix's layers, so it runs first.
        builder = builder.layer(PanelGate::new(Arc::clone(&state.realm)));
        // The auth form routes carry their own cap scoped by path.
        builder = crate::auth::mount::routes(builder, &state.realm);
        // Only a panel with a language to switch to answers the language menu's post.
        if state.realm.catalog.locales().len() > 1 {
            builder = builder.route(RouteFn::new(
                http::Method::POST,
                route_path(&format!("{prefix}/{LANGUAGE_SEGMENT}")),
                language_post,
            ));
        }
        // Search engines skip the panel's pages, as the layout's metadata asks.
        builder = builder.layout(
            LayoutFn::new(prefix_path.clone(), layout.unwrap_or(Panel::layout_shell))
                .with(Metadata::new().robots(Robots::new().index(false).follow(false))),
        );
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
        for page in page_routes {
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
        audit_log: Option<super::audit::CustomizeAuditLog>,
        configuration_errors: Vec<DeclarationError>,
    ) -> (Arc<Mounts>, Vec<DeclarationError>) {
        if let Some(customize) = audit_log {
            self.audit_log = super::register::AuditLogList::Unplaced(customize);
        }
        for registration in registrations {
            registration.register(self);
        }
        self.place_unplaced_audit_log();
        self.link_relations();
        self.check_root();
        let mut errors = configuration_errors;
        errors.append(&mut self.errors);
        (Arc::new(std::mem::take(&mut self.mounts)), errors)
    }

    /// Checks every registered resource against `db`, whether the panel acts for tenants,
    /// whether it installs an uploader, and the body cap its `limits` set.
    fn check_all(
        &self,
        db: &Db,
        mounts: &Arc<Mounts>,
        serves_tenants: bool,
        has_uploader: bool,
        limits: &crate::Limits,
        errors: &mut Vec<DeclarationError>,
    ) {
        let mut cx = validation_cx(db, mounts);
        if serves_tenants {
            cx = cx.with(crate::resource::ServesTenants);
        }
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
            // The body cap refuses a larger file first, so the field's own rule would never.
            let cap = limits.body_bytes;
            errors.extend(
                registered
                    .file_caps
                    .iter()
                    .filter(|(_, max)| *max >= cap)
                    .map(|(field, max)| DeclarationError {
                        resource: Some(registered.name),
                        site: Site::Form,
                        kind: DeclarationErrorKind::FieldRule {
                            field: field.clone(),
                            fault: crate::FieldRuleFault::AboveFormCap { max: *max, cap },
                        },
                    }),
            );
        }
    }
}

/// Whether a panel with `auth` and `mounts` acts for tenants: it offers a tenant registration or
/// settings page, or mounts a resource over tenant-owned rows.
fn serves_tenants(auth: &crate::auth::AuthConfig, mounts: &Mounts) -> bool {
    auth.tenant_registration_page().is_some()
        || auth.tenant_editor().is_some()
        || mounts.has_tenant_models()
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
    builder = builder
        .layer(CookieLayer::new())
        .layer(RuntimeGate::new())
        .with(Panels::default())
        .with(AuthHost(&PanelHost))
        .with(MountScope(request_mounts))
        .with(TenantSource(crate::auth::session_tenant))
        .with(crate::values::ZoneSource(crate::auth::session_time_zone))
        .with(crate::schema::FileHref(super::url::file))
        .with(crate::page::PageBody(super::pages::declared_body))
        .with(crate::i18n::CatalogSource(crate::auth::session_catalog))
        .with(NotificationBus::new());
    // The runtime layer has no path, so a page re-run reaches the panel's layers already rewritten
    // to a `GET`. One the app installs before the panels sits inside this one and passes on what
    // this one already rewrote.
    builder = builder.layer(RuntimeLayer::new());
    if builder.try_context::<PrefetchMode>().is_none() {
        builder = builder.with(PrefetchMode::Never);
    }
    // The last pathless layer runs outermost, so this one sees the socket's handshake unless the
    // app installs the runtime layer after the panels. `RuntimeGate` refuses those sockets' runs.
    builder.layer(PagePanelStamp)
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

/// The panel's catalog over `locales` and `translations`; `None`, with each fault pushed onto
/// `errors`, when it does not build.
fn build_catalog(
    locales: &[topcoat::l10n::Locale],
    translations: &[(topcoat::l10n::Locale, String)],
    errors: &mut Vec<DeclarationError>,
) -> Option<crate::i18n::Catalog> {
    crate::i18n::Catalog::new(locales, translations)
        .map_err(|faults| {
            errors.extend(
                faults
                    .into_iter()
                    .map(|fault| DeclarationError::panel(DeclarationErrorKind::Locale(fault))),
            );
        })
        .ok()
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
    Cx::new().with_shared(Arc::new(values))
}

#[cfg(test)]
mod tests;
