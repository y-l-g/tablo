//! Mounting a panel: [`RouterBuilderPanelExt::panel`], the declaration
//! checks it runs, and the route-path helpers.

use std::sync::Arc;

use toasty::{Db, schema::Model};
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
    search::{ShardPanel, TABLE_RELATION_SEARCH_PATH, TABLE_SEARCH_PATH},
    state::{PanelState, Panels, current, under_prefix},
};
use crate::{
    auth::{PanelGate, RUNTIME_PREFIX, RuntimeGate, SESSION_LIFETIME},
    error::TabloError,
    form::RecordForm,
    policy::{Ability, can},
    resource::{Declarations, Declared, Resource},
    schema::{DeclCx, Schema},
};

/// Mount a [`Panel`] on a router the app owns.
///
/// ```ignore
/// use tablo::prelude::*;
///
/// let router = Router::builder()
///     .discover()
///     .app_context(db)
///     .assets(bundle)
///     .panel(Panel::new("admin").resource::<UserResource>())?
///     .panel(Panel::new("portal").auth(Auth::custom(Members)).resource::<OrderResource>())?
///     .build();
/// ```
pub trait RouterBuilderPanelExt: Sized {
    /// Mount `panel` at its prefix: its resources' and pages' routes, its
    /// shell layout, its login and logout routes, and the layers that gate,
    /// size-limit and harden every request under the prefix.
    ///
    /// The router must already hold the `Db` (`.app_context(db)`) and, when
    /// the panel links [`shell_assets`](Panel::shell_assets), the asset bundle
    /// (`.assets(..)`). The first panel mounted also installs what every panel
    /// shares: cookies, sessions unless the router already configures them,
    /// the gate over Topcoat's runtime endpoints with the shard dispatch, and
    /// the runtime layer with prefetching off unless the router already set
    /// those up. The runtime
    /// layer has no path, so mount panels after the app's own pathless layers:
    /// a page re-run must reach them already rewritten to a `GET`.
    ///
    /// Each panel checks its resources' declarations here, so the handlers
    /// serve exactly the values checked.
    ///
    /// # Errors
    ///
    /// Reports what the declarative builders could only record, and what
    /// only the router can answer: a resource or page slug that is malformed,
    /// reserved or already held, a second home page, a malformed prefix or one
    /// that overlaps another panel's or Topcoat's runtime endpoints, a
    /// relation to a resource the panel does not register, a misdeclared
    /// resource, a missing `Db`, a `Db` missing the shipped auth models, or
    /// `shell_assets` without an asset bundle.
    /// Configuring a panel wrong is a boot failure, not a request-time panic,
    /// so it comes back as an error the caller can log or exit on.
    fn panel(self, panel: Panel) -> Result<Self>;
}

impl RouterBuilderPanelExt for RouterBuilder {
    fn panel(self, panel: Panel) -> Result<Self> {
        panel.mount(self)
    }
}

impl Panel {
    fn mount(self, mut builder: RouterBuilder) -> Result<RouterBuilder> {
        let errors = self.mount_errors(&builder);
        if !errors.is_empty() {
            return Err(self.refused(&errors));
        }
        let db = builder.get_app_context::<Db>().cloned().ok_or_else(|| {
            self.refused(&[
                "the router holds no Db: install it with `.app_context(db)` before \
                            mounting the panel"
                    .to_string(),
            ])
        })?;
        // Declaration checks: a resource whose table or form could never
        // render is a configuration error, and the declaration is knowable
        // here — waiting for the first request only moves the failure
        // somewhere less useful. Each check builds the resource's declarations
        // once and keeps them: the handlers serve exactly the values checked.
        let mut declarations = Declarations::default();
        if !self.resource_checks.is_empty() {
            let cx = validation_cx(&db);
            let dx = DeclCx::new(&db);
            let failures: Vec<String> = self
                .resource_checks
                .iter()
                .filter_map(|check| check(&cx, &dx, &mut declarations).err())
                .collect();
            if !failures.is_empty() {
                return Err(self.refused(&failures));
            }
        }
        if let Err(error) = crate::auth::check_models_registered(&db, &self.auth) {
            return Err(self.refused(&[error]));
        }
        if builder.get_app_context::<Panels>().is_none() {
            builder = install_shared(builder);
        }
        match builder.get_app_context_mut::<Declarations>() {
            Some(installed) => installed.extend(declarations),
            None => builder = builder.app_context(declarations),
        }
        let Panel {
            prefix,
            shell_assets,
            brand,
            dark_mode,
            nav_items,
            pages,
            routes,
            root,
            layout,
            urls,
            search_handlers,
            relation_handlers,
            frame_ancestors,
            uploads,
            served_dirs,
            login_hint,
            auth,
            ..
        } = self;
        let root_redirect = match root {
            Some(Root::Redirect(target)) => Some(target),
            Some(Root::Home) | None => None,
        };
        let state = Arc::new(PanelState {
            prefix: prefix.clone(),
            nav_items,
            brand,
            dark_mode: dark_mode.unwrap_or(false),
            shell_assets,
            search: search_handlers,
            relations: relation_handlers,
            root_redirect: root_redirect.clone(),
            auth,
            login_hint,
            uploads,
            urls,
        });
        let prefix_path = route_path(&prefix);
        // Form bodies (urlencoded buffered, multipart streamed) share one
        // cap: without this layer Topcoat's 2 MiB default would 413 uploads
        // the framework otherwise accepts.
        builder =
            builder.layer(topcoat::router::BodyLimit::max(MAX_FORM_BYTES).at(prefix_path.clone()));
        // Clickjacking hardening: a response anyone can frame is a threat on
        // every deployment, so the panel ships the directive itself and apps
        // that need framing opt out (or supply their own policy, which wins —
        // the layer only fills the gap).
        if let Some(directive) = frame_ancestors {
            builder = builder.layer(headers::FrameAncestors::new(directive, prefix.clone()));
        }
        // Registered last of the prefix's layers, so it runs first: every
        // other layer and handler under the prefix sees the panel.
        builder = builder.layer(PanelGate::new(Arc::clone(&state)));
        // Auth (ADR-0013): the login, logout and tenant-switch routes. A credential POST
        // carries no upload, so the login route gets its own cap, scoped by
        // path so it wins over the panel's form cap.
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
            // Files the panel serves share the app's origin, so each directory
            // route is wrapped in the hardening layer that makes them inert.
            // The same path scopes the layer to that route only.
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
        // Without a home page, the prefix serves a redirect to the first
        // resource's list so the mount point is never a dead URL; a home page
        // registered its own route at the prefix.
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

    /// What refuses this panel before anything is mounted.
    fn mount_errors(&self, builder: &RouterBuilder) -> Vec<String> {
        let mut errors = self.registration_errors.clone();
        errors.extend(self.relation_errors());
        if self.shell_assets.is_some() && builder.get_app_context::<AssetConfig>().is_none() {
            errors.push(
                "shell_assets need the router's asset bundle: install it with `.assets(..)` \
                 before mounting the panel"
                    .to_string(),
            );
        }
        if under_prefix(RUNTIME_PREFIX, &self.prefix) || under_prefix(&self.prefix, RUNTIME_PREFIX)
        {
            errors.push(format!(
                "prefix '{}' overlaps Topcoat's runtime endpoints at '{RUNTIME_PREFIX}'",
                self.prefix
            ));
        }
        if let Some(panels) = builder.get_app_context::<Panels>() {
            for other in &panels.0 {
                if under_prefix(&other.prefix, &self.prefix)
                    || under_prefix(&self.prefix, &other.prefix)
                {
                    errors.push(format!(
                        "prefix '{}' overlaps the panel mounted at '{}': each panel needs a \
                         prefix of its own",
                        self.prefix, other.prefix
                    ));
                }
            }
        }
        errors
    }

    /// The mount error naming this panel.
    fn refused(&self, errors: &[String]) -> topcoat::Error {
        TabloError::Declaration(format!("panel '{}': {}", self.prefix, errors.join("; "))).into()
    }

    /// Every relation must name a resource this panel registers — its table's
    /// row actions and create link go to that resource's routes — and name it
    /// once per owner, since the key prefixes the table's URL parameters.
    fn relation_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (owner, keys) in &self.relations {
            for (index, key) in keys.iter().enumerate() {
                if keys[..index].contains(key) {
                    errors.push(format!(
                        "resource `{owner}` declares two relations to `{key}`: each related \
                         resource is one relation"
                    ));
                } else if !self.resource_slugs.contains(key) {
                    errors.push(format!(
                        "resource `{owner}` relates to `{key}`, which this panel does not \
                         register: declare it with `Panel::resource`"
                    ));
                }
            }
        }
        errors
    }
}

/// What every panel on a router shares, installed by the first one mounted:
/// cookies and sessions, the gate over Topcoat's runtime endpoints and the
/// layers that tell a live table's shard which panel it re-renders for, the
/// runtime layer, and the [`Panels`] registry.
fn install_shared(mut builder: RouterBuilder) -> RouterBuilder {
    builder = builder.cookies();
    if builder.get_app_context::<SessionConfig>().is_none() {
        builder = builder.sessions(SessionConfig::builder().lifetime(SESSION_LIFETIME).build());
    }
    builder = builder
        .layer(RuntimeGate::new())
        .layer(ShardPanel::new(TABLE_SEARCH_PATH, 0))
        .layer(ShardPanel::new(TABLE_RELATION_SEARCH_PATH, 1))
        .app_context(Panels::default());
    // The runtime layer has no path: it runs outside every layer with one, so
    // a page re-run reaches the panel's layers already rewritten to a `GET`.
    // Panel links navigate through it without prefetching: a prefetch renders
    // the destination, list queries included, for a page the user may never
    // open.
    if builder.get_app_context::<RuntimeSetup>().is_none() {
        builder = builder.runtime();
    }
    if builder.get_app_context::<PrefetchMode>().is_none() {
        builder = builder.prefetch(PrefetchMode::Never);
    }
    builder
}

/// The panel root of a panel with no [`home`](Panel::home) page: a temporary
/// redirect to the first declared resource's list, so the mount point is never
/// a dead URL.
pub(crate) fn panel_root_redirect(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Defense in depth: every panel handler re-checks the
        // resolved user, so a missing or mis-mounted gate cannot leak the
        // first resource's slug via the redirect target.
        crate::auth::guard(cx)?;
        let target = current(cx)
            .and_then(|panel| panel.root_redirect.clone())
            .ok_or_else(topcoat::router::error::not_found)?;
        Err(redirect(target).into())
    })
}

/// Whether a path is a route pattern ending in a catch-all, which is the only
/// shape [`DirectoryRoute`](topcoat::router::DirectoryRoute) accepts.
///
/// Checked where the path is declared rather than where it is used: upstream
/// `serve_dir` panics on anything else, and mounting a panel reports instead
/// of panicking — but the path comes from the app, and it would panic first
/// in [`route_path`] (which refuses to spell a route it cannot parse) and then
/// inside `DirectoryRoute::new` (which needs the catch-all last). Asking both
/// conditions here turns a typo into a mount error instead of a panic during
/// the mount.
pub(super) fn is_directory_pattern(path: &str) -> bool {
    Path::from_str(path)
        .ok()
        .and_then(|parsed| parsed.segments().next_back())
        .is_some_and(|segment| segment.as_catch_all().is_some())
}

/// Validate one path segment a panel derives routes from: a resource's or a
/// page's `slug()`, or a segment of the panel prefix.
///
/// Both reach a route path and, through the panel, a response body. A hostile
/// value — quote, backslash, CR/LF, `..`, slash, URL punctuation, a route
/// pattern character — must fail at registration rather than at request time,
/// so this is the export filename sanitizer's rule tightened to what a URL
/// segment can be: the export drops the offending characters because it must
/// still produce a download, while a route has no meaningful fallback.
///
/// The route pattern characters that a literal segment cannot carry (`{`, `}`,
/// `(`, `)`) are rejected rather than escaped: `Path::from_str` treats
/// `{`/`(` as the start of a parameter or group segment, so a balanced pair
/// silently becomes a pattern and an unbalanced one panics [`route_path`]. `*`
/// stays accepted — it is a literal in a static segment — and the catch-all
/// spelling `{*name}` needs the `{` this rule already refuses.
pub(super) fn validate_route_segment(kind: &str, segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err(format!("{kind}: path segment must not be empty"));
    }
    if segment == "." || segment == ".." {
        return Err(format!(
            "{kind} '{segment}': a path segment may not be '.' or '..'"
        ));
    }
    if let Some(bad) = segment.chars().find(|c| {
        c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '"' | '\\' | '/' | '?' | '#' | '%' | '&' | '=' | '{' | '}' | '(' | ')'
            )
    }) {
        return Err(format!(
            "{kind} '{segment}': a path segment may not contain {bad:?} (quotes, backslashes, control characters, whitespace, URL punctuation and the route pattern characters '{{', '}}', '(' and ')' are rejected)"
        ));
    }
    Ok(())
}

/// A resource's declaration check: monomorphized once per declared resource
/// by [`Panel::resource`], run by [`RouterBuilderPanelExt::panel`] with the
/// app's values and no request.
pub(super) type ResourceCheck = fn(&Cx, &DeclCx, &mut Declarations) -> Result<(), String>;

/// What a declared resource must be able to promise before the panel serves it.
///
/// The trait defaults every method but `table`, so a resource that overrides
/// nothing else compiles and only fails when a user reaches the page that needs
/// the missing piece. The essentials that are *declarations* are checked here,
/// at build, and reported with the resource's type name: a
/// [`Tenancy::column`](crate::Tenancy::column) lens that names a field of the
/// model, what the table, the form and the view record as
/// misdeclared ([`Table::declaration_errors`](crate::Table::declaration_errors),
/// [`Schema::declaration_errors`]), the custom actions' names, and the
/// agreement between the resource's `Form` and its `form()` schema
/// ([`check_form_declaration`]). Runtime essentials (the record fns) keep their
/// loud failure.
///
/// The declarations are built once, here, and handed back for the panel to
/// serve: a request reads the values this check saw.
pub(super) fn check_resource<R: Resource>(
    cx: &Cx,
    dx: &DeclCx,
    declarations: &mut Declarations,
) -> Result<(), String> {
    // A tenant column that is not one field of the model would filter on, and
    // stamp, nothing: checked first because the tenancy governs every handler
    // this resource registers.
    if let Some(Err(error)) = R::tenancy().column_field() {
        return Err(format!(
            "resource `{}`'s `Tenancy::column` lens binds no column of `{}`: {error} — name a \
             UUID field of the model, or use `Tenancy::via` for a tenant reached through a relation",
            std::any::type_name::<R>(),
            std::any::type_name::<R::Model>(),
        ));
    }
    // A `via` over the model's own column stamps nothing, so creates would
    // fail or orphan: that shape is `column`.
    if R::tenancy().via_is_single() == Some(true) {
        return Err(format!(
            "resource `{}`'s `Tenancy::via` lens names one field of `{}` — use `Tenancy::column` \
             for the model's own tenant column, `Tenancy::via` for a tenant reached through a \
             relation",
            std::any::type_name::<R>(),
            std::any::type_name::<R::Model>(),
        ));
    }
    let declared = Declared::<R>::build(dx);
    let misdeclared: Vec<String> = [
        ("table", declared.table.declaration_errors()),
        ("form", declared.form.declaration_errors()),
        ("view", declared.view.declaration_errors()),
    ]
    .into_iter()
    .flat_map(|(part, errors)| {
        errors
            .into_iter()
            .map(move |error| format!("{part}: {error}"))
    })
    .collect();
    if !misdeclared.is_empty() {
        return Err(format!(
            "resource `{}` is misdeclared: {}",
            std::any::type_name::<R>(),
            misdeclared.join("; ")
        ));
    }
    check_actions::<R>()?;
    check_form_declaration::<R>(cx, dx, &declared)?;
    declarations.insert(Arc::new(declared));
    Ok(())
}

/// Every custom action's name is a route segment, distinct among the
/// resource's actions: the routes dispatch by it.
fn check_actions<R: Resource>() -> Result<(), String> {
    let actions = R::actions();
    let mut seen = std::collections::HashSet::new();
    for action in actions.entries() {
        validate_route_segment("Action::NAME", action.name)
            .map_err(|error| format!("resource `{}`: {error}", std::any::type_name::<R>()))?;
        if !seen.insert(action.name) {
            return Err(format!(
                "resource `{}` declares two actions named '{}': each needs a distinct `NAME`",
                std::any::type_name::<R>(),
                action.name
            ));
        }
    }
    Ok(())
}

/// A resource with a record form declares its schema and runs
/// [`check_form_inner`]. A resource whose form serves no pages
/// ([`RecordForm::HAS_FORM`] false) declares no schema, and a policy that
/// allows create would link to a page that does not exist.
fn check_form_declaration<R: Resource>(
    cx: &Cx,
    dx: &DeclCx,
    declared: &Declared<R>,
) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let form = std::any::type_name::<R::Form>();
    if <R::Form as RecordForm>::HAS_FORM {
        // A form with fields and an empty schema renders nothing to fill in;
        // the key check below would only name the first field.
        if declared.form.is_empty() && !<R::Form as RecordForm>::fields(dx).is_empty() {
            return Err(format!(
                "resource `{resource}` names record form `{form}`, but its `form()` declares no \
                 controls — drop the override to render the derived schema"
            ));
        }
        return check_form_inner::<R>(cx, dx, &declared.form);
    }
    if !declared.form.is_empty() {
        return Err(format!(
            "resource `{resource}` declares a form schema but its `Form`, `{form}`, serves no \
             form — name the record form in `type Form`"
        ));
    }
    if can::<R>(cx, Ability::Create) {
        return Err(format!(
            "resource `{resource}` allows create but has no form — name its record form in `type \
             Form` and declare `form()`"
        ));
    }
    Ok(())
}

fn check_form_inner<R: Resource>(cx: &Cx, dx: &DeclCx, form: &Schema) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let fields = <R::Form as RecordForm>::fields(dx);
    let controls = form.controls();
    // Key agreement, reported control-first: a control no field binds is the
    // direction that drops what the user typed.
    for control in &controls {
        let claims = fields
            .iter()
            .filter(|field| field.keys.contains(&control.name))
            .count();
        if claims == 0 {
            return Err(format!(
                "resource `{resource}` renders form control `{}` but no field of its record form \
                 binds it, so what the user types there is never written",
                control.name
            ));
        }
        if claims > 1 {
            return Err(format!(
                "resource `{resource}` binds form control `{}` from more than one record-form field",
                control.name
            ));
        }
    }
    for field in &fields {
        for key in &field.keys {
            if !controls.iter().any(|control| &control.name == key) {
                return Err(format!(
                    "resource `{resource}`'s record form field `{}` binds key `{key}`, but the form \
                     declares no control for it",
                    field.name
                ));
            }
        }
        // Blank agreement: an empty submission must resolve wherever the
        // schema lets one through. The discriminant is not asked — an empty one
        // reaches the read's fallback — and a variant group's payload only
        // where the group sits inside a `Repeater`, whose all-empty group skips
        // requiredness while the parse still reads the payload.
        if field.answers_blank {
            continue;
        }
        if let Some(control) = controls.iter().find(|control| {
            field.keys.contains(&control.name)
                && control.needs_answer()
                && (!control.required || control.in_repeater)
        }) {
            let place = if control.in_repeater {
                "sits inside a `Repeater`, so it may be posted empty"
            } else {
                "is optional"
            };
            return Err(format!(
                "resource `{resource}`'s form control `{}` {place}, but record form field `{}` has \
                 no blank answer — declare `#[form(blank = ..)]`, make the field an \
                 `Option`, or make the control required",
                control.name, field.name
            ));
        }
    }
    // Tenant ownership: the framework stamps a scoped resource's tenant column
    // on create; a form that claimed it would let the client choose.
    if let Some(column) = tenant_column::<R>()
        && let Some(field) = fields.iter().find(|field| field.keys.contains(&column))
    {
        return Err(format!(
            "resource `{resource}` is tenant-scoped, but record form field `{}` claims its tenant \
             column `{column}` — the framework stamps it on create; drop it from the form",
            field.name
        ));
    }
    // A `via` resource writes its parent key through the form, so without a
    // relationship field nothing re-checks it inside the write.
    if R::tenancy().via_is_single() == Some(false)
        && !form.fields().any(|field| {
            field
                .as_choice()
                .is_some_and(|choice| choice.is_relationship())
        })
    {
        return Err(format!(
            "resource `{resource}` uses `Tenancy::via` but its form declares no relationship \
             field — declare the parent key as a relationship field over the parent's resource"
        ));
    }
    // Column coverage: a create that leaves a non-nullable column unset fails
    // at the driver on every submit, with no field to point the user at.
    if can::<R>(cx, Ability::Create) {
        check_create_columns::<R>(&fields)?;
    }
    // `.unique()` is a promise the panel makes and the database has to keep
    // (item 3): the marker turns the app-side pre-check on, so a field
    // whose column carries no unique index makes the panel enforce a rule
    // nothing else does — a duplicate the check lets through, or a rule the
    // database never asked for. The declaration checks are the only place both
    // halves are reachable without a request, so the pair is refused here
    // rather than discovered by a user. It is checked whatever the policies say:
    // a `unique()` marker is wrong on a form the panel would not even serve.
    // `lens_field_unique` recognizes composite indexes too, which is what makes
    // `#[unique(tenant_id, email)]` — the tenant-scoped arrangement the panel
    // documents — pass.
    let model = R::Model::schema();
    let root = model.as_root_unwrap();
    for field in form.fields().filter(|field| field.is_unique()) {
        let name = field.name();
        // A bound lens always resolves, so a name with no field at all is a
        // mis-declared schema — but it is not worth a second error string: it
        // fails the same way, one message below.
        let backed = root
            .fields
            .iter()
            .filter(|field| field.name.app_unwrap() == name)
            .any(|field| crate::schema::lens_field_unique(field, root));
        if !backed {
            return Err(format!(
                "resource `{}` marks form field `{name}` unique, but `{}::{name}` carries no unique index — add `#[unique]` (or `#[unique(..)]`) to the column or drop `.unique()`, which would otherwise check a rule the database does not enforce",
                std::any::type_name::<R>(),
                std::any::type_name::<R::Model>()
            ));
        }
    }
    Ok(())
}

/// The name of `R`'s own tenant column, which the framework stamps on create:
/// a [`Tenancy::column`](crate::Tenancy::column) that binds one.
fn tenant_column<R: Resource>() -> Option<String> {
    R::tenancy()
        .column_field()
        .and_then(Result::ok)
        .map(|field| field.name.clone())
}

/// Every non-nullable column a create must set is set by something: the
/// record form, toasty (`#[auto]`, `#[default(..)]`), the tenant stamp, or the
/// resource's own `CREATE_COLUMNS`.
fn check_create_columns<R: Resource>(
    fields: &[crate::form::FormField<<R::Form as RecordForm>::Field>],
) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let prefilled = crate::form::prefilled_fields::<R::Model>();
    let tenant = tenant_column::<R>();
    if let Some(column) = &tenant
        && R::CREATE_COLUMNS.contains(&column.as_str())
    {
        return Err(format!(
            "resource `{resource}` lists its tenant column `{column}` in `CREATE_COLUMNS` — the \
             framework stamps it on create; drop it there and delegate to `write_create`"
        ));
    }
    let model = R::Model::schema();
    let root = model.as_root_unwrap();
    for name in R::CREATE_COLUMNS {
        if !root
            .fields
            .iter()
            .any(|field| field.name.app.as_deref() == Some(*name))
        {
            return Err(format!(
                "resource `{resource}` names `{name}` in `CREATE_COLUMNS`, but `{}` has no such \
                 field",
                std::any::type_name::<R::Model>()
            ));
        }
    }
    for (index, field) in root.fields.iter().enumerate() {
        let Some(name) = field.name.app.as_deref() else {
            continue;
        };
        let filled = field.nullable()
            || field.is_relation()
            || prefilled.get(index).copied().unwrap_or(false)
            || tenant.as_deref() == Some(name)
            || fields.iter().any(|claim| claim.name == name)
            || R::CREATE_COLUMNS.contains(&name);
        if !filled {
            return Err(format!(
                "resource `{resource}` allows create, but nothing writes the non-nullable column \
                 `{name}`: its record form has no such field, toasty fills no `#[default(..)]` for \
                 it, and `CREATE_COLUMNS` does not name it — every create would fail at the driver"
            ));
        }
    }
    Ok(())
}

/// A context for the build-time declaration checks: the app's own values, no
/// request. Resources must be able to describe their table and form from this
/// — that they cannot read a request here is the contract, not a limitation.
fn validation_cx(db: &Db) -> Cx {
    let mut app_context = topcoat::context::AppContext::new();
    app_context.insert(db.clone());
    Cx::new(std::sync::Arc::new(app_context))
}

/// Parse a panel route path, panicking on malformed input — the paths are
/// built from the panel prefix and a resource or page slug, both validated at
/// registration ([`validate_route_segment`]), so a malformed path here is a
/// framework bug rather than user input.
pub(crate) fn route_path(path: &str) -> topcoat::router::PathBuf {
    Path::from_str(path)
        .expect("panel route paths are well-formed")
        .to_owned()
}

#[cfg(test)]
mod tests;
