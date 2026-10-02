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

/// Mounts a [`Panel`] on a router the app owns.
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
    /// Mounts `panel` at its prefix with its routes, shell layout, and gating layers.
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

/// Installs what every panel on a router shares.
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
    // The runtime layer has no path, so a page re-run reaches the panel's layers already rewritten to a `GET`.
    if builder.get_app_context::<RuntimeSetup>().is_none() {
        builder = builder.runtime();
    }
    if builder.get_app_context::<PrefetchMode>().is_none() {
        builder = builder.prefetch(PrefetchMode::Never);
    }
    builder
}

/// Redirects the panel root of a panel with no [`home`](Panel::home) page to the first declared resource's list.
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

/// Validates one path segment a panel derives routes from, refusing anything that cannot serve as a literal URL segment.
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

/// Checks what a declared resource promises before the panel serves it, building its declarations once for handlers to serve.
pub(super) fn check_resource<R: Resource>(
    cx: &Cx,
    dx: &DeclCx,
    declarations: &mut Declarations,
) -> Result<(), String> {
    if let Some(Err(error)) = R::tenancy().column_field() {
        return Err(format!(
            "resource `{}`'s `Tenancy::column` lens binds no column of `{}`: {error} — name a \
             UUID field of the model, or use `Tenancy::via` for a tenant reached through a relation",
            std::any::type_name::<R>(),
            std::any::type_name::<R::Model>(),
        ));
    }
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

/// Checks a resource's form declaration against its record form.
fn check_form_declaration<R: Resource>(
    cx: &Cx,
    dx: &DeclCx,
    declared: &Declared<R>,
) -> Result<(), String> {
    let resource = std::any::type_name::<R>();
    let form = std::any::type_name::<R::Form>();
    if <R::Form as RecordForm>::HAS_FORM {
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
        // An empty submission resolves wherever the schema lets one through.
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
    // The framework stamps the tenant column on create.
    if let Some(column) = tenant_column::<R>()
        && let Some(field) = fields.iter().find(|field| field.keys.contains(&column))
    {
        return Err(format!(
            "resource `{resource}` is tenant-scoped, but record form field `{}` claims its tenant \
             column `{column}` — the framework stamps it on create; drop it from the form",
            field.name
        ));
    }
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
    if can::<R>(cx, Ability::Create) {
        check_create_columns::<R>(&fields)?;
    }
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

/// Names `R`'s own tenant column.
fn tenant_column<R: Resource>() -> Option<String> {
    R::tenancy()
        .column_field()
        .and_then(Result::ok)
        .map(|field| field.name.clone())
}

/// Checks that every non-nullable column a create needs has a writer.
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

/// Builds the context for the build-time declaration checks from the app's values with no request.
fn validation_cx(db: &Db) -> Cx {
    let mut app_context = topcoat::context::AppContext::new();
    app_context.insert(db.clone());
    Cx::new(std::sync::Arc::new(app_context))
}

/// Parses a panel route path, panicking on malformed input.
pub(crate) fn route_path(path: &str) -> topcoat::router::PathBuf {
    Path::from_str(path)
        .expect("panel route paths are well-formed")
        .to_owned()
}

#[cfg(test)]
mod tests;
