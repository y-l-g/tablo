//! Panel assembly: [`Panel::build`](super::Panel::build), the build-time
//! declaration checks, and the route-path helpers.

use toasty::{Db, schema::Model};
use topcoat::{
    Result,
    asset::RouterBuilderAssetExt,
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    router::{
        Body, Path, RouteFn, RouteFuture, Router, RouterBuilderDirectoryExt,
        RouterBuilderDiscoverExt, error::redirect,
    },
    runtime::RouterBuilderRuntimeExt,
};

use super::{
    Panel, Root,
    forms::MAX_FORM_BYTES,
    gate::{LoginHint, PanelPrefix, enforce_auth},
    headers,
    search::SearchRegistry,
    shell::DarkMode,
};
use crate::{error::TabloError, form::RecordForm, resource::Resource};

impl Panel {
    /// Build the [`Router`], discovering all `#[page]` / `#[layout]` / `#[shard]`
    /// items linked into the binary, mounting the browser-runtime layer
    /// (`RouterBuilderRuntimeExt::runtime`, required by `runtime::script`),
    /// installing the `Db` and the panel navigation on the `app_context`,
    /// registering each declared resource's list page and each page, and
    /// serving the home page at the panel root, or a redirect to the first
    /// resource when there is none.
    ///
    /// # Errors
    ///
    /// Reports what the declarative builders could only record:
    /// a missing [`Db`], a resource or page slug that is malformed, reserved
    /// or already held, a second home page, a malformed panel prefix, or
    /// `shell_assets` declared without `assets`. Configuring
    /// a panel wrong is a boot failure, not a request-time panic, so it comes
    /// back as an error the caller can log or exit on.
    pub fn build(self) -> topcoat::Result<Router> {
        if !self.registration_errors.is_empty() {
            return Err(TabloError::Declaration(format!(
                "Panel::build: {}",
                self.registration_errors.join("; ")
            ))
            .into());
        }
        let relation_errors = self.relation_errors();
        if !relation_errors.is_empty() {
            return Err(TabloError::Declaration(format!(
                "Panel::build: {}",
                relation_errors.join("; ")
            ))
            .into());
        }
        if self.shell_assets.is_some() && self.assets.is_none() {
            return Err(TabloError::Declaration(
                "Panel::build requires assets when shell_assets are configured".to_string(),
            )
            .into());
        }
        let Panel {
            prefix,
            db,
            assets,
            shell_assets,
            brand,
            dark_mode,
            nav_items,
            pages,
            routes,
            root,
            slugs: _,
            resource_slugs: _,
            relations: _,
            search_handlers,
            frame_ancestors,
            registration_errors: _,
            resource_checks,
            uploads,
            served_dirs,
            login_hint,
            auth,
        } = self;
        let db = db.ok_or_else(|| {
            TabloError::Declaration("Panel::build requires a Db via app_context".to_string())
        })?;
        // Declaration checks: a resource whose table or form could
        // never render is a configuration error, and the declaration is
        // knowable here — waiting for the first request only moves the failure
        // somewhere less useful. `table`, `form` and `can_create` are pure
        // declarations, so they must not need request-scoped context.
        if !resource_checks.is_empty() {
            let cx = validation_cx(&db);
            let failures: Vec<String> = resource_checks
                .iter()
                .filter_map(|check| check(&cx).err())
                .collect();
            if !failures.is_empty() {
                return Err(TabloError::Declaration(format!(
                    "Panel::build: {}",
                    failures.join("; ")
                ))
                .into());
            }
        }
        crate::auth::assert_models_registered(&db, &auth);
        let mut builder = Router::builder()
            .discover()
            .cookies()
            // Form bodies (urlencoded buffered, multipart streamed) share one
            // cap: without this layer Topcoat's 2 MiB default would
            // 413 uploads the framework otherwise accepts.
            .layer(topcoat::router::BodyLimit::max(MAX_FORM_BYTES))
            .app_context(db);
        // Clickjacking hardening: a response anyone can frame is a
        // threat on every deployment, so the panel ships the directive itself
        // and apps that need framing opt out (or supply their own policy,
        // which wins — the layer only fills the gap).
        if let Some(directive) = frame_ancestors {
            builder = builder.layer(headers::FrameAncestors::new(directive));
        }
        // Auth (ADR-0013): sessions plus the resolving gate under the panel
        // and runtime prefixes, and the login/logout routes. Disabled skips
        // all three but still installs the `Auth` value for the shell.
        if !auth.is_disabled() {
            builder = crate::auth::install(builder, &prefix);
            let login_path = route_path(&format!("{prefix}/login"));
            let logout_path = route_path(&format!("{prefix}/logout"));
            // A credential POST carries no upload: the login route
            // gets its own cap, scoped by path so it wins over the panel's
            // 10 MiB form cap.
            builder = builder.layer(
                topcoat::router::BodyLimit::max(crate::auth::MAX_LOGIN_BYTES)
                    .at(login_path.clone()),
            );
            builder = builder
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
                ));
        }
        if !search_handlers.is_empty() {
            builder = builder.app_context(SearchRegistry(search_handlers));
        }
        // The mount prefix travels with the Router so generic handlers derive
        // resource URLs from the declaration instead of sniffing the request
        // path (item 6 / B4).
        builder = builder.app_context(PanelPrefix(prefix.clone()));
        if !nav_items.is_empty() {
            builder = builder.app_context(nav_items);
        }
        if let Some(assets) = assets {
            builder = builder.assets(assets);
        }
        if let Some(shell_assets) = shell_assets {
            builder = builder.app_context(shell_assets);
        }
        if let Some(brand) = brand {
            builder = builder.app_context(brand);
        }
        if let Some(enabled) = dark_mode {
            builder = builder.app_context(DarkMode(enabled));
        }
        // Where uploaded bytes go: installed once, found by the form
        // handlers and the multipart parser through the app context.
        if let Some(uploads) = uploads {
            builder = builder.app_context(uploads);
        }
        for (path, dir) in served_dirs {
            // Files the panel serves share its origin, so each directory route
            // is wrapped in the hardening layer that makes them inert
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
        if let Some(Root::Redirect(target)) = root {
            builder = builder
                .app_context(RootRedirect(target))
                .route(RouteFn::new(
                    http::Method::GET,
                    route_path(&prefix),
                    panel_root_redirect,
                ));
        }
        builder = builder.app_context(auth);
        if let Some(hint) = login_hint {
            builder = builder.app_context(LoginHint(hint));
        }
        // The runtime layer registers last, outside every other pathless
        // layer: a page re-run is a marked POST the layer rewrites into a
        // GET for the page's own URL, and the layers it wraps must receive
        // the rewritten GET rather than the discarded POST. Panel links
        // navigate through it without prefetching: a prefetch renders the
        // destination, list queries included, for a page the user may never
        // open.
        Ok(builder
            .runtime()
            .prefetch(topcoat::runtime::PrefetchMode::Never)
            .build())
    }
}

impl Panel {
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

/// Where the panel root redirects (the first declared resource's list) when
/// the panel has no home page.
/// Lives on the `app_context` because page handlers are plain `fn` pointers
/// and cannot capture.
#[derive(Debug, Clone)]
struct RootRedirect(String);

/// The panel root of a panel with no [`home`](Panel::home) page: a temporary
/// redirect to the first declared resource's list, so the mount point is never
/// a dead URL.
pub(crate) fn panel_root_redirect(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Defense in depth: every panel handler re-checks the
        // resolved user, so a missing or mis-mounted gate cannot leak the
        // first resource's slug via the redirect target.
        enforce_auth(cx)?;
        let RootRedirect(target) = app_context::<RootRedirect>(cx);
        Err(redirect(target.clone()).into())
    })
}

/// Whether a path is a route pattern ending in a catch-all, which is the only
/// shape [`DirectoryRoute`](topcoat::router::DirectoryRoute) accepts.
///
/// Checked where the path is declared rather than where it is used: upstream
/// `serve_dir` panics on anything else, and `Panel::build` reports instead of
/// panicking — but the path comes from the app, and it would panic
/// first in [`route_path`] (which refuses to spell a route it cannot parse) and
/// then inside `DirectoryRoute::new` (which needs the catch-all last). Asking
/// both conditions here turns a typo into a build error instead of a panic
/// during the build.
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

/// A resource's build-time declaration check: monomorphized once per
/// declared resource by [`Panel::resource`], run by [`Panel::build`] with the
/// app's values and no request.
pub(super) type ResourceCheck = fn(&Cx) -> Result<(), String>;

/// What a declared resource must be able to promise before the panel serves it.
///
/// The trait defaults every method but `table`, so a resource that overrides
/// nothing else compiles and only fails when a user reaches the page that needs
/// the missing piece. The essentials that are *declarations* — a tenant
/// predicate for a gated resource — are checked here, at build, and reported
/// with the resource's type name, together with the agreement between the
/// resource's `Form` and its `form()` schema ([`check_form_declaration`]); a page
/// size the list cannot serve panics in `Table::paginate`, and the caught panic
/// becomes the same build error.
/// Runtime essentials (the record fns) keep their loud failure.
///
/// A declaration that panics is a boot failure too: `Resource::table` and
/// `Resource::form` run code that panics on a mis-declaration, and this check's
/// contract is a registration error the caller can log or exit on. The whole
/// body is caught, because `R::Model::schema()` and the policy predicates are
/// part of the same declaration, and the panic's own message is carried into
/// the error. `AssertUnwindSafe` is sound because nothing observes the captured
/// state after an unwind: `cx` is the build-time `validation_cx`, and the panic
/// fails the whole `build`.
pub(super) fn check_resource<R: Resource>(cx: &Cx) -> Result<(), String> {
    caught::<R>(|| check_resource_inner::<R>(cx))
}

/// The message out of a caught panic payload.
///
/// The declaration panics this catches are `assert!`/`panic!("…")` with a
/// formatted string, so `&str` and `String` cover every one of them; anything
/// else is reported by shape rather than silently dropped.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "a non-string panic payload".to_string()
    }
}

/// The body of [`check_resource`], unwound through `catch_unwind` so a
/// mis-declared resource is a registration error rather than a boot panic.
fn check_resource_inner<R: Resource>(cx: &Cx) -> Result<(), String> {
    // A gated resource that supplies no tenant predicate is misdeclared, and
    // the declaration is checkable without a request:
    // `R::tenant_scope` is pure, and the default derivation answers by the
    // model's *shape* — a `tenant_id` UUID field, found by name and type — not
    // by the tenant value, so the nil UUID is enough to ask whether a predicate
    // exists at all. Refusing here is what `build`'s contract promises a
    // declaration error gets; the request-time error in `apply_tenant_scope`
    // stays as the backstop for a resource whose predicate is only `None` for
    // some tenants, and for app code that calls `scoped_query` outside a panel.
    //
    // Checked before the declarations below because the gate and the scope
    // govern every handler this resource registers, not just the list and
    // create pages those checks are about.
    if R::requires_tenant() && R::tenant_scope(uuid::Uuid::nil()).is_none() {
        return Err(format!(
            "resource `{}` requires a tenant, but the framework cannot scope it: `{}` declares no \
             `tenant_id` UUID column to derive the filter from, and the resource does not override \
             `tenant_scope` — declare the column, override `tenant_scope`, or drop \
             `requires_tenant` and scope in `query` (GH #231)",
            std::any::type_name::<R>(),
            std::any::type_name::<R::Model>(),
        ));
    }
    // Declaring the table and the view runs their own misdeclaration checks
    // (a duplicate column or field name, a zero page size, a lens that is not
    // a single field, a modifier on the wrong control), which panic; the
    // `catch_unwind` around this body turns them into this resource's
    // registration error instead of a failure on the first list or detail
    // request.
    let _ = R::table(cx);
    let _ = R::view(cx);
    check_form_declaration::<R>(cx)
}

/// A resource with a record form declares its schema and runs
/// [`check_form_inner`]. A resource whose form serves no pages
/// ([`RecordForm::HAS_FORM`] false) declares no schema, and a policy that
/// allows create would link to a page that does not exist.
fn check_form_declaration<R: Resource>(cx: &Cx) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let form = std::any::type_name::<R::Form>();
    if <R::Form as RecordForm>::HAS_FORM {
        // A form with fields and the empty `form()` default is a missing
        // override; the key check below would only name the first field.
        if R::form(cx).is_empty() && !<R::Form as RecordForm>::fields(cx).is_empty() {
            return Err(format!(
                "resource `{resource}` names record form `{form}` but does not override `form()`, \
                 whose default declares no controls"
            ));
        }
        return check_form_inner::<R>(cx);
    }
    if !R::form(cx).is_empty() {
        return Err(format!(
            "resource `{resource}` declares a form schema but its `Form`, `{form}`, serves no \
             form — name the record form in `type Form`"
        ));
    }
    if R::can_create(cx) {
        return Err(format!(
            "resource `{resource}` allows create but has no form — name its record form in `type \
             Form` and declare `form()`"
        ));
    }
    Ok(())
}

fn check_form_inner<R: Resource>(cx: &Cx) -> Result<(), String> {
    let form = R::form(cx);
    let resource = std::any::type_name::<R>();
    let fields = <R::Form as RecordForm>::fields(cx);
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
    // Tenant ownership: the framework stamps a gated resource's tenant column
    // on create; a form that claimed it would let the client choose.
    if R::requires_tenant()
        && let Some(column) = crate::tenancy::tenant_field_name::<R::Model>()
        && let Some(field) = fields.iter().find(|field| field.keys.contains(&column))
    {
        return Err(format!(
            "resource `{resource}` requires a tenant, but record form field `{}` claims its tenant \
             column `{column}` — the framework stamps it on create; drop it from the form",
            field.name
        ));
    }
    // Column coverage: a create that leaves a non-nullable column unset fails
    // at the driver on every submit, with no field to point the user at.
    if R::can_create(cx) {
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

/// Every non-nullable column a create must set is set by something: the
/// record form, toasty (`#[auto]`, `#[default(..)]`), the tenant stamp, or the
/// resource's own `CREATE_COLUMNS`.
fn check_create_columns<R: Resource>(
    fields: &[crate::form::FormField<<R::Form as RecordForm>::Field>],
) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let prefilled = crate::form::prefilled_fields::<R::Model>();
    let tenant = R::requires_tenant()
        .then(crate::tenancy::tenant_field_name::<R::Model>)
        .flatten();
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

/// Run a declaration check, turning a panic in the app's declarations into a
/// registration error naming the resource.
fn caught<R: Resource>(check: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(check)) {
        Ok(result) => result,
        Err(payload) => Err(format!(
            "resource `{}` panicked while declaring itself: {}",
            std::any::type_name::<R>(),
            panic_message(payload.as_ref())
        )),
    }
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
