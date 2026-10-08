//! Mounting a panel: [`RouterBuilderPanelExt::panel`], the declaration
//! checks it runs, and the route-path helpers.

use std::{collections::HashMap, sync::Arc};

use toasty::Db;
use topcoat::{
    Result,
    asset::AssetConfig,
    context::Cx,
    cookie::RouterBuilderCookieExt,
    router::{
        Body, LayoutFn, Path, RouteFn, RouteFuture, RouterBuilder, RouterBuilderDirectoryExt,
        error::redirect,
    },
    runtime::{PrefetchMode, RouterBuilderRuntimeExt, RuntimeSetup},
    session::{RouterBuilderSessionExt, SessionConfig},
};

use super::{
    Panel, Root,
    forms::MAX_FORM_BYTES,
    headers,
    register::Registry,
    state::{PanelState, Panels, current, under_prefix},
};
use crate::{
    ActionInputFault, DeclarationError, DeclarationErrorKind, MountError, Site,
    auth::{PanelGate, RuntimeGate, SESSION_LIFETIME},
    declaration::segment_fault,
    form::RecordForm,
    policy::Ability,
    resource::{MountScope, Mounted, Mounts, Resource, require_mounted},
    tenancy::TenantSource,
    toasty_compat::model::{self, AppSchema},
    topcoat_compat::RUNTIME_PREFIX,
};

/// Mounts a [`Panel`] on a router the app owns.
///
/// ```rust,no_run
/// # use tablo_core::{Auth, NoForm, Panel, PanelUser, Resource, RouterBuilderPanelExt, auth::Authenticator};
/// # use topcoat::{context::Cx, router::{Router, RouterBuilderDiscoverExt}};
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, toasty::Model)]
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
}

impl RouterBuilderPanelExt for RouterBuilder {
    fn panel(self, panel: Panel) -> Result<Self> {
        panel.mount(self)
    }
}

impl Panel {
    /// A context outside any request in which the panel's resources answer as it mounts them:
    /// what a background job or a test passes to [`scoped_query`](crate::scoped_query),
    /// [`can`](crate::can), [`write_create`](crate::write_create) and the other entry points that
    /// answer from a mounted def. It holds `db` and the panel's resources, and no request,
    /// session or tenant; add a tenant with `cx.with(Tenant(id))`.
    ///
    /// Like a request's, the context is one unit of work: loads it memoizes stay cached for its
    /// lifetime, so a job builds one per run.
    ///
    /// # Errors
    ///
    /// The declaration errors [`panel`](RouterBuilderPanelExt::panel) refuses the resources with.
    pub fn context(self, db: &Db) -> Result<Cx> {
        let Panel {
            prefix,
            registrations,
            configuration_errors,
            ..
        } = self;
        let mut registry = Registry::new(prefix.clone(), Some(AppSchema::of_db(db)));
        let (mounts, mut errors) = registry.register_all(registrations, configuration_errors);
        let cx = registry.check_all(db, &mounts, &mut errors);
        if !errors.is_empty() {
            return Err(MountError::new(&prefix, errors).into());
        }
        Ok(cx)
    }

    fn mount(self, mut builder: RouterBuilder) -> Result<RouterBuilder> {
        let db = builder.get_app_context::<Db>().cloned();
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
            auth,
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
                registry.check_all(db, &mounts, &mut errors);
                if let Err(kind) = crate::auth::check_models_registered(db, &auth) {
                    errors.push(DeclarationError::panel(kind));
                }
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
            uploads,
            served_paths,
            urls,
        });
        let prefix_path = route_path(&prefix);
        builder =
            builder.layer(topcoat::router::BodyLimit::max(MAX_FORM_BYTES).at(prefix_path.clone()));
        if let Some(directive) = frame_ancestors {
            builder = builder.layer(headers::FrameAncestors::new(directive, prefix.clone()));
        }
        // Registered last of the prefix's layers, so it runs first.
        builder = builder.layer(PanelGate::new(Arc::clone(&state)));
        // The login route carries its own cap scoped by path.
        if state.gates() {
            let login_path = route_path(&format!("{prefix}/login"));
            let logout_path = route_path(&format!("{prefix}/logout"));
            let tenant_path = route_path(&format!("{prefix}/tenant"));
            builder = builder
                .layer(
                    topcoat::router::BodyLimit::max(crate::auth::MAX_LOGIN_BYTES)
                        .at(login_path.clone()),
                )
                .route(RouteFn::new(
                    http::Method::GET,
                    login_path.clone(),
                    crate::auth::login_page,
                ))
                .route(RouteFn::new(
                    http::Method::POST,
                    login_path,
                    crate::auth::login_post,
                ))
                .route(RouteFn::new(
                    http::Method::POST,
                    logout_path,
                    crate::auth::logout_post,
                ))
                .route(RouteFn::new(
                    http::Method::POST,
                    tenant_path,
                    crate::auth::tenant_post,
                ));
        }
        builder = builder.layout(LayoutFn::new(
            prefix_path.clone(),
            layout.unwrap_or(Panel::layout_shell),
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

    /// Checks every registered resource against `db`, returning the context the checks ran in.
    fn check_all(&self, db: &Db, mounts: &Arc<Mounts>, errors: &mut Vec<DeclarationError>) -> Cx {
        let cx = validation_cx(db, mounts);
        for registered in &self.resources {
            (registered.check)(&cx, errors);
        }
        cx
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
        .app_context(MountScope(|cx| current(cx).map(|panel| &*panel.mounts)))
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

/// A resource's declaration check: monomorphized once per registered resource, run by
/// [`RouterBuilderPanelExt::panel`] with the app's values, the panel's mounts and no request.
pub(super) type ResourceCheck = fn(&Cx, &mut Vec<DeclarationError>);

/// Checks what a mounted resource promises before the panel serves it.
pub(super) fn check_resource<R: Resource>(cx: &Cx, errors: &mut Vec<DeclarationError>) {
    let Ok(declared) = require_mounted::<R>(cx) else {
        return;
    };
    let tenancy = &declared.tenancy;
    if let Some(Err(kind)) = tenancy.column_field() {
        errors.push(DeclarationError::of::<R>(Site::Tenancy, kind));
    }
    if tenancy.via_is_single() == Some(true) {
        errors.push(DeclarationError::of::<R>(
            Site::Tenancy,
            DeclarationErrorKind::TenancyViaOwnColumn,
        ));
    }
    let mut form_errors = declared.form.declaration_errors();
    form_errors.extend(declared.form.empty_choices());
    let form_is_sound = form_errors.is_empty();
    // A derived view mirrors the record form, whose mistakes the form already reports.
    let view_errors = if declared.declares_view {
        declared.view.declaration_errors()
    } else {
        Vec::new()
    };
    for (site, kinds) in [
        (Site::Table, declared.table.declaration_errors()),
        (Site::Form, form_errors),
        (Site::View, view_errors),
    ] {
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<R>(site.clone(), kind)),
        );
    }
    for (site, kinds) in [
        (Site::Table, declared.table.unavailable_sources(cx)),
        (Site::View, declared.view.unavailable_sources(cx)),
    ] {
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<R>(site.clone(), kind)),
        );
    }
    check_actions(cx, &declared, errors);
    check_form_declaration(cx, &declared, form_is_sound, errors);
}

/// Every custom action's name is distinct among the resource's actions: the routes dispatch by
/// it. Its input declares no misdeclared field, none the action's POST carries itself and no file
/// field, and an input with no field parses an empty submission.
/// [`ResourceDef::action`](crate::ResourceDef::action) checks each name is a route segment as
/// it compiles.
fn check_actions<R: Resource>(cx: &Cx, declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let mut seen = std::collections::HashSet::new();
    for action in declared.actions.entries() {
        if !seen.insert(action.name) {
            errors.push(DeclarationError::of::<R>(
                Site::Registration,
                DeclarationErrorKind::DuplicateAction { name: action.name },
            ));
        }
        let input = (action.input)();
        let mut kinds = input.declaration_errors();
        kinds.extend(input.empty_choices());
        let mut faults: Vec<ActionInputFault> = input
            .fields()
            .filter_map(|field| {
                let name = field.name().to_string();
                if crate::resource::RESERVED_KEYS.contains(&name.as_str()) {
                    Some(ActionInputFault::ReservedField(name))
                } else if field.is_file() {
                    Some(ActionInputFault::FileField(name))
                } else {
                    None
                }
            })
            .collect();
        if !action.takes_input && (action.parse_input)(cx, &HashMap::new()).is_err() {
            faults.push(ActionInputFault::RefusesEmpty);
        }
        kinds.extend(
            faults
                .into_iter()
                .map(|fault| DeclarationErrorKind::ActionInput {
                    action: action.name,
                    fault,
                }),
        );
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<R>(Site::Registration, kind)),
        );
    }
}

/// A mistake in `R`'s form.
fn form_error<R: Resource>(kind: DeclarationErrorKind) -> DeclarationError {
    DeclarationError::of::<R>(Site::Form, kind)
}

/// Checks a resource's form declaration against its record form.
fn check_form_declaration<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    form_is_sound: bool,
    errors: &mut Vec<DeclarationError>,
) {
    // A misdeclared form, such as one placing a control twice, would only echo its mistakes.
    if !form_is_sound {
        return;
    }
    if <R::Form as RecordForm>::HAS_FORM {
        check_form_inner(cx, declared, errors);
    } else if declared.can(cx, Ability::Create) {
        errors.push(form_error::<R>(DeclarationErrorKind::CreateWithoutForm));
    }
}

fn check_form_inner<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let (fields, form) = (declared.fields.as_slice(), &*declared.form);
    // The mount renders every field the form does not place, through `RecordForm::control`.
    for field in fields {
        for key in &field.keys {
            if !form.fields().any(|control| control.name() == key) {
                errors.push(form_error::<R>(DeclarationErrorKind::MissingControl {
                    field: field.name.to_string(),
                    key: key.clone(),
                }));
            }
        }
    }
    // The framework stamps the tenant column on create.
    if let Some(column) = tenant_column(declared)
        && let Some(field) = fields.iter().find(|field| field.keys.contains(&column))
    {
        errors.push(form_error::<R>(
            DeclarationErrorKind::FormClaimsTenantColumn {
                field: field.name.to_string(),
                column,
            },
        ));
    }
    for field in form.fields().filter(|field| {
        field
            .as_choice()
            .is_some_and(|choice| choice.has_composite_source())
    }) {
        errors.push(form_error::<R>(DeclarationErrorKind::CompositeKeyChoice {
            field: field.name().to_string(),
        }));
    }
    for field in form.fields() {
        if let Some(source) = field
            .as_choice()
            .and_then(|choice| choice.unavailable_source(cx))
        {
            errors.push(form_error::<R>(
                DeclarationErrorKind::UnregisteredOptionSource {
                    field: field.name().to_string(),
                    source,
                },
            ));
        }
    }
    if declared.tenancy.via_is_single() == Some(false) {
        check_via_foreign_keys(cx, declared, errors);
    }
    if declared.can(cx, Ability::Create) {
        check_create_columns(declared, errors);
    }
    let columns = model::fields::<R::Model>();
    for field in form.fields().filter(|field| field.is_unique()) {
        let name = field.name();
        let backed = columns
            .iter()
            .any(|column| column.name == name && column.unique);
        if !backed {
            errors.push(form_error::<R>(DeclarationErrorKind::UniqueWithoutIndex {
                field: name.to_string(),
            }));
        }
        if !field.is_required() && !field.is_nullable() {
            errors.push(form_error::<R>(DeclarationErrorKind::OptionalUnique {
                field: name.to_string(),
            }));
        }
    }
}

/// Names `R`'s own tenant column.
fn tenant_column<R: Resource>(declared: &Mounted<R>) -> Option<String> {
    declared
        .tenancy
        .column_field()
        .and_then(Result::ok)
        .map(|field| field.name.clone())
}

/// Checks that every non-nullable column a create needs has a writer.
fn check_create_columns<R: Resource>(declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let fields = &declared.fields;
    let mut create_columns = Vec::new();
    for column in &declared.create_columns {
        match column {
            Ok(name) => create_columns.push(name.as_str()),
            Err(kind) => errors.push(DeclarationError::of::<R>(Site::Registration, kind.clone())),
        }
    }
    let prefilled = model::prefilled_fields::<R::Model>();
    let tenant = tenant_column(declared);
    if let Some(column) = &tenant
        && create_columns.contains(&column.as_str())
    {
        errors.push(DeclarationError::of::<R>(
            Site::Registration,
            DeclarationErrorKind::CreateColumnsNameTenant {
                column: column.clone(),
            },
        ));
    }
    for field in &model::fields::<R::Model>() {
        let name = field.name.as_str();
        let filled = field.nullable
            || field.relation
            || prefilled.get(field.index).copied().unwrap_or(false)
            || tenant.as_deref() == Some(name)
            || fields.iter().any(|claim| claim.name == name)
            || create_columns.contains(&name);
        if !filled {
            errors.push(form_error::<R>(DeclarationErrorKind::UnwrittenColumn {
                column: name.to_string(),
            }));
        }
    }
}

/// Builds the context for the mount-time declaration checks and [`Panel::context`] from `db` and
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

/// A `Tenancy::via` resource inherits its tenant from the parent its foreign key names, so the
/// form must write that key through a relationship field over the parent's tenant-scoped
/// resource: the write re-checks only such a field's key against the request's tenant.
fn check_via_foreign_keys<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let form = &declared.form;
    // The `belongs_to` relation the `via` lens steps through first.
    let Some(model::BelongsTo {
        name: relation,
        target: parent,
        foreign_keys: keys,
    }) = declared
        .tenancy
        .via_hop()
        .and_then(model::belongs_to::<R::Model>)
    else {
        errors.push(DeclarationError::of::<R>(
            Site::Tenancy,
            DeclarationErrorKind::TenancyViaWithoutBelongsTo,
        ));
        return;
    };
    let unguarded: Vec<String> = keys
        .into_iter()
        .filter(|key| {
            !form.fields().any(|field| {
                field.name() == key
                    && field
                        .as_choice()
                        .and_then(|choice| choice.tenant_scoped_model(cx))
                        == Some(parent)
            })
        })
        .collect();
    if !unguarded.is_empty() {
        errors.push(form_error::<R>(DeclarationErrorKind::UnguardedForeignKey {
            relation,
            keys: unguarded,
        }));
    }
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
