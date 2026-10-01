use toasty::Db;

use super::*;
use crate::{
    Ability, Policy, Tenancy,
    panel::test_support::{
        Dummy, current_panel, dummy_table, mount, mount_without_db, panel_for, panel_state,
    },
};

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
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
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
    let router = mount(db, panel_for::<PlainResource>()).expect("a plain slug builds");
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
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
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
    let router = mount(db, panel_for::<StarResource>()).expect("a slug containing `*` builds");
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

        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::Create)
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
    let router =
        mount(db, panel_for::<DummyResource>()).expect("the explicit opt-out builds the panel");

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
    let router = mount(
        db,
        Panel::new("admin")
            .auth(crate::Auth::password())
            .dark_mode(true),
    )
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
        fn policy() -> impl Policy<Author> {
            |_cx: &Cx, ability: Ability<'_, Author>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            }
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
    mount(db, panel_for::<AuthorResource>()).expect("a composite unique index backs the marker");
}

/// GH #174: a panel with no `Db` is a configuration error, not a panic.
#[test]
fn panel_build_errors_without_db() {
    // `Router` has no `Debug`, so `expect_err` cannot report the Ok case.
    let Err(error) = mount_without_db(Panel::new("admin")) else {
        panic!("a panel without a Db must not build");
    };
    assert!(
        format!("{error}").contains("holds no Db"),
        "the error must name the missing Db, got {error}"
    );
}

/// A `Db` missing the shipped auth models is a mount error naming them, not a
/// panic: every other misconfiguration comes back as an error the caller logs.
#[tokio::test]
async fn panel_mount_reports_missing_auth_models() {
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let Err(error) = mount(db, Panel::new("admin")) else {
        panic!("a Db without the auth models must not mount a gated panel");
    };
    let error = format!("{error}");
    assert!(
        error.contains("AuthSession") && error.contains("AdminUser"),
        "the error must name the missing models, got {error}"
    );
}

/// A parent row that carries its tenant, and a child that inherits it.
#[derive(Debug, Clone, toasty::Model)]
struct Parent {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    name: String,
    #[has_many]
    children: toasty::Deferred<Vec<Child>>,
}

#[derive(Debug, Clone, toasty::Model)]
struct Child {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    parent_id: uuid::Uuid,
    #[belongs_to(key = parent_id, references = id)]
    parent: toasty::Deferred<Parent>,
    name: String,
}

/// `Tenancy::column` names the model's own column, so a lens through a
/// relation is refused at mount with an error naming the resource and
/// `Tenancy::via`, which is the declaration that path needs.
#[tokio::test]
async fn panel_mount_rejects_a_tenancy_column_through_a_relation() {
    use crate::resource::{Resource, Table, TextColumn};

    fn child_table() -> Table<Child> {
        Table::new(
            |c: &Child| c.id.to_string(),
            TextColumn::r#for(Child::fields().name(), |c: &Child| c.name.clone()),
        )
    }

    struct ColumnThroughRelation;
    impl Resource for ColumnThroughRelation {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "children".to_string()
        }
        fn tenancy() -> Tenancy<Child> {
            Tenancy::column(Child::fields().parent().tenant_id())
        }
        fn table() -> Table<Child> {
            child_table()
        }
    }

    struct Inherited;
    impl Resource for Inherited {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "inherited".to_string()
        }
        fn tenancy() -> Tenancy<Child> {
            Tenancy::via(Child::fields().parent().tenant_id())
        }
        fn table() -> Table<Child> {
            child_table()
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());

    let Err(error) = mount(db.clone(), panel().resource::<ColumnThroughRelation>()) else {
        panic!("a tenant column through a relation must not mount");
    };
    let error = format!("{error}");
    assert!(
        error.contains("ColumnThroughRelation") && error.contains("Tenancy::via"),
        "the error must name the resource and the declaration that fits, got {error}"
    );

    mount(db, panel().resource::<Inherited>()).expect("`Tenancy::via` scopes through a relation");
}

/// A `via` over the model's own column stamps nothing: the mount refuses it
/// in favor of `Tenancy::column`.
#[tokio::test]
async fn panel_mount_rejects_a_tenancy_via_over_its_own_column() {
    use crate::resource::{Resource, Table, TextColumn};

    struct ViaOwnColumn;
    impl Resource for ViaOwnColumn {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "children".to_string()
        }
        fn tenancy() -> Tenancy<Child> {
            Tenancy::via(Child::fields().parent_id())
        }
        fn table() -> Table<Child> {
            Table::new(
                |c: &Child| c.id.to_string(),
                TextColumn::r#for(Child::fields().name(), |c: &Child| c.name.clone()),
            )
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let Err(error) = mount(
        db,
        Panel::new("admin")
            .auth(crate::Auth::disabled())
            .resource::<ViaOwnColumn>(),
    ) else {
        panic!("a `via` over its own column must not mount");
    };
    let error = format!("{error}");
    assert!(
        error.contains("ViaOwnColumn") && error.contains("Tenancy::column"),
        "the error must name the resource and the declaration that fits, got {error}"
    );
}

/// A `via` resource writes its parent key through the form, so a form with no
/// relationship field mounts nothing the write re-checks.
#[tokio::test]
async fn panel_mount_rejects_a_tenancy_via_without_a_relationship_field() {
    use crate::{
        resource::{Resource, Table, TextColumn},
        schema::{Field, Schema},
    };

    #[derive(crate::RecordForm)]
    #[form(model = Child)]
    struct ChildForm {
        name: String,
    }

    struct ViaWithoutRelationship;
    impl Resource for ViaWithoutRelationship {
        type Model = Child;
        type Form = ChildForm;
        fn slug() -> String {
            "children".to_string()
        }
        fn tenancy() -> Tenancy<Child> {
            Tenancy::via(Child::fields().parent().tenant_id())
        }
        fn policy() -> impl Policy<Child> {
            |_cx: &Cx, ability: Ability<'_, Child>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            }
        }
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Child::fields().name()))
        }
        fn table() -> Table<Child> {
            Table::new(
                |c: &Child| c.id.to_string(),
                TextColumn::r#for(Child::fields().name(), |c: &Child| c.name.clone()),
            )
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let Err(error) = mount(
        db,
        Panel::new("admin")
            .auth(crate::Auth::disabled())
            .resource::<ViaWithoutRelationship>(),
    ) else {
        panic!("a `via` with no relationship field must not mount");
    };
    let error = format!("{error}");
    assert!(
        error.contains("ViaWithoutRelationship") && error.contains("no relationship"),
        "the error must name the resource and the missing field, got {error}"
    );
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

    let Err(error) = mount_without_db(Panel::new("admin").resource::<HostileResource>()) else {
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
        fn policy() -> impl Policy<Subscriber> {
            |_cx: &Cx, ability: Ability<'_, Subscriber>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            }
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
    let Err(error) = mount(db, Panel::new("admin").resource::<UnbackedResource>()) else {
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
        fn policy() -> impl Policy<Subscriber> {
            |_cx: &Cx, ability: Ability<'_, Subscriber>| {
                matches!(ability, Ability::DeleteAny | Ability::Delete(_))
            }
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
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());

    mount(db.clone(), panel().resource::<ChromeResource>())
        .expect("a keyed table with chrome builds");
    mount(db.clone(), panel().resource::<ChromeOffResource>())
        .expect("a keyed table without chrome builds");
    mount(db.clone(), panel().resource::<ViewedResource>())
        .expect("a keyed table with a detail view builds");
}

/// The marker is a property of the declaration, not of the policy serving
/// it: a read-only resource — `Create` denied, the default —
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
        fn policy() -> impl Policy<Subscriber> {
            |_cx: &Cx, ability: Ability<'_, Subscriber>| matches!(ability, Ability::ViewAny)
        }
        // `Create` keeps its default (deny); only the form is declared.
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
    let Err(error) = mount(db, Panel::new("admin").resource::<ReadOnlyResource>()) else {
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

    let Err(error) = mount_without_db(
        Panel::new("admin")
            .resource::<FirstResource>()
            .resource::<SecondResource>(),
    ) else {
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
            let Err(error) = mount_without_db(Panel::new("admin").resource::<$name>()) else {
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
    let Err(error) = mount_without_db(Panel::new("adm{in}")) else {
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
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());

    let Err(error) = mount(db.clone(), panel().resource::<DuplicateColumnResource>()) else {
        panic!("a duplicate column name must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("is misdeclared") && error.contains("table: duplicate column name"),
        "the recorded misdeclaration must reach the registration error, got {error}"
    );

    let Err(error) = mount(db.clone(), panel().resource::<TraversalLensResource>()) else {
        panic!("a traversal lens must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("is misdeclared") && error.contains("single-field lens"),
        "the recorded misdeclaration must reach the registration error, got {error}"
    );

    let Err(error) = mount(db.clone(), panel().resource::<ZeroPageResource>()) else {
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
    let router = mount(db, panel_for::<DummyResource>()).expect("panel builds");

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
    // login — and never reaches the root target read.
    let (parts, ()) = http::Request::builder()
        .uri("/admin")
        .body(())
        .unwrap()
        .into_parts();
    let mut gated = panel_state("/admin", crate::Auth::password());
    gated.root_redirect = Some("/admin/users".to_string());
    let gated = current_panel(gated);
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(gated.clone())
        .build();
    let err = match panel_root_redirect(&cx, Body::empty()).await {
        Ok(_) => panic!("unauthenticated root must not read the root target"),
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
    let user = crate::auth::SignedIn {
        user: Arc::new(crate::auth::AdminUser {
            id: uuid::Uuid::nil(),
            email: "ada@example.com".to_string(),
            password_hash: String::new(),
            display_name: "Ada".to_string(),
            active: true,
            created_at: jiff::Timestamp::UNIX_EPOCH,
        }),
        panel: Arc::clone(&gated.0),
        tenant: None,
    };
    let (parts, ()) = http::Request::builder()
        .uri("/admin")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(gated)
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
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    /// The directive the finished page carries.
    async fn policy(db: Db, panel: Panel) -> Option<String> {
        let router = mount(db, panel).expect("panel builds");
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
    let base = panel_for::<DummyResource>;

    assert_eq!(
        policy(db.clone(), base()).await.as_deref(),
        Some("frame-ancestors 'self'"),
        "a panel page must not be frameable by default"
    );
    assert_eq!(
        policy(
            db.clone(),
            base().frame_ancestors("'self' https://intranet.example")
        )
        .await
        .as_deref(),
        Some("frame-ancestors 'self' https://intranet.example"),
        "a deployment that frames the panel says so"
    );
    assert!(
        policy(db, base().without_frame_ancestors()).await.is_none(),
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
    let Err(error) = mount(db, panel_for::<BadView>()) else {
        panic!("a view declaring one field twice must not build");
    };
    let error = format!("{error}");
    assert!(
        error.contains("view: duplicate field name 'name'"),
        "the error names the part and the field, got {error}"
    );
}

/// The served declarations are built once: mounting the panel calls the table, form and view
/// once each (relations twice: once at registration for its handlers and keys, once here for
/// the served copy), and every handler serves the cached copy across requests.
#[tokio::test]
async fn declarations_are_built_once_across_requests() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        resource::{Relation, Resource},
        schema::{Field, Schema},
    };

    static TABLE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static FORM_CALLS: AtomicUsize = AtomicUsize::new(0);
    static VIEW_CALLS: AtomicUsize = AtomicUsize::new(0);
    static RELATIONS_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct CountedForm {
        name: String,
    }

    struct CountedResource;
    impl Resource for CountedResource {
        type Model = Dummy;
        type Form = CountedForm;

        fn slug() -> String {
            "dummies".to_string()
        }

        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            }
        }

        fn table() -> crate::resource::Table<Dummy> {
            TABLE_CALLS.fetch_add(1, Ordering::SeqCst);
            dummy_table()
        }

        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            FORM_CALLS.fetch_add(1, Ordering::SeqCst);
            Schema::new(Field::text(Dummy::fields().name()))
        }

        fn view(_dx: &crate::schema::DeclCx) -> Schema {
            VIEW_CALLS.fetch_add(1, Ordering::SeqCst);
            Schema::new(Field::text(Dummy::fields().name()))
        }

        fn relations() -> Vec<Relation<Dummy>> {
            RELATIONS_CALLS.fetch_add(1, Ordering::SeqCst);
            Vec::new()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let record = toasty::create!(Dummy {
        name: "Ada".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<CountedResource>()).expect("panel builds");
    for (calls, name, want) in [
        (&TABLE_CALLS, "table", 1),
        (&FORM_CALLS, "form", 1),
        (&VIEW_CALLS, "view", 1),
        // `relations` also runs at registration, where the panel derives
        // its relation handlers and keys: one call there, one for the
        // served declarations built here.
        (&RELATIONS_CALLS, "relations", 2),
    ] {
        assert_eq!(
            calls.load(Ordering::SeqCst),
            want,
            "`{name}` builds once, when the panel is mounted"
        );
    }

    for uri in [
        "/admin/dummies".to_string(),
        "/admin/dummies/export".to_string(),
        format!("/admin/dummies/{}", record.id),
        "/admin/dummies/create".to_string(),
        format!("/admin/dummies/{}/edit", record.id),
    ] {
        let response = router
            .handle(
                http::Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), http::StatusCode::OK, "{uri} renders");
    }
    for (calls, name, want) in [
        (&TABLE_CALLS, "table", 1),
        (&FORM_CALLS, "form", 1),
        (&VIEW_CALLS, "view", 1),
        (&RELATIONS_CALLS, "relations", 2),
    ] {
        assert_eq!(
            calls.load(Ordering::SeqCst),
            want,
            "`{name}` serves requests from the cached build"
        );
    }
}

/// A resource no panel registers builds on demand: `declared` falls back to
/// a fresh build from the request's app schema, which is what a page-owned
/// table reads.
#[tokio::test]
async fn declared_fallback_builds_for_an_unregistered_resource() {
    use crate::resource::{Resource, declared};

    struct FallbackResource;
    impl Resource for FallbackResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let cx = topcoat::context::CxTestBuilder::new()
        .app_context(db)
        .build();
    let fallback = declared::<FallbackResource>(&cx);
    assert!(fallback.table.declaration_errors().is_empty());
    assert!(fallback.form.declaration_errors().is_empty());
}
