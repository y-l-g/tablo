use toasty::Db;

use super::*;
use crate::{
    Ability, ResourceDef, Tenancy, lens,
    panel::test_support::{
        Dummy, current_panel, dummy_table, mount, mount_without_db, panel_for, panel_state, refusal,
    },
};

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
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(
                    crate::table::Table::new(crate::table::TextColumn::new(lens!(Dummy.name)))
                        .paginate(25),
                )
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
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(
                    crate::table::Table::new(crate::table::TextColumn::new(lens!(Dummy.name)))
                        .paginate(25),
                )
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

/// Enforces CSRF with `Auth::disabled()`.
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::Create))
                .table(dummy_table())
                .form(Schema::new(Field::text(Dummy::fields().name())))
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

/// Applies `Panel::dark_mode` to the rendered document's `<html class>`.
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

/// Accepts unique markers with a backing index.
#[tokio::test]
async fn panel_build_accepts_unique_markers_with_a_backing_index() {
    use crate::{
        resource::{Resource, ResourceDef},
        schema::{Field, Schema},
        table::{Table, TextColumn},
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("authors")
                .policy(|_cx: &Cx, ability: Ability<'_, Author>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Author.email))))
                .form(Schema::new(Field::text(Author::fields().email()).unique()))
                // Not gated, so the tenant is not stamped: a create override would
                // set it.
                .create_columns(["tenant_id"])
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

/// A panel with no `Db` is a configuration error, not a panic.
#[test]
fn panel_build_errors_without_db() {
    assert_eq!(
        refusal(mount_without_db(Panel::new("admin"))),
        [DeclarationError::panel(DeclarationErrorKind::MissingDb)]
    );
}

/// Reports a `Db` missing the shipped auth models as a mount error naming them.
#[tokio::test]
async fn panel_mount_reports_missing_auth_models() {
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    assert_eq!(
        refusal(mount(db, Panel::new("admin"))),
        [DeclarationError::panel(
            DeclarationErrorKind::MissingAuthModels {
                models: vec!["AuthSession", "AdminUser"],
            }
        )]
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

/// Refuses a tenancy column through a relation.
#[tokio::test]
async fn panel_mount_rejects_a_tenancy_column_through_a_relation() {
    use crate::{
        resource::Resource,
        table::{Table, TextColumn},
    };

    fn child_table() -> Table<Child> {
        Table::new(TextColumn::new(lens!(Child.name)))
    }

    struct ColumnThroughRelation;
    impl Resource for ColumnThroughRelation {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("children")
                .tenancy(Tenancy::column(Child::fields().parent().tenant_id()))
                .table(child_table())
        }
    }

    struct Inherited;
    impl Resource for Inherited {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("inherited")
                .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
                .table(child_table())
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());

    assert_eq!(
        refusal(mount(
            db.clone(),
            panel().resource::<ColumnThroughRelation>()
        )),
        [DeclarationError::of::<ColumnThroughRelation>(
            Site::Tenancy,
            DeclarationErrorKind::TenancyColumnNotAField,
        )]
    );

    mount(db, panel().resource::<Inherited>()).expect("`Tenancy::via` scopes through a relation");
}

/// Refuses a tenancy `via` over its own column.
#[tokio::test]
async fn panel_mount_rejects_a_tenancy_via_over_its_own_column() {
    use crate::{
        resource::Resource,
        table::{Table, TextColumn},
    };

    struct ViaOwnColumn;
    impl Resource for ViaOwnColumn {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("children")
                .tenancy(Tenancy::via(Child::fields().parent_id()))
                .table(Table::new(TextColumn::new(lens!(Child.name))))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    assert_eq!(
        refusal(mount(
            db,
            Panel::new("admin")
                .auth(crate::Auth::disabled())
                .resource::<ViaOwnColumn>(),
        )),
        [DeclarationError::of::<ViaOwnColumn>(
            Site::Tenancy,
            DeclarationErrorKind::TenancyViaOwnColumn,
        )]
    );
}

/// Refuses a tenancy `via` unless the form writes the relation's foreign key through a
/// relationship field over a tenant-scoped resource.
#[tokio::test]
async fn panel_mount_requires_a_tenancy_via_key_over_a_scoped_parent() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
        table::{Table, TextColumn},
    };

    fn parent_table() -> Table<Parent> {
        Table::new(TextColumn::new(lens!(Parent.name)))
    }

    struct ScopedParents;
    impl Resource for ScopedParents {
        type Model = Parent;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("parents")
                .tenancy(Tenancy::column(Parent::fields().tenant_id()))
                .table(parent_table())
        }
    }

    struct OpenParents;
    impl Resource for OpenParents {
        type Model = Parent;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("open-parents")
                .table(parent_table())
        }
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
        #[form(choice)]
        parent_id: uuid::Uuid,
    }

    /// A child resource inheriting its tenant through `parent`, whose form `form` declares.
    macro_rules! via_child {
        ($name:ident, $form:ty, $schema:expr) => {
            struct $name;
            impl Resource for $name {
                type Model = Child;
                type Form = $form;

                fn declare() -> ResourceDef<Self> {
                    ResourceDef::new()
                        .slug("children")
                        .policy(|_cx: &Cx, ability: Ability<'_, Child>| {
                            matches!(ability, Ability::ViewAny | Ability::Create)
                        })
                        .tenancy(Tenancy::via(Child::fields().parent().tenant_id()))
                        .table(Table::new(TextColumn::new(lens!(Child.name))))
                        .form($schema())
                }
            }
        };
    }

    via_child!(WithoutKey, NameForm, || Schema::new(Field::text(
        Child::fields().name()
    )));
    via_child!(OverOpenParent, KeyedForm, || {
        let c = KeyedForm::controls();
        Schema::new((
            c.name,
            c.parent_id
                .relationship::<OpenParents>(|p: &Parent| p.name.clone()),
        ))
    });
    via_child!(OverScopedParent, KeyedForm, || {
        let c = KeyedForm::controls();
        Schema::new((
            c.name,
            c.parent_id
                .relationship::<ScopedParents>(|p: &Parent| p.name.clone()),
        ))
    });

    let db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // The relationship fields' option sources are the panel's own resources.
    let panel = || {
        Panel::new("admin")
            .auth(crate::Auth::disabled())
            .resource::<OpenParents>()
            .resource::<ScopedParents>()
    };

    let unguarded = DeclarationErrorKind::UnguardedForeignKey {
        relation: "parent".to_string(),
        keys: vec!["parent_id".to_string()],
    };
    let errors = refusal(mount(db.clone(), panel().resource::<WithoutKey>()));
    assert!(
        errors.contains(&DeclarationError::of::<WithoutKey>(
            Site::Form,
            unguarded.clone()
        )),
        "{errors:?}"
    );
    assert_eq!(
        refusal(mount(db.clone(), panel().resource::<OverOpenParent>())),
        [DeclarationError::of::<OverOpenParent>(
            Site::Form,
            unguarded
        )]
    );
    mount(db, panel().resource::<OverScopedParent>())
        .expect("a foreign key over the tenant-scoped parent mounts");
}

/// Refuses a relationship over a model whose composite primary key no option value can spell.
#[tokio::test]
async fn panel_mount_rejects_a_relationship_over_a_composite_key() {
    use crate::{
        resource::Resource,
        schema::Schema,
        table::{Table, TextColumn},
    };

    #[derive(Debug, Clone, toasty::Model)]
    #[key(owner, slot)]
    struct Seat {
        owner: String,
        slot: i64,
        label: String,
    }

    struct Seats;
    impl Resource for Seats {
        type Model = Seat;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(Table::new(TextColumn::new(lens!(Seat.label))))
        }
    }

    #[derive(crate::RecordForm)]
    #[form(model = Child)]
    struct SeatedForm {
        name: String,
        #[form(choice)]
        parent_id: uuid::Uuid,
    }

    struct Seated;
    impl Resource for Seated {
        type Model = Child;
        type Form = SeatedForm;

        fn declare() -> ResourceDef<Self> {
            let c = SeatedForm::controls();
            ResourceDef::new()
                .policy(|_cx: &Cx, ability: Ability<'_, Child>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Child.name))))
                .form(Schema::new((
                    c.name,
                    c.parent_id
                        .relationship::<Seats>(|s: &Seat| s.label.clone()),
                )))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Parent, Child, Seat))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let errors = refusal(mount(
        db,
        Panel::new("admin")
            .auth(crate::Auth::disabled())
            .resource::<Seated>(),
    ));
    assert!(
        errors.contains(&DeclarationError::of::<Seated>(
            Site::Form,
            DeclarationErrorKind::CompositeKeyChoice {
                field: "parent_id".to_string(),
            },
        )),
        "{errors:?}"
    );
}

/// Rejects a hostile slug at registration.
#[test]
fn panel_build_rejects_a_hostile_slug() {
    use crate::resource::Resource;

    struct HostileResource;
    impl Resource for HostileResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("a\"b\r\n")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    let errors = refusal(mount_without_db(
        Panel::new("admin").resource::<HostileResource>(),
    ));
    assert_eq!(
        errors[0],
        DeclarationError::of::<HostileResource>(
            Site::Registration,
            DeclarationErrorKind::InvalidSegment {
                item: "ResourceDef::slug",
                segment: "a\"b\r\n".to_string(),
                fault: crate::SegmentFault::Char('"'),
            },
        )
    );
}

/// Rejects a `unique()` marker without a unique index.
#[tokio::test]
async fn panel_build_rejects_a_unique_marker_without_a_unique_index() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
        table::{Table, TextColumn},
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("subscribers")
                .policy(|_cx: &Cx, ability: Ability<'_, Subscriber>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Subscriber.nickname))))
                .form(Schema::new(
                    Field::text(Subscriber::fields().nickname()).unique(),
                ))
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
    assert_eq!(
        refusal(mount(db, panel_for::<UnbackedResource>())),
        [DeclarationError::of::<UnbackedResource>(
            Site::Form,
            DeclarationErrorKind::UniqueWithoutIndex {
                field: "nickname".to_string(),
            },
        )]
    );
}

/// Accepts keyed tables with and without chrome.
#[tokio::test]
async fn panel_build_accepts_keyed_tables_with_and_without_chrome() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
        table::{Table, TextColumn},
    };

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        nickname: String,
    }

    fn keyed_table() -> Table<Subscriber> {
        Table::new(TextColumn::new(lens!(Subscriber.nickname)))
    }

    struct ChromeResource;
    impl Resource for ChromeResource {
        type Model = Subscriber;
        type Form = ChromeForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("subscribers")
                .policy(|_cx: &Cx, ability: Ability<'_, Subscriber>| {
                    matches!(ability, Ability::DeleteAny | Ability::Delete(_))
                })
                .table(keyed_table())
                .form(Schema::new(Field::text(Subscriber::fields().nickname())))
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().slug("subscribers").table(keyed_table())
        }
    }

    struct ViewedResource;
    impl Resource for ViewedResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("subscribers")
                .table(keyed_table())
                .view(Schema::new(Field::text(Subscriber::fields().nickname())))
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

/// Rejects an unbacked `unique()` marker even when create is denied.
#[tokio::test]
async fn panel_build_rejects_an_unbacked_unique_marker_even_when_create_is_denied() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
        table::{Table, TextColumn},
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("subscribers")
                .policy(|_cx: &Cx, ability: Ability<'_, Subscriber>| {
                    matches!(ability, Ability::ViewAny)
                })
                // `Create` keeps its default (deny); only the form is declared.
                .table(Table::new(TextColumn::new(lens!(Subscriber.nickname))))
                .form(Schema::new(
                    Field::text(Subscriber::fields().nickname()).unique(),
                ))
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
    assert_eq!(
        refusal(mount(db, panel_for::<ReadOnlyResource>())),
        [DeclarationError::of::<ReadOnlyResource>(
            Site::Form,
            DeclarationErrorKind::UniqueWithoutIndex {
                field: "nickname".to_string(),
            },
        )]
    );
}

/// Rejects duplicate resource slugs.
#[test]
fn panel_build_rejects_duplicate_resource_slugs() {
    use crate::resource::Resource;

    struct FirstResource;
    impl Resource for FirstResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    struct SecondResource;
    impl Resource for SecondResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    let errors = refusal(mount_without_db(
        Panel::new("admin")
            .resource::<FirstResource>()
            .resource::<SecondResource>(),
    ));
    assert_eq!(
        errors[0],
        DeclarationError::of::<SecondResource>(
            Site::Registration,
            DeclarationErrorKind::DuplicateSlug {
                slug: "dummies".to_string(),
            },
        )
    );
}

/// Rejects route pattern characters in a slug.
#[test]
fn panel_build_rejects_route_pattern_characters_in_a_slug() {
    use crate::resource::Resource;

    macro_rules! pattern_resource {
        ($name:ident, $slug:literal) => {
            struct $name;
            impl Resource for $name {
                type Model = Dummy;
                type Form = crate::NoForm<Self::Model>;

                fn declare() -> ResourceDef<Self> {
                    ResourceDef::new()
                        .slug($slug)
                        .table(crate::table::Table::new(crate::table::TextColumn::new(
                            lens!(Dummy.name),
                        )))
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
            let errors = refusal(mount_without_db(Panel::new("admin").resource::<$name>()));
            assert!(
                matches!(
                    &errors[0].kind,
                    DeclarationErrorKind::InvalidSegment {
                        item: "ResourceDef::slug",
                        segment,
                        ..
                    } if segment == $slug
                ),
                "{errors:?}"
            );
        }};
    }
    rejects!(BraceOpen, "a{b");
    rejects!(BraceClose, "a}b");
    rejects!(ParenOpen, "a(b");
    rejects!(ParenClose, "a)b");

    // The prefix goes through the same rule, once per segment.
    let errors = refusal(mount_without_db(Panel::new("adm{in}")));
    assert_eq!(
        errors[0],
        DeclarationError::panel(DeclarationErrorKind::InvalidSegment {
            item: "panel prefix",
            segment: "adm{in}".to_string(),
            fault: crate::SegmentFault::Char('{'),
        })
    );
}

/// Reports recorded table misdeclarations.
#[tokio::test]
async fn panel_build_reports_recorded_table_misdeclarations() {
    use crate::{
        resource::Resource,
        table::{Table, TextColumn},
    };

    #[derive(Debug, toasty::Model, Clone)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        title: String,
    }

    /// Two columns over one field: a duplicate name.
    struct DuplicateColumnResource;
    impl Resource for DuplicateColumnResource {
        type Model = Doc;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().slug("docs").table(Table::new((
                TextColumn::new(lens!(Doc.title)),
                TextColumn::new(lens!(Doc.title)),
            )))
        }
    }

    /// A page of no rows: `Table::paginate` records zero.
    struct ZeroPageResource;
    impl Resource for ZeroPageResource {
        type Model = Doc;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("docs")
                .table(Table::new(TextColumn::new(lens!(Doc.title))).paginate(0))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Doc))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let panel = || Panel::new("admin").auth(crate::Auth::disabled());

    assert_eq!(
        refusal(mount(
            db.clone(),
            panel().resource::<DuplicateColumnResource>()
        )),
        [DeclarationError::of::<DuplicateColumnResource>(
            Site::Table,
            DeclarationErrorKind::DuplicateColumn {
                name: "title".to_string(),
            },
        )]
    );
    assert_eq!(
        refusal(mount(db.clone(), panel().resource::<ZeroPageResource>())),
        [DeclarationError::of::<ZeroPageResource>(
            Site::Table,
            DeclarationErrorKind::ZeroPageSize,
        )]
    );
}

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
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
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

/// Rejects a misdeclared view.
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(dummy_table()).view(Schema::new((
                Field::text(Dummy::fields().name()),
                Field::text(Dummy::fields().name()),
            )))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    assert_eq!(
        refusal(mount(db, panel_for::<BadView>())),
        [DeclarationError::of::<BadView>(
            Site::View,
            DeclarationErrorKind::DuplicateField {
                name: "name".to_string(),
            },
        )]
    );
}

/// Declares each resource once, when the panel mounts, and serves every request from that build.
#[tokio::test]
async fn a_resource_declares_once_when_its_panel_mounts() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

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
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                    )
                })
                .table(dummy_table())
                .form(Schema::new(Field::text(Dummy::fields().name())))
                .view(Schema::new(Field::text(Dummy::fields().name())))
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
    assert_eq!(DECLARE_CALLS.load(Ordering::SeqCst), 1);

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
    assert_eq!(
        DECLARE_CALLS.load(Ordering::SeqCst),
        1,
        "requests are served from the mounted def"
    );
}

/// A resource the request's panel does not mount has no def there: its policy allows nothing and
/// its scoped query is refused, where a context with no panel answers from its own declaration.
#[test]
fn an_unmounted_resource_is_refused_not_rebuilt() {
    use crate::resource::{MountScope, Mounts, Resource};

    struct UnmountedResource;
    impl Resource for UnmountedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().policy(crate::Allow).table(dummy_table())
        }
    }

    let bare = topcoat::context::CxTestBuilder::new().build();
    assert!(crate::can::<UnmountedResource>(&bare, Ability::ViewAny));
    assert!(crate::scoped_query::<UnmountedResource>(&bare).is_ok());

    let panel = topcoat::context::CxTestBuilder::new()
        .app_context(MountScope(|_| Some(&EMPTY)))
        .build();
    static EMPTY: std::sync::LazyLock<Mounts> = std::sync::LazyLock::new(Mounts::default);
    assert!(!crate::can::<UnmountedResource>(&panel, Ability::ViewAny));
    let refused = crate::scoped_query::<UnmountedResource>(&panel).expect_err("not mounted");
    assert!(refused.to_string().contains("not mounted"), "{refused}");
}
