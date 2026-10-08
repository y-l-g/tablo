use super::*;
use crate::{
    Ability, Panel, ReadOnly, ResourceDef, lens, panel::test_support::mount,
    test_support::memory_db,
};

#[tokio::test]
async fn options_endpoint_searches_and_gates() {
    // `GET {parent}/options?field=&q=` narrows server-side,
    // allow-lists to searchable relationship selects, and mirrors gates.
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct OptAuthor {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct OptAuthorResource;
    impl Resource for OptAuthorResource {
        type Model = OptAuthor;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("opt-authors")
                .policy(ReadOnly)
                .table(crate::table::Table::new(
                    crate::table::TextColumn::new(lens!(OptAuthor.name)).searchable(),
                ))
        }
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct OptPost {
        #[key]
        #[auto]
        id: uuid::Uuid,
        author_id: uuid::Uuid,
        title: String,
    }
    struct OptPostResource;
    impl Resource for OptPostResource {
        type Model = OptPost;
        type Form = OptPostForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("opt-posts")
                .policy(ReadOnly)
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(OptPost.title),
                )))
                .form(crate::schema::Schema::new(
                    OptPostForm::controls()
                        .author_id
                        .relationship::<OptAuthorResource>(|a: &OptAuthor| a.name.clone())
                        .searchable(),
                ))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = OptPost)]
    struct OptPostForm {
        #[form(choice)]
        author_id: uuid::Uuid,
    }
    async fn body_text(resp: http::Response<Body>) -> String {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&bytes).to_string()
    }

    let mut db = memory_db(toasty::models!(OptAuthor, OptPost)).await;
    for name in ["Ada", "Grace", "Alan"] {
        toasty::create!(OptAuthor {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(
        db,
        Panel::new("admin")
            .resource::<OptPostResource>()
            .resource::<OptAuthorResource>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    // Narrowing works.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/opt-posts/options?field=author_id&q=Ada")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let html = body_text(resp).await;
    assert!(html.contains("Ada"), "search must return Ada, got {html}");
    assert!(!html.contains("Grace"), "search must narrow, got {html}");

    // Unknown field → 400.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/opt-posts/options?field=nope&q=Ada")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);

    // Missing field → 400.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/opt-posts/options?q=Ada")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
    // Escaping itself is pinned where it can fail:
    // `an_option_escapes_its_value_and_label` feeds characters that must be
    // escaped and asserts the exact output. These fixtures are "Ada"/"Grace"/"Alan", so a
    // `!html.contains("<script")` here could never fail.
}

#[tokio::test]
async fn option_load_loads_no_relation() {
    // An option load projects a value and a label off the related record's
    // own columns, so it loads no relation. The source's detail page and its
    // list both declare a column including `parent`, and `View` keeps a row
    // only while that relation is unloaded, so a rendered option proves the
    // loader ran the bare `scoped_query`, not either of those.
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct Parent {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Child {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
        #[index]
        parent_id: uuid::Uuid,
        #[belongs_to(key = parent_id, references = id)]
        parent: toasty::Deferred<Parent>,
    }

    fn name_with_parent() -> crate::table::ComputedColumn<Child> {
        crate::table::ComputedColumn::new("Name", |c: &Child| c.name.clone())
            .include(Child::fields().parent())
    }

    struct ChildSource;
    impl Resource for ChildSource {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("children")
                .policy(|_cx: &Cx, ability: Ability<'_, Child>| match ability {
                    Ability::ViewAny => true,
                    Ability::View(record) => record.parent.is_unloaded(),
                    _ => false,
                })
                .table(crate::table::Table::new(name_with_parent()))
                .view(crate::Detail::new(name_with_parent()))
        }
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Owner {
        #[key]
        #[auto]
        id: uuid::Uuid,
        child_id: uuid::Uuid,
        name: String,
    }

    struct OwnerResource;
    impl Resource for OwnerResource {
        type Model = Owner;
        type Form = OwnerForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("owners")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Owner.name),
                )))
                .form(crate::schema::Schema::new(
                    OwnerForm::controls()
                        .child_id
                        .relationship::<ChildSource>(|c: &Child| c.name.clone())
                        .searchable(),
                ))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Owner)]
    struct OwnerForm {
        #[form(choice)]
        child_id: uuid::Uuid,
    }
    let mut db = memory_db(toasty::models!(Parent, Child, Owner)).await;
    let parent_id = uuid::Uuid::new_v4();
    toasty::create!(Parent {
        id: parent_id,
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Child {
        name: "Only Child".to_string(),
        parent_id,
    })
    .exec(&mut db)
    .await
    .unwrap();

    let router = mount(
        db,
        Panel::new("admin")
            .resource::<OwnerResource>()
            .resource::<ChildSource>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/owners/options?field=child_id")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let html = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        html.contains("Only Child"),
        "the option must render, which it cannot if the loader loaded the source's include: {html}"
    );
}

#[tokio::test]
async fn options_endpoint_rejects_non_searchable_and_overflows() {
    // Non-searchable selects never serve search (400);
    // filtered overflow answers 200 with the keep-typing hint.
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct BigA {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct BigAResource;
    impl Resource for BigAResource {
        type Model = BigA;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("big-as")
                .policy(ReadOnly)
                .table(crate::table::Table::new(
                    crate::table::TextColumn::new(lens!(BigA.name)).searchable(),
                ))
        }
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct BigP {
        #[key]
        #[auto]
        id: uuid::Uuid,
        author_id: uuid::Uuid,
        /// A text column for the table declaration: every
        /// servable resource needs one, and `author_id` is a Uuid.
        name: String,
    }
    struct SearchableParent;
    impl Resource for SearchableParent {
        type Model = BigP;
        type Form = SearchableParentForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("big-ps")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(BigP.name),
                )))
                .form(crate::schema::Schema::new(
                    SearchableParentForm::controls()
                        .author_id
                        .relationship::<BigAResource>(|a: &BigA| a.name.clone())
                        .searchable(),
                ))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = BigP)]
    struct SearchableParentForm {
        #[form(choice)]
        author_id: uuid::Uuid,
    }
    struct PlainParent;
    impl Resource for PlainParent {
        type Model = BigP;
        type Form = PlainParentForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("plain-ps")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(BigP.name),
                )))
                .form(crate::schema::Schema::new(
                    PlainParentForm::controls()
                        .author_id
                        .relationship::<BigAResource>(|a: &BigA| a.name.clone()),
                ))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = BigP)]
    struct PlainParentForm {
        #[form(choice)]
        author_id: uuid::Uuid,
    }
    let mut db = memory_db(toasty::models!(BigA, BigP)).await;
    for i in 0..=crate::schema::MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(BigA {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(
        db,
        Panel::new("admin")
            .resource::<SearchableParent>()
            .resource::<PlainParent>()
            .resource::<BigAResource>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    // Non-searchable → 400 (keeps today's cap error path, never search).
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/plain-ps/options?field=author_id&q=author-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);

    // Empty q on over-cap searchable → 200 with keep-typing hint.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/big-ps/options?field=author_id&q=")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        html.contains("keep typing"),
        "filtered overflow must hint, got {html}"
    );
}

/// The endpoint renders options through the field's own `option_view`, so a
/// label or value carrying markup reaches the page escaped.
#[tokio::test]
async fn an_option_escapes_its_value_and_label() {
    use topcoat::context::CxTestBuilder;

    let cx = CxTestBuilder::new().build();
    let html = crate::schema::option_view(
        &cx,
        "a\"b".to_string(),
        "<script>x</script>".to_string(),
        false,
    )
    .single()
    .await
    .unwrap()
    .render(&cx);
    assert!(
        !html.contains("<script>") && html.contains("&lt;script&gt;"),
        "the label must be escaped, got {html}"
    );
    assert!(
        !html.contains("value=\"a\"b\""),
        "the value must be escaped, got {html}"
    );
}
