use toasty::Db;

use super::*;
use crate::{
    Ability, ResourceDef, SelectFilter, Tenancy, TenantId, lens,
    panel::test_support::{
        Dummy, current_panel, dummy_table, mount, mount_without_db, panel_for, panel_state, refusal,
    },
    test_support::{memory_db, tableless_db},
};

/// Declares the resource `$name` over `$model` in one statement, with `$def` as its `declare`
/// body. Without a form the resource is list-only (`NoForm`); `$form` names the form it writes
/// through.
macro_rules! resource {
    ($name:ident, $model:ty => $def:expr) => {
        resource!($name, $model, crate::NoForm<$model> => $def);
    };
    ($name:ident, $model:ty, $form:ty => $def:expr) => {
        struct $name;

        impl crate::resource::Resource for $name {
            type Model = $model;
            type Form = $form;

            fn declare() -> ResourceDef<Self> {
                $def
            }
        }
    };
}

#[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
struct Subscriber {
    #[key]
    #[auto]
    id: uuid::Uuid,
    nickname: String,
}

/// Builds and resolves a slug of ordinary URL-segment characters.
#[tokio::test]
async fn a_plain_slug_builds_and_resolves() {
    use crate::resource::Resource;

    struct PlainResource;
    impl Resource for PlainResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("user-profiles_2")
                .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
                .table(
                    crate::table::Table::new(crate::table::TextColumn::new(lens!(Dummy.name)))
                        .paginate(25),
                )
        }
    }

    let db = memory_db(toasty::models!(Dummy)).await;
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

/// Builds and resolves a slug containing `*`.
#[tokio::test]
async fn a_star_slug_builds_and_resolves() {
    use crate::resource::Resource;

    struct StarResource;
    impl Resource for StarResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("user*profiles")
                .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
                .table(
                    crate::table::Table::new(crate::table::TextColumn::new(lens!(Dummy.name)))
                        .paginate(25),
                )
        }
    }

    let db = memory_db(toasty::models!(Dummy)).await;
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

/// Enforces CSRF with `Auth::disabled()`.
#[tokio::test]
async fn csrf_is_enforced_with_auth_disabled() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = DummyForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy_fn(|_cx, ability| matches!(ability, Ability::Create))
                .table(dummy_table())
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct DummyForm {
        name: String,
    }
    let db = memory_db(toasty::models!(Dummy)).await;
    let router =
        mount(db, panel_for::<DummyResource>()).expect("the explicit opt-out builds the panel");

    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/dummies/-/create")
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

/// Applies `Panel::dark_mode` to the rendered document's `<html class>`.
#[tokio::test]
async fn dark_mode_sets_the_document_class() {
    let db = memory_db(toasty::models!(
        crate::auth::AdminUser,
        crate::auth::AuthSession
    ))
    .await;
    let router = mount(
        db,
        Panel::new("admin")
            .auth(crate::Auth::password())
            .dark_mode(true),
    )
    .expect("panel builds");

    // The login page carries the theme class without a session.
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

/// A parent row that carries its tenant, and a child that inherits it.
#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
struct Parent {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    name: String,
    #[has_many]
    children: toasty::Deferred<Vec<Child>>,
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
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

use crate::{
    RelationColumn, relation,
    schema::Schema,
    table::{Table, TextColumn},
};

#[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
struct Doc {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
#[key(owner, slot)]
struct Seat {
    owner: String,
    slot: i64,
    label: String,
}

#[derive(Debug, Clone, toasty::Embed)]
struct Place {
    city: String,
}

#[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
struct Ticket {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    title: String,
    place: Place,
}

#[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
#[unique(tenant_id, email)]
struct Author {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: TenantId,
    email: String,
}

#[derive(crate::RecordForm)]
#[form(model = Child)]
struct NameForm {
    name: String,
}

#[derive(crate::RecordForm)]
#[form(model = Child)]
struct KeyedForm {
    name: String,
    parent_id: uuid::Uuid,
}

#[derive(crate::RecordForm)]
#[form(model = Child)]
struct SeatedForm {
    name: String,
    #[form(relationship = Seats)]
    parent_id: uuid::Uuid,
}

#[derive(crate::RecordForm)]
#[form(model = Subscriber)]
struct UnbackedForm {
    nickname: String,
}

#[derive(crate::RecordForm)]
#[form(model = Subscriber)]
struct ReadOnlyForm {
    nickname: String,
}

#[derive(crate::RecordForm)]
#[form(model = Subscriber)]
struct ChromeForm {
    nickname: String,
}

#[derive(crate::RecordForm)]
#[form(model = Author)]
struct AuthorForm {
    email: String,
}

#[derive(crate::RecordForm)]
#[form(model = Ticket)]
struct TicketForm {
    title: String,
}

/// [`Subscriber`]'s keyed table: one nickname column.
fn keyed_table() -> Table<Subscriber> {
    Table::new(TextColumn::new(lens!(Subscriber.nickname)))
}

resource!(Parents, Parent => ResourceDef::new().table(Table::new(TextColumn::new(lens!(Parent.name)))));
resource!(FilteredChildren, Child => ResourceDef::new()
.slug("filtered-children")
.table(
    Table::new(TextColumn::new(lens!(Child.name)))
        .filters(SelectFilter::relationship::<Parents>(lens!(Child.parent_id)))
));
resource!(Children, Child => ResourceDef::new()
.table(Table::new((
    TextColumn::new(lens!(Child.name)),
    RelationColumn::of::<Parents>(relation!(Child.parent)),
)))
.detail(crate::Detail::new(RelationColumn::of::<Parents>(
    relation!(Child.parent),
))));

resource!(ColumnThroughRelation, Child => ResourceDef::new()
    .slug("children")
    .tenancy(Tenancy::column(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name)))));
resource!(Inherited, Child => ResourceDef::new()
    .slug("inherited")
    .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name)))));
resource!(ViaOwnColumn, Parent => ResourceDef::new()
    .slug("parents")
    .tenancy(Tenancy::via(Parent::fields().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
resource!(ScopedParents, Parent => ResourceDef::new()
    .slug("parents")
    .tenancy(Tenancy::column(Parent::fields().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));
resource!(OpenParents, Parent => ResourceDef::new()
    .slug("open-parents")
    .table(Table::new(TextColumn::new(lens!(Parent.name)))));

// A child resource inheriting its tenant through `parent`, whose form declares the key.
resource!(WithoutKey, Child, NameForm => ResourceDef::new()
    .slug("children")
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::ViewAny | Ability::Create)
    })
    .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
    .table(Table::new(TextColumn::new(lens!(Child.name))))
    .form(Schema::default()));
resource!(OverOpenParent, Child, KeyedForm => ResourceDef::new()
.slug("children")
.policy_fn(|_cx, ability| {
    matches!(ability, Ability::ViewAny | Ability::Create)
})
.tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
.table(Table::new(TextColumn::new(lens!(Child.name))))
.form({
    let c = KeyedForm::controls();
    Schema::new((c.name, c.parent_id.choice().relationship::<OpenParents>()))
}));
resource!(OverScopedParent, Child, KeyedForm => ResourceDef::new()
.slug("children")
.policy_fn(|_cx, ability| {
    matches!(ability, Ability::ViewAny | Ability::Create)
})
.tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
.table(Table::new(TextColumn::new(lens!(Child.name))))
.form({
    let c = KeyedForm::controls();
    Schema::new((c.name, c.parent_id.choice().relationship::<ScopedParents>()))
}));

resource!(Seats, Seat => ResourceDef::new()
    .table(Table::new(TextColumn::new(lens!(Seat.label)))));
resource!(Seated, Child, SeatedForm => {
    let c = SeatedForm::controls();
    ResourceDef::new()
        .policy_fn(|_cx, ability| {
            matches!(ability, Ability::ViewAny | Ability::Create)
        })
        .table(Table::new(TextColumn::new(lens!(Child.name))))
        .form(Schema::new((c.name, c.parent_id)))
});

// Not gated, so the tenant is not stamped: a create override would set it.
resource!(AuthorResource, Author, AuthorForm => ResourceDef::new()
    .slug("authors")
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::ViewAny | Ability::Create)
    })
    .table(Table::new(TextColumn::new(lens!(Author.email))))
    .form(Schema::new(AuthorForm::controls().email.unique()))
    .create_column(Author::fields().tenant_id()));
resource!(UnbackedResource, Subscriber, UnbackedForm => ResourceDef::new()
    .slug("subscribers")
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::ViewAny | Ability::Create)
    })
    .table(Table::new(TextColumn::new(lens!(Subscriber.nickname))))
    .form(Schema::new(UnbackedForm::controls().nickname.unique())));
// `Create` keeps its default (deny); only the form is declared.
resource!(ReadOnlyResource, Subscriber, ReadOnlyForm => ResourceDef::new()
    .slug("subscribers")
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::ViewAny)
    })
    .table(Table::new(TextColumn::new(lens!(Subscriber.nickname))))
    .form(Schema::new(ReadOnlyForm::controls().nickname.unique())));
resource!(ChromeResource, Subscriber, ChromeForm => ResourceDef::new()
    .slug("subscribers")
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::DeleteAny | Ability::Delete(_))
    })
    .table(keyed_table()));
resource!(ChromeOffResource, Subscriber => ResourceDef::new()
    .slug("subscribers")
    .table(keyed_table()));
resource!(ViewedResource, Subscriber => ResourceDef::new()
.slug("subscribers")
.table(keyed_table())
.detail(crate::Detail::new(crate::table::TextColumn::new(lens!(
    Subscriber.nickname
)))));

// Two columns over one field: a duplicate name.
resource!(DuplicateColumnResource, Doc => ResourceDef::new()
.slug("docs")
.table(Table::new((
    TextColumn::new(lens!(Doc.title)),
    TextColumn::new(lens!(Doc.title)),
))));
// A page of no rows: `Table::paginate` records zero.
resource!(ZeroPageResource, Doc => ResourceDef::new()
    .slug("docs")
    .table(Table::new(TextColumn::new(lens!(Doc.title))).paginate(0)));

resource!(Tickets, Ticket, TicketForm => ResourceDef::new()
    .slug("tickets")
    .tenancy(Tenancy::column(Ticket::fields().tenant_id()))
    .policy_fn(|_cx, ability| {
        matches!(ability, Ability::ViewAny | Ability::Create)
    })
    .table(Table::new(TextColumn::new(lens!(Ticket.title))))
    .create_column(Ticket::fields().tenant_id())
    .create_column(Ticket::fields().place().city()));

resource!(UnbindableView, Dummy => ResourceDef::new()
    .table(dummy_table())
    .detail(crate::Detail::empty().column(Unbindable)));

#[tokio::test]
async fn panel_mounts_runtime_page_rerun_routes() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(dummy_table())
        }
    }

    let db = Db::builder().connect("sqlite::memory:").await.unwrap();
    let router = mount(db, panel_for::<DummyResource>()).expect("panel builds");

    // A marked POST rewrites into a GET for the page's URL.
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

/// Re-checks auth before reading the root target.
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

/// Sends `frame-ancestors` unless opted out.
#[tokio::test]
async fn panel_sends_frame_ancestors_unless_opted_out() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                // A rendered page, not the default-deny 403: an error response is
                // produced above the layer chain, so only a served document proves
                // the header is installed.
                .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
                .table(dummy_table())
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

    let db = memory_db(toasty::models!(Dummy)).await;
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

/// A served directory's path is a route pattern ending in a
/// catch-all, and only that; everything else is a build error.
#[test]
fn serve_dir_accepts_only_a_catch_all_pattern() {
    assert!(is_directory_pattern("/uploads/{*file}"));
    assert!(is_directory_pattern("/{*file}"));
    assert!(is_directory_pattern("/up loads/{*file}"));
    assert!(!is_directory_pattern("/uploads"));
    assert!(!is_directory_pattern("/uploads/"));
    assert!(!is_directory_pattern("/uploads/{file}"));
    assert!(!is_directory_pattern("/uploads/{*}"));
    assert!(!is_directory_pattern("/{*file}/more"));
    assert!(!is_directory_pattern("/uploads/{*file"));
    assert!(!is_directory_pattern("/uploads//{*file}"));
    assert!(!is_directory_pattern("/uploads/{*fi-le}"));
}

/// A detail column whose path binds no single field.
struct Unbindable;

impl crate::table::Column<Dummy> for Unbindable {
    fn name(&self) -> &str {
        "unbindable"
    }

    fn label(&self) -> &str {
        "Unbindable"
    }

    fn text(&self, _cx: &Cx, _row: &Dummy) -> String {
        String::new()
    }

    fn misdeclared(&self) -> Option<DeclarationErrorKind> {
        Some(DeclarationErrorKind::TraversalLens { steps: 2 })
    }
}

/// Declares each resource once, when the panel mounts, and serves every request from that build.
#[tokio::test]
async fn a_resource_declares_once_when_its_panel_mounts() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::resource::Resource;

    static DECLARE_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct CountedForm {
        name: String,
    }

    struct CountedResource;
    impl Resource for CountedResource {
        type Model = Dummy;
        type Form = CountedForm;

        fn declare() -> ResourceDef<Self> {
            DECLARE_CALLS.fetch_add(1, Ordering::SeqCst);
            ResourceDef::new()
                .slug("dummies")
                .policy_fn(|_cx, ability| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                    )
                })
                .table(dummy_table())
                .detail(crate::Detail::new(crate::table::TextColumn::new(lens!(
                    Dummy.name
                ))))
        }
    }

    let mut db = memory_db(toasty::models!(Dummy)).await;
    let record = toasty::create!(Dummy {
        name: "Ada".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<CountedResource>()).expect("panel builds");
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);

    for uri in [
        "/admin/dummies".to_string(),
        "/admin/dummies/-/export".to_string(),
        format!("/admin/dummies/{}", record.id),
        "/admin/dummies/-/create".to_string(),
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
    assert_eq!(
        DECLARE_CALLS.load(Ordering::SeqCst),
        1,
        "requests are served from the mounted def"
    );
}

/// A resource no panel in the context mounts has no def: its policy allows nothing and its
/// scoped query is refused, whether the context holds another panel or none at all.
#[tokio::test]
async fn an_unmounted_resource_is_refused_not_rebuilt() {
    use crate::resource::Resource;

    struct UnmountedResource;
    impl Resource for UnmountedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().policy(crate::Allow).table(dummy_table())
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    let bare = topcoat::context::CxTestBuilder::new()
        .app_context(db.clone())
        .build();
    let panel = Panel::new("admin")
        .handle(&db)
        .expect("panel builds")
        .context();
    for cx in [bare, panel] {
        assert!(!crate::can::<UnmountedResource>(&cx, Ability::ViewAny));
        let refused = crate::scoped_query::<UnmountedResource>(&cx).expect_err("not mounted");
        assert!(refused.to_string().contains("not mounted"), "{refused}");
    }
}

/// A mounted panel's handle builds each context from the def the mount declared: the resource
/// answers as mounted, and `declare` does not run again.
#[tokio::test]
async fn a_panel_handle_answers_from_the_mounted_def() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::resource::Resource;

    static DECLARE_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct HandledResource;
    impl Resource for HandledResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            DECLARE_CALLS.fetch_add(1, Ordering::SeqCst);
            ResourceDef::new()
                .policy(crate::ReadOnly)
                .table(dummy_table())
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    let builder = topcoat::router::Router::builder()
        .app_context(db)
        .panel(panel_for::<HandledResource>())
        .expect("panel builds");
    assert!(builder.panel_handle("portal").is_none(), "no panel there");
    let handle = builder
        .panel_handle("/admin/")
        .expect("the panel at /admin");
    for cx in [handle.context(), handle.clone().context()] {
        assert!(crate::can::<HandledResource>(&cx, Ability::ViewAny));
        assert!(!crate::can::<HandledResource>(&cx, Ability::Create));
    }
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);
}

/// [`Panel::handle`] refuses a misdeclared resource with the errors a router mount reports.
#[tokio::test]
async fn handle_refuses_a_misdeclared_resource() {
    use crate::resource::Resource;

    struct BadView;
    impl Resource for BadView {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .table(dummy_table())
                .detail(crate::Detail::empty().column(Unbindable))
        }
    }

    let db = tableless_db(toasty::models!(Dummy)).await;
    assert_eq!(
        refusal(panel_for::<BadView>().handle(&db)),
        refusal(mount(db, panel_for::<BadView>()))
    );
}

/// Refuses a panel whose mistake needs no database to find: a panel with no `Db`, and a slug or
/// prefix that is not one route segment. A panel built without a `Db` also reports that it has
/// none.
#[test]
fn mounting_refuses_each_misdeclaration_without_a_db() {
    resource!(HostileResource, Dummy => ResourceDef::new().slug("a\"b\r\n").table(dummy_table()));
    resource!(FirstResource, Dummy => ResourceDef::new().slug("dummies").table(dummy_table()));
    resource!(SecondResource, Dummy => ResourceDef::new().slug("dummies").table(dummy_table()));
    resource!(BraceOpen, Dummy => ResourceDef::new().slug("a{b").table(dummy_table()));
    resource!(BraceClose, Dummy => ResourceDef::new().slug("a}b").table(dummy_table()));
    resource!(ParenOpen, Dummy => ResourceDef::new().slug("a(b").table(dummy_table()));
    resource!(ParenClose, Dummy => ResourceDef::new().slug("a)b").table(dummy_table()));

    let no_db = || DeclarationError::panel(DeclarationErrorKind::MissingDb);
    let refusals = [
        // A panel with no `Db` is a configuration error, not a panic.
        (
            "a panel with no Db",
            mount_without_db(Panel::new("admin")),
            vec![no_db()],
        ),
        // Rejects a hostile slug at registration.
        (
            "a hostile slug",
            mount_without_db(Panel::new("admin").resource::<HostileResource>()),
            vec![
                DeclarationError::of::<HostileResource>(
                    Site::Registration,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment: "a\"b\r\n".to_string(),
                        fault: crate::SegmentFault::Char('"'),
                    },
                ),
                no_db(),
            ],
        ),
        // Rejects duplicate resource slugs.
        (
            "duplicate resource slugs",
            mount_without_db(
                Panel::new("admin")
                    .resource::<FirstResource>()
                    .resource::<SecondResource>(),
            ),
            vec![
                DeclarationError::of::<SecondResource>(
                    Site::Registration,
                    DeclarationErrorKind::DuplicateSlug {
                        slug: "dummies".to_string(),
                    },
                ),
                no_db(),
            ],
        ),
        // Rejects route pattern characters in a slug.
        (
            "a `{` in a slug",
            mount_without_db(Panel::new("admin").resource::<BraceOpen>()),
            vec![
                DeclarationError::of::<BraceOpen>(
                    Site::Registration,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment: "a{b".to_string(),
                        fault: crate::SegmentFault::Char('{'),
                    },
                ),
                no_db(),
            ],
        ),
        (
            "a `}` in a slug",
            mount_without_db(Panel::new("admin").resource::<BraceClose>()),
            vec![
                DeclarationError::of::<BraceClose>(
                    Site::Registration,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment: "a}b".to_string(),
                        fault: crate::SegmentFault::Char('}'),
                    },
                ),
                no_db(),
            ],
        ),
        (
            "a `(` in a slug",
            mount_without_db(Panel::new("admin").resource::<ParenOpen>()),
            vec![
                DeclarationError::of::<ParenOpen>(
                    Site::Registration,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment: "a(b".to_string(),
                        fault: crate::SegmentFault::Char('('),
                    },
                ),
                no_db(),
            ],
        ),
        (
            "a `)` in a slug",
            mount_without_db(Panel::new("admin").resource::<ParenClose>()),
            vec![
                DeclarationError::of::<ParenClose>(
                    Site::Registration,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment: "a)b".to_string(),
                        fault: crate::SegmentFault::Char(')'),
                    },
                ),
                no_db(),
            ],
        ),
        // The prefix goes through the same rule, once per segment.
        (
            "a `{` in a panel prefix",
            mount_without_db(Panel::new("adm{in}")),
            vec![
                DeclarationError::panel(DeclarationErrorKind::InvalidSegment {
                    item: "panel prefix",
                    segment: "adm{in}".to_string(),
                    fault: crate::SegmentFault::Char('{'),
                }),
                no_db(),
            ],
        ),
    ];
    for (label, mounted, expected) in refusals {
        assert_eq!(refusal(mounted), expected, "{label}");
    }
}

/// Refuses each declaration mistake a panel mounts with, naming the site and kind of every
/// error, and mounts the valid declarations that sit beside them.
#[tokio::test]
async fn mounting_refuses_each_misdeclaration() {
    let db = tableless_db(toasty::models!(
        Dummy, Parent, Child, Subscriber, Doc, Seat, Ticket, Author
    ))
    .await;
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());
    // The relationship fields' option sources are the panel's own resources.
    let via_panel = || {
        panel()
            .resource::<OpenParents>()
            .resource::<ScopedParents>()
    };

    let unguarded = DeclarationErrorKind::UnguardedForeignKey {
        relation: "parent".to_string(),
        keys: vec!["parent_id".to_string()],
    };
    let unregistered = DeclarationErrorKind::UnregisteredLabelSource {
        column: "parent".to_string(),
        source: std::any::type_name::<Parents>(),
    };

    let refusals = [
        // Reports a `Db` missing the shipped auth models as a mount error naming them.
        (
            "a Db missing the auth models",
            mount(db.clone(), Panel::new("admin")),
            vec![DeclarationError::panel(
                DeclarationErrorKind::MissingAuthModels {
                    models: vec!["AuthSession", "AdminUser"],
                },
            )],
        ),
        // Refuses a relation column labelled by a resource the panel does not register, which
        // would otherwise show every related record by its bare key.
        (
            "a relation column of an unregistered resource",
            mount(db.clone(), panel().resource::<Children>()),
            vec![
                DeclarationError::of::<Children>(Site::Table, unregistered.clone()),
                DeclarationError::of::<Children>(Site::Detail, unregistered),
            ],
        ),
        // Refuses a relationship filter over a resource the panel does not register, whose
        // control would otherwise render no option at all.
        (
            "a relationship filter of an unregistered resource",
            mount(db.clone(), panel().resource::<FilteredChildren>()),
            vec![DeclarationError::of::<FilteredChildren>(
                Site::Table,
                DeclarationErrorKind::UnregisteredOptionSource {
                    field: "parent_id".to_string(),
                    source: std::any::type_name::<Parents>(),
                },
            )],
        ),
        // Refuses a tenancy column through a relation.
        (
            "a tenancy column through a relation",
            mount(db.clone(), panel().resource::<ColumnThroughRelation>()),
            vec![DeclarationError::of::<ColumnThroughRelation>(
                Site::Tenancy,
                DeclarationErrorKind::TenancyColumnNotAField,
            )],
        ),
        // Refuses a tenancy `via` over its own column.
        (
            "a tenancy via over its own column",
            mount(db.clone(), panel().resource::<ViaOwnColumn>()),
            vec![DeclarationError::of::<ViaOwnColumn>(
                Site::Tenancy,
                DeclarationErrorKind::TenancyViaOwnColumn,
            )],
        ),
        // Refuses a tenancy `via` unless the form writes the relation's foreign key through a
        // relationship field over a tenant-scoped resource.
        (
            "a tenancy via without a key over a scoped parent",
            mount(db.clone(), via_panel().resource::<WithoutKey>()),
            vec![
                DeclarationError::of::<WithoutKey>(Site::Form, unguarded.clone()),
                DeclarationError::of::<WithoutKey>(
                    Site::Form,
                    DeclarationErrorKind::UnwrittenColumn {
                        column: "parent_id".to_string(),
                    },
                ),
            ],
        ),
        (
            "a tenancy via over an open parent",
            mount(db.clone(), via_panel().resource::<OverOpenParent>()),
            vec![DeclarationError::of::<OverOpenParent>(
                Site::Form,
                unguarded,
            )],
        ),
        // Refuses a relationship over a model whose composite primary key no option value can
        // spell. `Seats` is not registered, so its option source is refused too.
        (
            "a relationship over a composite key",
            mount(db.clone(), panel().resource::<Seated>()),
            vec![
                DeclarationError::of::<Seated>(
                    Site::Form,
                    DeclarationErrorKind::CompositeKeyChoice {
                        field: "parent_id".to_string(),
                    },
                ),
                DeclarationError::of::<Seated>(
                    Site::Form,
                    DeclarationErrorKind::UnregisteredOptionSource {
                        field: "parent_id".to_string(),
                        source: std::any::type_name::<Seats>(),
                    },
                ),
            ],
        ),
        // Rejects a `unique()` marker without a unique index.
        (
            "a unique marker without a unique index",
            mount(db.clone(), panel_for::<UnbackedResource>()),
            vec![DeclarationError::of::<UnbackedResource>(
                Site::Form,
                DeclarationErrorKind::UniqueWithoutIndex {
                    field: "nickname".to_string(),
                },
            )],
        ),
        // Rejects an unbacked `unique()` marker even when create is denied.
        (
            "an unbacked unique marker when create is denied",
            mount(db.clone(), panel_for::<ReadOnlyResource>()),
            vec![DeclarationError::of::<ReadOnlyResource>(
                Site::Form,
                DeclarationErrorKind::UniqueWithoutIndex {
                    field: "nickname".to_string(),
                },
            )],
        ),
        // Reports recorded table misdeclarations.
        (
            "a duplicate table column",
            mount(db.clone(), panel().resource::<DuplicateColumnResource>()),
            vec![DeclarationError::of::<DuplicateColumnResource>(
                Site::Table,
                DeclarationErrorKind::DuplicateColumn {
                    name: "title".to_string(),
                },
            )],
        ),
        (
            "a table with a zero page size",
            mount(db.clone(), panel().resource::<ZeroPageResource>()),
            vec![DeclarationError::of::<ZeroPageResource>(
                Site::Table,
                DeclarationErrorKind::ZeroPageSize,
            )],
        ),
        // Rejects a misdeclared view.
        (
            "a detail column whose path binds no field",
            mount(db.clone(), panel_for::<UnbindableView>()),
            vec![DeclarationError::of::<UnbindableView>(
                Site::Detail,
                DeclarationErrorKind::TraversalLens { steps: 2 },
            )],
        ),
        // `create_column` refuses the tenant column, which the framework stamps, and an embedded
        // path, which names no one field of the model. The refused columns are never written, so
        // the form reports the embedded one unwritten too.
        (
            "a create column it cannot honor",
            mount(db.clone(), panel_for::<Tickets>()),
            vec![
                DeclarationError::of::<Tickets>(
                    Site::Registration,
                    DeclarationErrorKind::TraversalLens { steps: 2 },
                ),
                DeclarationError::of::<Tickets>(
                    Site::Registration,
                    DeclarationErrorKind::CreateColumnsNameTenant {
                        column: "tenant_id".to_string(),
                    },
                ),
                DeclarationError::of::<Tickets>(
                    Site::Form,
                    DeclarationErrorKind::UnwrittenColumn {
                        column: "place".to_string(),
                    },
                ),
            ],
        ),
    ];
    for (label, mounted, expected) in refusals {
        assert_eq!(refusal(mounted), expected, "{label}");
    }

    // Declarations a panel must mount: the valid shapes beside each refusal above.
    let mounts = [
        (
            "a relation column of a registered resource mounts",
            mount(
                db.clone(),
                panel().resource::<Children>().resource::<Parents>(),
            ),
        ),
        (
            "`Tenancy::via` scopes through a relation",
            mount(db.clone(), panel().resource::<Inherited>()),
        ),
        (
            "a foreign key over the tenant-scoped parent mounts",
            mount(db.clone(), via_panel().resource::<OverScopedParent>()),
        ),
        (
            "a composite unique index backs the marker",
            mount(db.clone(), panel_for::<AuthorResource>()),
        ),
        (
            "a keyed table with chrome builds",
            mount(db.clone(), panel().resource::<ChromeResource>()),
        ),
        (
            "a keyed table without chrome builds",
            mount(db.clone(), panel().resource::<ChromeOffResource>()),
        ),
        (
            "a keyed table with a detail view builds",
            mount(db.clone(), panel().resource::<ViewedResource>()),
        ),
    ];
    for (label, mounted) in mounts {
        mounted.expect(label);
    }
}

/// `Panel::endpoint` binds a shard or procedure: a route outside Topcoat's runtime path refuses to
/// mount, since the runtime gate never sees it.
#[test]
fn an_endpoint_outside_the_runtime_path_refuses_the_panel() {
    let route = RouteFn::new(
        http::Method::GET,
        route_path("/reports"),
        panel_root_redirect,
    );
    let errors = refusal(mount_without_db(Panel::new("admin").endpoint(route)));
    assert!(
        errors.iter().any(|error| error.kind
            == DeclarationErrorKind::EndpointOutsideRuntime {
                path: "/reports".to_string()
            }),
        "{errors:?}"
    );
}
