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

#[cfg(feature = "auth")]
use super::gate::LoginHint;
use super::{
    Panel,
    forms::MAX_FORM_BYTES,
    gate::{PanelPrefix, enforce_auth},
    headers,
    list::declared_chrome,
    search::SearchRegistry,
    shell::DarkMode,
};
use crate::resource::Resource;

impl Panel {
    /// Build the [`Router`], discovering all `#[page]` / `#[layout]` / `#[shard]`
    /// items linked into the binary, mounting the browser-runtime layer
    /// (`RouterBuilderRuntimeExt::runtime`, required by `runtime::script`),
    /// installing the `Db` and the panel navigation on the `app_context`,
    /// registering each declared resource's list page, and pointing the
    /// panel root at the first resource.
    ///
    /// # Errors
    ///
    /// Reports what the declarative builders could only record:
    /// a missing [`Db`], a duplicate or malformed resource slug, a malformed
    /// panel prefix, or `shell_assets` declared without `assets`. Configuring
    /// a panel wrong is a boot failure, not a request-time panic, so it comes
    /// back as an error the caller can log or exit on.
    ///
    /// With the `auth` feature off nothing authenticates requests, so a panel
    /// that has not acknowledged that with
    /// [`Panel::auth(Auth::disabled())`](Self::auth) is also an error
    /// (ADR-0013).
    pub fn build(self) -> topcoat::Result<Router> {
        if !self.registration_errors.is_empty() {
            return Err(std::io::Error::other(format!(
                "Panel::build: {}",
                self.registration_errors.join("; ")
            ))
            .into());
        }
        if self.shell_assets.is_some() && self.assets.is_none() {
            return Err(std::io::Error::other(
                "Panel::build requires assets when shell_assets are configured",
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
            root_target,
            slugs: _,
            search_handlers,
            frame_ancestors,
            registration_errors: _,
            resource_checks,
            uploads,
            served_dirs,
            #[cfg(feature = "auth")]
            login_hint,
            #[cfg(feature = "auth")]
            auth,
            #[cfg(not(feature = "auth"))]
            auth_disabled,
        } = self;
        let db = db.ok_or_else(|| {
            topcoat::Error::from(std::io::Error::other(
                "Panel::build requires a Db via app_context",
            ))
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
                return Err(std::io::Error::other(format!(
                    "Panel::build: {}",
                    failures.join("; ")
                ))
                .into());
            }
        }
        // Auth compiled out (ADR-0013): `enforce_auth` is a no-op and no gate
        // is installed, so a panel that reaches here would serve every page and
        // mutation to anyone. The opt-out stays a line of app code.
        #[cfg(not(feature = "auth"))]
        if !auth_disabled {
            return Err(std::io::Error::other(
                "Panel::build: tablo-core is built without the `auth` feature, so nothing \
                 authenticates requests; call `.auth(Auth::disabled())` to serve the panel \
                 ungated, or enable the feature",
            )
            .into());
        }
        #[cfg(feature = "auth")]
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
        #[cfg(feature = "auth")]
        {
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
        // The panel root has no home page of its own; until custom pages exist,
        // the prefix serves a redirect to the first resource's
        // list so the mount point is never a dead URL.
        if let Some(target) = root_target {
            builder = builder
                .app_context(RootRedirect(target))
                .route(RouteFn::new(
                    http::Method::GET,
                    route_path(&prefix),
                    panel_root_redirect,
                ));
        }
        #[cfg(feature = "auth")]
        {
            builder = builder.app_context(auth);
            if let Some(hint) = login_hint {
                builder = builder.app_context(LoginHint(hint));
            }
        }
        // The runtime layer registers last, outside every other pathless
        // layer: a page re-run is a marked POST the layer rewrites into a
        // GET for the page's own URL, and the layers it wraps must receive
        // the rewritten GET rather than the discarded POST.
        Ok(builder.runtime().build())
    }
}

/// Where the panel root redirects (the first declared resource's list).
/// Lives on the `app_context` because page handlers are plain `fn` pointers
/// and cannot capture.
#[derive(Debug, Clone)]
struct RootRedirect(String);

/// The panel root: a temporary redirect to the first declared resource's
/// list, so the mount point is never a dead URL (custom pages remain future
/// work; see `docs/guide/src/panel-and-routing.md`). Filament registers its
/// home page here.
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

/// Validate one path segment a panel derives routes from: a
/// `Resource::slug()` override, or a segment of the panel prefix.
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
/// The trait ships every method with a default, so a resource that overrides
/// nothing compiles and only fails when a user reaches a page. The essentials
/// that are *declarations* — a tenant predicate for a gated resource, a
/// renderable table, a form for the create page, a backed `unique()` marker —
/// are checked here, at build, and reported with the resource's type name.
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
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_resource_inner::<R>(cx)
    })) {
        Ok(result) => result,
        Err(payload) => Err(format!(
            "resource `{}` panicked while declaring itself: {}",
            std::any::type_name::<R>(),
            panic_message(payload.as_ref())
        )),
    }
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
    // Checked before the page essentials below because the gate and the scope
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
    // Chrome is attached by `wire_table_actions`, not by `R::table(cx)`
    // the record-key requirement is only knowable from the same
    // derivation the wiring reads.
    let chrome = declared_chrome::<R>(cx);
    if let Some(missing) = R::table(cx).missing_essentials(chrome) {
        return Err(format!(
            "resource `{}` cannot serve its list: {missing}",
            std::any::type_name::<R>()
        ));
    }
    // The form is only required where the panel would serve one, and `create`
    // is the statically checkable half of that (`can_update` needs a record).
    // The default policy denies create, so a read-only resource is unaffected.
    let form = R::form(cx);
    if R::can_create(cx) && form.is_empty() {
        return Err(format!(
            "resource `{}` allows create but its form declares no fields — build it with Schema::new(..)",
            std::any::type_name::<R>()
        ));
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
    for (name, input) in form.text_inputs() {
        if !input.is_unique() {
            continue;
        }
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

/// A context for the build-time declaration checks: the app's own values, no
/// request. Resources must be able to describe their table and form from this
/// — that they cannot read a request here is the contract, not a limitation.
fn validation_cx(db: &Db) -> Cx {
    let mut app_context = topcoat::context::AppContext::new();
    app_context.insert(db.clone());
    Cx::new(std::sync::Arc::new(app_context))
}

/// Parse a panel route path, panicking on malformed input — the paths are
/// built from the panel prefix and the resource slug, both validated at
/// registration ([`validate_route_segment`]), so a malformed path here is a
/// framework bug rather than user input.
pub(crate) fn route_path(path: &str) -> topcoat::router::PathBuf {
    Path::from_str(path)
        .expect("panel route paths are well-formed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use toasty::Db;

    use super::*;
    use crate::panel::test_support::{Dummy, dummy_table, panel_for};

    /// A slug made of ordinary URL-segment characters still builds, and its
    /// list route resolves: rejecting the pattern characters must not
    /// reject the accepted ones.
    #[tokio::test]
    async fn a_plain_slug_builds_and_resolves() {
        use crate::resource::Resource;

        struct PlainResource;
        impl Resource for PlainResource {
            type Model = Dummy;
            fn slug() -> String {
                "user-profiles_2".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                crate::resource::Table::r#for(cx)
                    .key(|d: &Dummy| d.id.to_string())
                    .paginate(25)
                    .columns(crate::resource::TextColumn::r#for(
                        Dummy::fields().name(),
                        |d: &Dummy| d.name.clone(),
                    ))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<PlainResource>(db)
            .build()
            .expect("a plain slug builds");
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri("/admin/user-profiles_2")
            .body(Body::empty())
            .unwrap();
        let response = router.handle(request).await;
        assert_eq!(
            response.status(),
            http::StatusCode::OK,
            "the list route a plain slug builds must resolve"
        );
        let html = String::from_utf8_lossy(
            &http_body_util::BodyExt::collect(response.into_body())
                .await
                .unwrap()
                .to_bytes(),
        )
        .to_string();
        assert!(
            html.contains("Dummies</h1>"),
            "the resolved list page must render its title: {html}"
        );
    }

    /// A slug containing `*` builds and resolves: `*` is a literal
    /// static segment in the router, so rejecting it would break a slug that
    /// worked; only the `{*name}` catch-all spelling carries meaning, and the
    /// `{` it needs is already refused.
    #[tokio::test]
    async fn a_star_slug_builds_and_resolves() {
        use crate::resource::Resource;

        struct StarResource;
        impl Resource for StarResource {
            type Model = Dummy;
            fn slug() -> String {
                "user*profiles".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                crate::resource::Table::r#for(cx)
                    .key(|d: &Dummy| d.id.to_string())
                    .paginate(25)
                    .columns(crate::resource::TextColumn::r#for(
                        Dummy::fields().name(),
                        |d: &Dummy| d.name.clone(),
                    ))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<StarResource>(db)
            .build()
            .expect("a slug containing `*` builds");
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri("/admin/user*profiles")
            .body(Body::empty())
            .unwrap();
        let response = router.handle(request).await;
        assert_eq!(
            response.status(),
            http::StatusCode::OK,
            "the list route a `*` slug builds must resolve"
        );
        let html = String::from_utf8_lossy(
            &http_body_util::BodyExt::collect(response.into_body())
                .await
                .unwrap()
                .to_bytes(),
        )
        .to_string();
        assert!(
            html.contains("Dummies</h1>"),
            "the resolved list page must render its title: {html}"
        );
    }

    /// The explicit opt-out is the acknowledgement `build` requires, so an app
    /// that asks for an ungated panel gets one.
    #[cfg(not(feature = "auth"))]
    #[tokio::test]
    async fn build_accepts_the_explicit_opt_out() {
        use crate::resource::Resource;

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;

            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        panel_for::<DummyResource>(db)
            .build()
            .expect("the explicit opt-out builds the panel");
    }

    /// The feature-off build has no gate, so it refuses a panel that has not
    /// acknowledged that (ADR-0013): serving ungated stays a line of app code,
    /// never a side effect of trimming dependencies.
    #[cfg(not(feature = "auth"))]
    #[tokio::test]
    async fn build_refuses_an_unacknowledged_ungated_panel() {
        use crate::resource::Resource;

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;

            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let Err(error) = Panel::new("admin")
            .app_context(db)
            .resource::<DummyResource>()
            .build()
        else {
            panic!("an ungated panel must not build without the auth feature");
        };
        assert!(
            format!("{error}").contains("auth"),
            "the error must name the missing auth feature, got {error}"
        );
    }

    /// CSRF does not depend on the `auth` feature: with the gate
    /// compiled out, a create POST without a matching `csrf_token` is still
    /// 403, so dropping sessions does not drop the double-submit check.
    #[cfg(not(feature = "auth"))]
    #[tokio::test]
    async fn csrf_is_enforced_without_the_auth_feature() {
        use crate::{
            resource::Resource,
            schema::{Schema, TextInput},
        };

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;

            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Dummy::fields().name()))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<DummyResource>(db)
            .build()
            .expect("the explicit opt-out builds the panel");

        let response = router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/admin/dummies/create")
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("name=Ada"))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            response.status(),
            http::StatusCode::FORBIDDEN,
            "a create POST with no csrf_token must fail closed"
        );
    }

    /// GH #102: `Panel::dark_mode` is the theme a first-time visitor gets. It
    /// must reach the rendered document's `<html class>`.
    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn dark_mode_sets_the_document_class() {
        let db = Db::builder()
            .models(toasty::models!(
                crate::auth::AdminUser,
                crate::auth::AuthSession
            ))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = Panel::new("admin")
            .app_context(db)
            .auth(crate::Auth::password())
            .dark_mode(true)
            .build()
            .expect("panel builds");

        // The standalone login page renders the same document the admin shell
        // does (ADR-0013), so it carries the theme class without a session.
        let response = router
            .handle(
                http::Request::builder()
                    .uri("/admin/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), http::StatusCode::OK);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("<html class=\"dark\">"),
            "dark_mode(true) must set the document's dark class, got {html}"
        );
    }

    /// The guard's other half: a unique index — single-field or composite —
    /// keeps building. `lens_field_unique` reads the model's index list, so
    /// `#[unique(tenant_id, email)]` (the tenant-scoped arrangement the panel
    /// documents) is not a false positive.
    #[tokio::test]
    async fn panel_build_accepts_unique_markers_with_a_backing_index() {
        use crate::{
            resource::{Resource, Table, TextColumn},
            schema::{Schema, TextInput},
        };

        #[derive(Debug, toasty::Model, Clone)]
        #[unique(tenant_id, email)]
        struct Author {
            #[key]
            #[auto]
            id: uuid::Uuid,
            tenant_id: uuid::Uuid,
            email: String,
        }
        struct AuthorResource;
        impl Resource for AuthorResource {
            type Model = Author;
            fn slug() -> String {
                "authors".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> Table<Author> {
                Table::r#for(cx)
                    .id(|a: &Author| a.id.to_string())
                    .columns(TextColumn::r#for(Author::fields().email(), |a: &Author| {
                        a.email.clone()
                    }))
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Author::fields().email()).unique())
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Author))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        panel_for::<AuthorResource>(db)
            .build()
            .expect("a composite unique index backs the marker");
    }

    /// GH #174: a panel with no `Db` is a configuration error, not a panic.
    #[test]
    fn panel_build_errors_without_db() {
        // `Router` has no `Debug`, so `expect_err` cannot report the Ok case.
        let Err(error) = Panel::new("admin").build() else {
            panic!("a panel without a Db must not build");
        };
        assert!(
            format!("{error}").contains("requires a Db"),
            "the error must name the missing Db, got {error}"
        );
    }

    /// GH #231: a gated resource that supplies no tenant predicate is a
    /// declaration error, and #223's `tenant_scope` probe is pure — so `build`
    /// refuses it with an error naming the resource instead of waiting for the
    /// first request to answer its logged 500. The override half builds, so the
    /// check rejects a *missing* predicate rather than the hook itself.
    #[tokio::test]
    async fn panel_build_rejects_a_gated_resource_with_no_tenant_predicate() {
        use crate::resource::{Resource, Table, TextColumn};

        /// A renderable table, so tenancy is the *only* thing either resource
        /// below could be refused for: the rejection is the tenant probe's, not
        /// a page essential's. The model has no `tenant_id` column, so only an
        /// override can scope it.
        fn dummy_table(cx: &Cx) -> Table<Dummy> {
            Table::r#for(cx)
                .id(|d: &Dummy| d.id.to_string())
                .columns(TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }))
        }

        struct UndiscoverableResource;
        impl Resource for UndiscoverableResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn requires_tenant() -> bool {
                true
            }
            fn table(cx: &Cx) -> Table<Dummy> {
                dummy_table(cx)
            }
        }

        /// The same undiscoverable model, scoped by the resource itself — the
        /// shape a row that inherits its tenant uses. `name` stands in for the
        /// relation path; the point is that the probe accepts a declared
        /// predicate.
        struct DeclaredScopeResource;
        impl Resource for DeclaredScopeResource {
            type Model = Dummy;
            fn slug() -> String {
                "declared".to_string()
            }
            fn requires_tenant() -> bool {
                true
            }
            fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
                Some(Dummy::fields().name().eq(tenant.to_string()))
            }
            fn table(cx: &Cx) -> Table<Dummy> {
                dummy_table(cx)
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let panel = || {
            Panel::new("admin")
                .app_context(db.clone())
                .auth(crate::Auth::disabled())
        };

        let Err(error) = panel().resource::<UndiscoverableResource>().build() else {
            panic!("a gated resource with no tenant predicate must not build");
        };
        let error = format!("{error}");
        assert!(
            error.contains("UndiscoverableResource")
                && error.contains("tenant_id")
                && error.contains("tenant_scope"),
            "the error must name the resource and both ways to scope it, got {error}"
        );

        panel()
            .resource::<DeclaredScopeResource>()
            .build()
            .expect("a declared tenant_scope scopes a gated resource");
    }

    /// GH #174: `slug()` is free-form and reaches route paths and response
    /// headers, so a hostile value fails registration instead of splitting a
    /// header or panicking in `route_path` at boot.
    #[test]
    fn panel_build_rejects_a_hostile_slug() {
        use crate::resource::Resource;

        struct HostileResource;
        impl Resource for HostileResource {
            type Model = Dummy;

            fn slug() -> String {
                "a\"b\r\n".to_string()
            }
        }

        let Err(error) = Panel::new("admin").resource::<HostileResource>().build() else {
            panic!("a slug with quotes and CRLF must not build");
        };
        assert!(
            format!("{error}").contains("Resource::slug"),
            "the error must name the offending slug, got {error}"
        );
    }

    /// GH #189 item 3: `.unique()` is a promise about the column, so declaring
    /// it on a field with no unique index fails the build instead of turning on
    /// a check the database does not back.
    #[tokio::test]
    async fn panel_build_rejects_a_unique_marker_without_a_unique_index() {
        use crate::{
            resource::{Resource, Table, TextColumn},
            schema::{Schema, TextInput},
        };

        #[derive(Debug, toasty::Model, Clone)]
        struct Subscriber {
            #[key]
            #[auto]
            id: uuid::Uuid,
            nickname: String,
        }
        struct UnbackedResource;
        impl Resource for UnbackedResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_create(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> Table<Subscriber> {
                Table::r#for(cx)
                    .id(|s: &Subscriber| s.id.to_string())
                    .columns(TextColumn::r#for(
                        Subscriber::fields().nickname(),
                        |s: &Subscriber| s.nickname.clone(),
                    ))
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Subscriber::fields().nickname()).unique())
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let Err(error) = Panel::new("admin")
            .app_context(db)
            .resource::<UnbackedResource>()
            .build()
        else {
            panic!("a `unique()` marker with no unique index must not build");
        };
        let error = format!("{error}");
        assert!(
            error.contains("`nickname`") && error.contains("no unique index"),
            "the error must name the field and the missing index, got {error}"
        );
    }

    /// GH #207 part 1: `R::table(cx)` carries no action chrome —
    /// `wire_table_actions` attaches it — so the key requirement is only
    /// knowable from the same declaration the wiring reads. A resource with
    /// action chrome and no key at all fails `build`; a `pk`-only table builds
    /// through the record-key fallback (GH #340).
    #[tokio::test]
    async fn panel_build_rejects_action_chrome_without_a_key() {
        use crate::{
            resource::{Resource, Table, TextColumn},
            schema::{Schema, TextInput},
        };

        #[derive(Debug, toasty::Model, Clone)]
        struct Subscriber {
            #[key]
            #[auto]
            id: uuid::Uuid,
            nickname: String,
        }

        fn keyless_table(cx: &Cx) -> Table<Subscriber> {
            Table::r#for(cx).columns(TextColumn::r#for(
                Subscriber::fields().nickname(),
                |s: &Subscriber| s.nickname.clone(),
            ))
        }

        /// Chrome opted into explicitly: the default opts out of
        /// both links, so a resource that wants them names them — and that is
        /// what makes the record key required.
        struct ChromeResource;
        impl Resource for ChromeResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn deletable() -> bool {
                true
            }
            fn editable() -> bool {
                true
            }
            fn table(cx: &Cx) -> Table<Subscriber> {
                keyless_table(cx)
            }
        }

        /// Chrome left at the opt-in default, so no display key is needed: the
        /// `pk`-only declaration builds through the display fallback.
        struct ChromeOffResource;
        impl Resource for ChromeOffResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            #[allow(deprecated)]
            fn table(cx: &Cx) -> Table<Subscriber> {
                Table::r#for(cx)
                    .pk(|s: &Subscriber| s.id.to_string())
                    .columns(TextColumn::r#for(
                        Subscriber::fields().nickname(),
                        |s: &Subscriber| s.nickname.clone(),
                    ))
            }
        }

        /// Chrome opted in with only a record key: the display falls back to it.
        struct PkOnlyChromeResource;
        impl Resource for PkOnlyChromeResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn deletable() -> bool {
                true
            }
            fn editable() -> bool {
                true
            }
            #[allow(deprecated)]
            fn table(cx: &Cx) -> Table<Subscriber> {
                Table::r#for(cx)
                    .pk(|s: &Subscriber| s.id.to_string())
                    .columns(TextColumn::r#for(
                        Subscriber::fields().nickname(),
                        |s: &Subscriber| s.nickname.clone(),
                    ))
            }
        }

        /// No chrome and no keys at all: still a build error — the row key is
        /// required even with nothing to link to.
        struct KeylessOffResource;
        impl Resource for KeylessOffResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn table(cx: &Cx) -> Table<Subscriber> {
                keyless_table(cx)
            }
        }

        /// Delete and edit left at the opt-in default, but the detail page is
        /// declared, so the View link is action chrome all the same.
        struct ViewedResource;
        impl Resource for ViewedResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn table(cx: &Cx) -> Table<Subscriber> {
                keyless_table(cx)
            }
            fn view(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Subscriber::fields().nickname()))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let panel = || {
            Panel::new("admin")
                .app_context(db.clone())
                .auth(crate::Auth::disabled())
        };

        let Err(error) = panel().resource::<ChromeResource>().build() else {
            panic!("action chrome without a key must not build");
        };
        assert!(
            format!("{error}").contains("no row key"),
            "the error must name the missing key, got {error}"
        );

        let Err(error) = panel().resource::<ViewedResource>().build() else {
            panic!("a View link is action chrome too");
        };
        assert!(
            format!("{error}").contains("no row key"),
            "the error must name the missing key, got {error}"
        );

        panel()
            .resource::<PkOnlyChromeResource>()
            .build()
            .expect("a pk-only table builds through the display fallback");

        let Err(error) = panel().resource::<KeylessOffResource>().build() else {
            panic!("a keyless table must not build even without chrome");
        };
        assert!(
            format!("{error}").contains("no row key"),
            "the error must name the missing key, got {error}"
        );

        panel()
            .resource::<ChromeOffResource>()
            .build()
            .expect("a resource with no action chrome needs no display key");
    }

    /// The marker is a property of the declaration, not of the policy serving
    /// it: a read-only resource — `can_create` denied, the default —
    /// still fails the build on an unbacked `unique()`, so fixing the policy
    /// later cannot silently re-arm a check the database does not keep.
    #[tokio::test]
    async fn panel_build_rejects_an_unbacked_unique_marker_even_when_create_is_denied() {
        use crate::{
            resource::{Resource, Table, TextColumn},
            schema::{Schema, TextInput},
        };

        #[derive(Debug, toasty::Model, Clone)]
        struct Subscriber {
            #[key]
            #[auto]
            id: uuid::Uuid,
            nickname: String,
        }
        struct ReadOnlyResource;
        impl Resource for ReadOnlyResource {
            type Model = Subscriber;
            fn slug() -> String {
                "subscribers".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            // `can_create` keeps its default (deny); only the form is declared.
            fn table(cx: &Cx) -> Table<Subscriber> {
                Table::r#for(cx)
                    .id(|s: &Subscriber| s.id.to_string())
                    .columns(TextColumn::r#for(
                        Subscriber::fields().nickname(),
                        |s: &Subscriber| s.nickname.clone(),
                    ))
            }
            fn form(_cx: &Cx) -> Schema {
                Schema::new(TextInput::r#for(Subscriber::fields().nickname()).unique())
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Subscriber))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let Err(error) = Panel::new("admin")
            .app_context(db)
            .resource::<ReadOnlyResource>()
            .build()
        else {
            panic!("the marker is unbacked whether or not create is allowed");
        };
        assert!(
            format!("{error}").contains("no unique index"),
            "the error must be the marker's, not the policy's, got {error}"
        );
    }

    /// GH #174: duplicate slugs are reported by `build`, not asserted in the
    /// declarative builder — a panel is configured, then validated once.
    #[test]
    fn panel_build_rejects_duplicate_resource_slugs() {
        use crate::resource::Resource;

        struct FirstResource;
        impl Resource for FirstResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
        }
        struct SecondResource;
        impl Resource for SecondResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
        }

        let Err(error) = Panel::new("admin")
            .resource::<FirstResource>()
            .resource::<SecondResource>()
            .build()
        else {
            panic!("two resources over one slug must not build");
        };
        assert!(
            format!("{error}").contains("duplicate resource slug"),
            "the error must name the duplicate, got {error}"
        );
    }

    /// GH #174/#295: a slug carrying a route pattern character a literal segment
    /// cannot hold is a declared registration error, not a panic in
    /// [`route_path`]. `Path::from_str` starts a parameter segment at `{` and a
    /// group at `(`, so an unbalanced pair panics the route builder and a
    /// balanced one silently makes the slug a pattern.
    #[test]
    fn panel_build_rejects_route_pattern_characters_in_a_slug() {
        use crate::resource::Resource;

        macro_rules! pattern_resource {
            ($name:ident, $slug:literal) => {
                struct $name;
                impl Resource for $name {
                    type Model = Dummy;
                    fn slug() -> String {
                        $slug.to_string()
                    }
                }
            };
        }
        pattern_resource!(BraceOpen, "a{b");
        pattern_resource!(BraceClose, "a}b");
        pattern_resource!(ParenOpen, "a(b");
        pattern_resource!(ParenClose, "a)b");

        macro_rules! rejects {
            ($name:ident, $slug:literal) => {{
                let Err(error) = Panel::new("admin").resource::<$name>().build() else {
                    panic!("a slug containing {} must not build", $slug);
                };
                let error = format!("{error}");
                assert!(
                    error.contains("Resource::slug") && error.contains($slug),
                    "the error must name the offending slug {:?}, got {error}",
                    $slug
                );
            }};
        }
        rejects!(BraceOpen, "a{b");
        rejects!(BraceClose, "a}b");
        rejects!(ParenOpen, "a(b");
        rejects!(ParenClose, "a)b");

        // The prefix goes through the same rule, once per segment.
        let Err(error) = Panel::new("adm{in}").build() else {
            panic!("a panel prefix with a route pattern character must not build");
        };
        assert!(
            format!("{error}").contains("panel prefix"),
            "the error must name the panel prefix, got {error}"
        );
    }

    /// GH #207 part 2: `Resource::table` and `Resource::form` run code that
    /// panics on a mis-declaration, but `build`'s contract is a registration
    /// error the caller can log or exit on. Both classes below are caught at
    /// the boundary instead of unwinding out of `build`.
    #[tokio::test]
    async fn panel_build_turns_declaration_panics_into_registration_errors() {
        use crate::resource::{Resource, Table, TextColumn};

        #[derive(Debug, Clone, toasty::Embed)]
        struct Meta {
            note: String,
        }

        #[derive(Debug, toasty::Model, Clone)]
        struct Doc {
            #[key]
            #[auto]
            id: uuid::Uuid,
            title: String,
            meta: Meta,
        }

        /// Two columns over one field: `Table::columns` asserts on the
        /// duplicate name.
        struct DuplicateColumnResource;
        impl Resource for DuplicateColumnResource {
            type Model = Doc;
            fn slug() -> String {
                "docs".to_string()
            }
            fn table(cx: &Cx) -> Table<Doc> {
                Table::r#for(cx).id(|d: &Doc| d.id.to_string()).columns((
                    TextColumn::r#for(Doc::fields().title(), |d: &Doc| d.title.clone()),
                    TextColumn::r#for(Doc::fields().title(), |d: &Doc| d.title.clone()),
                ))
            }
        }

        /// An embedded step is not a single-field lens: `lens_field` refuses
        /// the traversal loudly, which without the boundary catch is
        /// a boot panic.
        struct TraversalLensResource;
        impl Resource for TraversalLensResource {
            type Model = Doc;
            fn slug() -> String {
                "docs".to_string()
            }
            fn table(cx: &Cx) -> Table<Doc> {
                Table::r#for(cx)
                    .id(|d: &Doc| d.id.to_string())
                    .columns(TextColumn::r#for(Doc::fields().meta().note(), |d: &Doc| {
                        d.meta.note.clone()
                    }))
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Doc))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let panel = || {
            Panel::new("admin")
                .app_context(db.clone())
                .auth(crate::Auth::disabled())
        };

        let Err(error) = panel().resource::<DuplicateColumnResource>().build() else {
            panic!("a duplicate column name must not build");
        };
        let error = format!("{error}");
        assert!(
            error.contains("panicked while declaring") && error.contains("duplicate column name"),
            "the panic's own message must survive into the registration error, got {error}"
        );

        let Err(error) = panel().resource::<TraversalLensResource>().build() else {
            panic!("a traversal lens must not build");
        };
        let error = format!("{error}");
        assert!(
            error.contains("panicked while declaring") && error.contains("single-field lens"),
            "the panic's own message must survive into the registration error, got {error}"
        );
    }

    #[tokio::test]
    async fn panel_mounts_runtime_page_rerun_routes() {
        use crate::resource::Resource;

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        let db = Db::builder().connect("sqlite::memory:").await.unwrap();
        let router = panel_for::<DummyResource>(db)
            .build()
            .expect("panel builds");

        // The list page denies by default (default-deny policy → 403). A
        // POST carrying the runtime marker rewrites into a GET for the
        // page's own URL, so it reaches the handler and reports 403;
        // without `.runtime()` on the builder the marked POST never becomes
        // a page GET. (Topcoat's `runtime::script` requires this layer.)
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri("/admin/dummies")
            .header("content-type", "application/json")
            .header(&topcoat::runtime::RUNTIME_HEADER, "true")
            .body(Body::from("{}".to_owned()))
            .unwrap();
        let response = router.handle(request).await;
        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    /// The panel root answers the gate before reading `RootRedirect`
    /// (defense in depth): a mis-mounted gate must not leak the
    /// first resource's slug via the redirect target.
    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn panel_root_redirect_rechecks_auth_before_the_root_target() {
        use topcoat::{context::CxTestBuilder, router::response::IntoResponse};

        // Enforced auth, no resolved user: the handler itself redirects to
        // login — and never reaches the `RootRedirect` read (absent here, so
        // a missing re-check would panic instead of answering).
        let (parts, ()) = http::Request::builder()
            .uri("/admin")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(crate::Auth::password())
            .build();
        let err = match panel_root_redirect(&cx, Body::empty()).await {
            Ok(_) => panic!("unauthenticated root must not read RootRedirect"),
            Err(err) => err,
        };
        let location = err
            .into_response(&cx)
            .expect("gate redirect renders")
            .headers()
            .get(http::header::LOCATION)
            .expect("login redirect carries a location")
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            location.starts_with("/admin/login"),
            "unauthenticated root must redirect to login, got {location}"
        );

        // A resolved user passes the re-check and lands on the first resource.
        let user = crate::auth::CurrentUser {
            id: "u1".to_string(),
            login: "ada@example.com".to_string(),
            display_name: "Ada".to_string(),
            tenant_id: None,
            can_access_panel: true,
        };
        let (parts, ()) = http::Request::builder()
            .uri("/admin")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(crate::Auth::password())
            .app_context(RootRedirect("/admin/users".to_string()))
            .request_context(user)
            .build();
        let err = match panel_root_redirect(&cx, Body::empty()).await {
            Ok(_) => panic!("the redirect is an Err response"),
            Err(err) => err,
        };
        let location = err
            .into_response(&cx)
            .expect("root redirect renders")
            .headers()
            .get(http::header::LOCATION)
            .expect("root redirect carries a location")
            .to_str()
            .unwrap()
            .to_string();
        assert_eq!(location, "/admin/users");
    }

    /// GH #176: every page the panel renders carries the clickjacking
    /// directive by default, and both escape hatches work — a deployment
    /// directive, and an opt-out for a proxy that owns the whole policy.
    #[tokio::test]
    async fn panel_sends_frame_ancestors_unless_opted_out() {
        use crate::resource::Resource;

        struct DummyResource;
        impl Resource for DummyResource {
            type Model = Dummy;

            // A rendered page, not the default-deny 403: an error response is
            // produced above the layer chain, so only a served document proves
            // the header is installed.
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        /// The directive the finished page carries.
        async fn policy(panel: Panel) -> Option<String> {
            let router = panel.build().expect("panel builds");
            let response = router
                .handle(
                    http::Request::builder()
                        .uri("/admin/dummies")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await;
            assert_eq!(response.status(), http::StatusCode::OK, "page must render");
            response
                .headers()
                .get(http::header::CONTENT_SECURITY_POLICY)
                .map(|value| value.to_str().unwrap().to_string())
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let base = || panel_for::<DummyResource>(db.clone());

        assert_eq!(
            policy(base()).await.as_deref(),
            Some("frame-ancestors 'self'"),
            "a panel page must not be frameable by default"
        );
        assert_eq!(
            policy(base().frame_ancestors("'self' https://intranet.example"))
                .await
                .as_deref(),
            Some("frame-ancestors 'self' https://intranet.example"),
            "a deployment that frames the panel says so"
        );
        assert!(
            policy(base().without_frame_ancestors()).await.is_none(),
            "an opted-out panel sends no policy of its own"
        );
    }

    /// GH #188: a served directory's path is a route pattern ending in a
    /// catch-all, and only that; everything else is a build error rather than
    /// the panic upstream `serve_dir` would raise.
    #[test]
    fn serve_dir_accepts_only_a_catch_all_pattern() {
        assert!(is_directory_pattern("/uploads/{*file}"));
        assert!(is_directory_pattern("/{*file}"));
        // Upstream allows a space in a static segment, so a pattern carrying
        // one is still a pattern: the catch-all is what matters, not tidiness.
        assert!(is_directory_pattern("/up loads/{*file}"));
        // Not a catch-all: a plain path, its trailing-slash form, a named
        // parameter, an unnamed catch-all, and a catch-all that is not last.
        assert!(!is_directory_pattern("/uploads"));
        assert!(!is_directory_pattern("/uploads/"));
        assert!(!is_directory_pattern("/uploads/{file}"));
        assert!(!is_directory_pattern("/uploads/{*}"));
        assert!(!is_directory_pattern("/{*file}/more"));
        // Not a route path at all: an unclosed brace, an empty segment, and a
        // catch-all name that is not an identifier.
        assert!(!is_directory_pattern("/uploads/{*file"));
        assert!(!is_directory_pattern("/uploads//{*file}"));
        assert!(!is_directory_pattern("/uploads/{*fi-le}"));
    }
}
