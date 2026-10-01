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
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "user-profiles_2".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }),
            )
            .paginate(25)
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
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "user*profiles".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }),
            )
            .paginate(25)
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

/// CSRF does not depend on authentication: with `Auth::disabled()` there is
/// no gate, and a create POST without a matching `csrf_token` is still 403,
/// so opting out of sessions does not drop the double-submit check.
#[tokio::test]
async fn csrf_is_enforced_with_auth_disabled() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = DummyForm;

        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct DummyForm {
        name: String,
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
        schema::{Field, Schema},
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
        type Form = AuthorForm;
        // Not gated, so the tenant is not stamped: a create override would
        // set it.
        const CREATE_COLUMNS: &'static [&'static str] = &["tenant_id"];
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Author::fields().email()).unique())
        }

        fn slug() -> String {
            "authors".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table() -> Table<Author> {
            Table::new(
                |a: &Author| a.id.to_string(),
                TextColumn::r#for(Author::fields().email(), |a: &Author| a.email.clone()),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Author)]
    struct AuthorForm {
        email: String,
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
    fn dummy_table() -> Table<Dummy> {
        Table::new(
            |d: &Dummy| d.id.to_string(),
            TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| d.name.clone()),
        )
    }

    struct UndiscoverableResource;
    impl Resource for UndiscoverableResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn requires_tenant() -> bool {
            true
        }
        fn table() -> Table<Dummy> {
            dummy_table()
        }
    }

    /// The same undiscoverable model, scoped by the resource itself — the
    /// shape a row that inherits its tenant uses. `name` stands in for the
    /// relation path; the point is that the probe accepts a declared
    /// predicate.
    struct DeclaredScopeResource;
    impl Resource for DeclaredScopeResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "declared".to_string()
        }
        fn requires_tenant() -> bool {
            true
        }
        fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
            Some(Dummy::fields().name().eq(tenant.to_string()))
        }
        fn table() -> Table<Dummy> {
            dummy_table()
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
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

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
        schema::{Field, Schema},
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
        type Form = UnbackedForm;
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Subscriber::fields().nickname()).unique())
        }

        fn slug() -> String {
            "subscribers".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table() -> Table<Subscriber> {
            Table::new(
                |s: &Subscriber| s.id.to_string(),
                TextColumn::r#for(Subscriber::fields().nickname(), |s: &Subscriber| {
                    s.nickname.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Subscriber)]
    struct UnbackedForm {
        nickname: String,
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

/// The table carries its key and columns by construction, so a keyed table
/// builds with or without action chrome.
#[tokio::test]
async fn panel_build_accepts_keyed_tables_with_and_without_chrome() {
    use crate::{
        resource::{Resource, Table, TextColumn},
        schema::{Field, Schema},
    };

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        nickname: String,
    }

    fn keyed_table() -> Table<Subscriber> {
        Table::new(
            |s: &Subscriber| s.id.to_string(),
            TextColumn::r#for(Subscriber::fields().nickname(), |s: &Subscriber| {
                s.nickname.clone()
            }),
        )
    }

    struct ChromeResource;
    impl Resource for ChromeResource {
        type Model = Subscriber;
        type Form = ChromeForm;
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Subscriber::fields().nickname()))
        }

        fn slug() -> String {
            "subscribers".to_string()
        }
        fn can_delete_any(_cx: &Cx) -> bool {
            true
        }
        fn table() -> Table<Subscriber> {
            keyed_table()
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Subscriber)]
    struct ChromeForm {
        nickname: String,
    }
    struct ChromeOffResource;
    impl Resource for ChromeOffResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "subscribers".to_string()
        }
        fn table() -> Table<Subscriber> {
            keyed_table()
        }
    }

    struct ViewedResource;
    impl Resource for ViewedResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "subscribers".to_string()
        }
        fn table() -> Table<Subscriber> {
            keyed_table()
        }
        fn view(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Subscriber::fields().nickname()))
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

    panel()
        .resource::<ChromeResource>()
        .build()
        .expect("a keyed table with chrome builds");
    panel()
        .resource::<ChromeOffResource>()
        .build()
        .expect("a keyed table without chrome builds");
    panel()
        .resource::<ViewedResource>()
        .build()
        .expect("a keyed table with a detail view builds");
}

/// The marker is a property of the declaration, not of the policy serving
/// it: a read-only resource — `can_create` denied, the default —
/// still fails the build on an unbacked `unique()`, so fixing the policy
/// later cannot silently re-arm a check the database does not keep.
#[tokio::test]
async fn panel_build_rejects_an_unbacked_unique_marker_even_when_create_is_denied() {
    use crate::{
        resource::{Resource, Table, TextColumn},
        schema::{Field, Schema},
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
        type Form = ReadOnlyForm;
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Subscriber::fields().nickname()).unique())
        }

        fn slug() -> String {
            "subscribers".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        // `can_create` keeps its default (deny); only the form is declared.
        fn table() -> Table<Subscriber> {
            Table::new(
                |s: &Subscriber| s.id.to_string(),
                TextColumn::r#for(Subscriber::fields().nickname(), |s: &Subscriber| {
                    s.nickname.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Subscriber)]
    struct ReadOnlyForm {
        nickname: String,
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
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
        fn slug() -> String {
            "dummies".to_string()
        }
    }
    struct SecondResource;
    impl Resource for SecondResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
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
        format!("{error}").contains("duplicate slug"),
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
                type Form = crate::NoForm<Self::Model>;
                fn slug() -> String {
                    $slug.to_string()
                }
                fn table() -> crate::resource::Table<Dummy> {
                    crate::resource::Table::new(
                        |d: &Dummy| d.id.to_string(),
                        crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                            d.name.clone()
                        }),
                    )
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

/// A table's builders record a misdeclaration rather than panic, and `build`
/// reports it as a registration error the caller can log or exit on.
#[tokio::test]
async fn panel_build_reports_recorded_table_misdeclarations() {
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

    /// Two columns over one field: a duplicate name.
    struct DuplicateColumnResource;
    impl Resource for DuplicateColumnResource {
        type Model = Doc;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "docs".to_string()
        }
        fn table() -> Table<Doc> {
            Table::new(
                |d: &Doc| d.id.to_string(),
                (
                    TextColumn::r#for(Doc::fields().title(), |d: &Doc| d.title.clone()),
                    TextColumn::r#for(Doc::fields().title(), |d: &Doc| d.title.clone()),
                ),
            )
        }
    }

    /// An embedded step is not a single-field lens: the column records the
    /// refused traversal.
    struct TraversalLensResource;
    impl Resource for TraversalLensResource {
        type Model = Doc;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "docs".to_string()
        }
        fn table() -> Table<Doc> {
            Table::new(
                |d: &Doc| d.id.to_string(),
                TextColumn::r#for(Doc::fields().meta().note(), |d: &Doc| d.meta.note.clone()),
            )
        }
    }

    /// A page of no rows: `Table::paginate` records zero.
    struct ZeroPageResource;
    impl Resource for ZeroPageResource {
        type Model = Doc;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "docs".to_string()
        }
        fn table() -> Table<Doc> {
            Table::new(
                |d: &Doc| d.id.to_string(),
                TextColumn::r#for(Doc::fields().title(), |d: &Doc| d.title.clone()),
            )
            .paginate(0)
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
        error.contains("is misdeclared") && error.contains("table: duplicate column name"),
        "the recorded misdeclaration must reach the registration error, got {error}"
    );

    let Err(error) = panel().resource::<TraversalLensResource>().build() else {
        panic!("a traversal lens must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("is misdeclared") && error.contains("single-field lens"),
        "the recorded misdeclaration must reach the registration error, got {error}"
    );

    let Err(error) = panel().resource::<ZeroPageResource>().build() else {
        panic!("a zero page size must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("is misdeclared") && error.contains("page size"),
        "the recorded misdeclaration must reach the registration error, got {error}"
    );
}

#[tokio::test]
async fn panel_mounts_runtime_page_rerun_routes() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
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
        type Form = crate::NoForm<Self::Model>;

        // A rendered page, not the default-deny 403: an error response is
        // produced above the layer chain, so only a served document proves
        // the header is installed.
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
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

/// A view's misdeclaration — here two fields over one column — fails the
/// build, as a table's does, rather than the first detail request.
#[tokio::test]
async fn panel_build_rejects_a_misdeclared_view() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

    struct BadView;
    impl Resource for BadView {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }

        fn view(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new((
                Field::text(Dummy::fields().name()),
                Field::text(Dummy::fields().name()),
            ))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let Err(error) = panel_for::<BadView>(db).build() else {
        panic!("a view declaring one field twice must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("view: duplicate field name 'name'"),
        "the error names the part and the field, got {error}"
    );
}
