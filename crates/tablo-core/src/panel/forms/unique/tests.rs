use topcoat::router::Body;

use super::*;
use crate::{
    ResourceDef,
    form::FormValues,
    lens,
    panel::test_support::{Subscriber, Tagged, TaggedResource, mount, panel_for, response_html},
    test_support::{memory_db, tableless_db},
};

fn messages(errors: &FieldErrors, key: &str) -> Vec<String> {
    errors
        .iter()
        .filter(|error| error.key == key)
        .map(|error| error.message(key))
        .collect()
}

#[tokio::test]
async fn unique_check_flags_duplicates_for_marked_fields() {
    use crate::schema::{Field, Schema};

    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Subscriber.email),
            )))
        }
    }

    let mut db = memory_db(toasty::models!(Subscriber)).await;
    toasty::create!(Subscriber { email: "a@b.c" })
        .exec(&mut db)
        .await
        .unwrap();
    let cx = crate::test_support::panel_cx::<SubscriberResource>(&db);
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(Field::text(Subscriber::fields().email()).unique());
    let mut values = FormValues::new();
    values.insert("email".to_string(), "a@b.c".to_string());

    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &values,
        &FormValues::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert_eq!(
        messages(&errors, "email"),
        ["Email has already been taken"],
        "duplicate must be flagged, got {errors:?}"
    );

    let mut fresh = FormValues::new();
    fresh.insert("email".to_string(), "other@b.c".to_string());
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &fresh,
        &FormValues::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(errors.is_empty(), "fresh value must pass, got {errors:?}");

    let mut current = FormValues::new();
    current.insert("email".to_string(), "a@b.c".to_string());
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &values,
        &current,
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "own unchanged value must be skipped, got {errors:?}"
    );

    let mut changed_current = FormValues::new();
    changed_current.insert("email".to_string(), "old@b.c".to_string());
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &values,
        &changed_current,
        &mut ex,
    )
    .await
    .unwrap();
    assert_eq!(
        messages(&errors, "email"),
        ["Email has already been taken"],
        "changed-to-duplicate must be flagged, got {errors:?}"
    );

    let mut empty = FormValues::new();
    empty.insert("email".to_string(), "   ".to_string());
    let optional_schema = Schema::new(Field::text(Subscriber::fields().email()).unique());
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &optional_schema,
        &empty,
        &FormValues::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "an empty unique submit must not be probed, got {errors:?}"
    );
}

#[tokio::test]
async fn unique_check_propagates_probe_errors() {
    use crate::schema::{Field, Schema};

    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct Probe {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        email: String,
    }
    struct ProbeResource;
    impl Resource for ProbeResource {
        type Model = Probe;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Probe.email),
            )))
        }
    }

    // Schema never pushed, so the probe cannot run.
    let db = tableless_db(toasty::models!(Probe)).await;
    let cx = crate::test_support::panel_cx::<ProbeResource>(&db);
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(Field::text(Probe::fields().email()).unique());
    let mut values = FormValues::new();
    values.insert("email".to_string(), "a@b.c".to_string());
    let result = check_unique(
        &cx,
        &crate::resource::require_mounted::<ProbeResource>(&cx).unwrap(),
        &schema,
        &values,
        &FormValues::new(),
        &mut ex,
    )
    .await;
    assert!(
        result.is_err(),
        "a failing probe must fail the submit, got {result:?}"
    );
}

/// A typed unique field probes the declared type, not its text spelling.
#[tokio::test]
async fn a_typed_unique_field_probes_the_declared_type() {
    const TOKEN: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

    let db = memory_db(toasty::models!(Tagged)).await;
    let mut db_q = db.clone();
    toasty::create!(Tagged {
        name: "one".to_string(),
        token: uuid::Uuid::parse_str(TOKEN).unwrap(),
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    let router = mount(db.clone(), panel_for::<TaggedResource>()).expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    let request = |body: String| {
        http::Request::builder()
            .method(http::Method::POST)
            .uri("/admin/tagged/-/create")
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(
                http::header::COOKIE,
                format!("{}={csrf}", crate::csrf::COOKIE_NAME),
            )
            .body(Body::from(body))
            .unwrap()
    };

    let response = router
        .handle(request(format!(
            "name=two&token={}&csrf_token={csrf}",
            TOKEN.to_uppercase()
        )))
        .await;
    assert_eq!(
        response.status(),
        200,
        "the duplicate must re-render, not create"
    );
    let html = response_html(response).await;
    assert!(
        html.contains("Token has already been taken"),
        "the typed probe must see the duplicate, got {html}"
    );
    let mut db_q = db.clone();
    assert_eq!(
        Tagged::all().exec(&mut db_q).await.unwrap().len(),
        1,
        "a refused create writes nothing"
    );

    let response = router
        .handle(request(format!(
            "name=two&token=3f8fad5b-d9cb-469f-a165-70867728950e&csrf_token={csrf}"
        )))
        .await;
    assert_eq!(
        response.status(),
        303,
        "a distinct value must create, not flag a duplicate"
    );
    let mut db_q = db.clone();
    assert_eq!(
        Tagged::all().exec(&mut db_q).await.unwrap().len(),
        2,
        "the accepted create writes its row"
    );
}

/// An edit re-spelling the record's own typed value still saves.
#[tokio::test]
async fn a_typed_unique_field_skips_the_records_own_value_on_edit() {
    const MINE: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
    const THEIRS: &str = "3f8fad5b-d9cb-469f-a165-70867728950e";

    let db = memory_db(toasty::models!(Tagged)).await;
    let mut db_q = db.clone();
    let mine = toasty::create!(Tagged {
        name: "mine".to_string(),
        token: uuid::Uuid::parse_str(MINE).unwrap(),
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    toasty::create!(Tagged {
        name: "theirs".to_string(),
        token: uuid::Uuid::parse_str(THEIRS).unwrap(),
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    let router = mount(db.clone(), panel_for::<TaggedResource>()).expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    let url = format!("/admin/tagged/{}/edit", mine.id);
    let edit = |token: &str| {
        http::Request::builder()
            .method(http::Method::POST)
            .uri(&url)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(
                http::header::COOKIE,
                format!("{}={csrf}", crate::csrf::COOKIE_NAME),
            )
            .body(Body::from(format!(
                "name=mine&token={token}&csrf_token={csrf}"
            )))
            .unwrap()
    };

    let response = router.handle(edit(&MINE.to_uppercase())).await;
    assert!(
        response.status().is_redirection(),
        "re-spelling the record's own value must save, got {} {}",
        response.status(),
        response_html(response).await
    );
    let mut db_q = db.clone();
    let saved = Tagged::filter(Tagged::fields().id().eq(mine.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the edited record");
    assert_eq!(
        saved.token,
        uuid::Uuid::parse_str(MINE).unwrap(),
        "the re-spelled value is stored canonically"
    );

    let response = router.handle(edit(THEIRS)).await;
    assert_eq!(
        response.status(),
        200,
        "another record's value must refuse the edit"
    );
    let html = response_html(response).await;
    assert!(
        html.contains("Token has already been taken"),
        "the typed probe must see the other record, got {html}"
    );
    let mut db_q = db.clone();
    let unchanged = Tagged::filter(Tagged::fields().id().eq(mine.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the refused record");
    assert_eq!(
        unchanged.token,
        uuid::Uuid::parse_str(MINE).unwrap(),
        "a refused edit writes nothing"
    );
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
#[unique(slug, locale)]
struct Page {
    #[key]
    #[auto]
    id: uuid::Uuid,
    slug: String,
    locale: String,
}

/// A composite index refuses only a value every one of its fields repeats: a slug another
/// locale holds is free.
#[tokio::test]
async fn a_composite_index_is_probed_on_all_its_fields() {
    use crate::schema::{Field, Schema};

    struct PageResource;
    impl Resource for PageResource {
        type Model = Page;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Page.slug),
            )))
        }
    }

    let mut db = memory_db(toasty::models!(Page)).await;
    toasty::create!(Page {
        slug: "about",
        locale: "en"
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = crate::test_support::panel_cx::<PageResource>(&db);
    let mut ex = crate::db::db(&cx);
    let schema = Schema::new((
        Field::text(Page::fields().slug()),
        Field::text(Page::fields().locale()),
    ));
    let page = |slug: &str, locale: &str| {
        FormValues::from_iter([
            ("slug".to_string(), slug.to_string()),
            ("locale".to_string(), locale.to_string()),
        ])
    };
    let resource = crate::resource::require_mounted::<PageResource>(&cx).unwrap();
    let mut check = async |values: &FormValues, current: &FormValues| {
        check_unique(&cx, &resource, &schema, values, current, &mut ex)
            .await
            .unwrap()
    };

    assert!(
        check(&page("about", "fr"), &FormValues::new())
            .await
            .is_empty(),
        "another locale"
    );
    let errors = check(&page("about", "en"), &FormValues::new()).await;
    assert_eq!(messages(&errors, "slug"), ["Slug has already been taken"]);
    assert!(
        messages(&errors, "locale").is_empty(),
        "one refusal per index"
    );
    assert!(
        check(&page("about", "en"), &page("about", "en"))
            .await
            .is_empty(),
        "the record's own values"
    );
    assert!(
        !check(&page("about", "en"), &page("about", "fr"))
            .await
            .is_empty(),
        "a record moving onto another's pair"
    );
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
#[unique(owner, slug)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    owner: String,
    slug: String,
}

/// An index member the form does not post is the one the resource's query fixes, as the guide
/// advises scoping an index: the probe matches the posted fields through that query.
#[tokio::test]
async fn an_index_member_the_form_does_not_post_is_left_to_the_scope() {
    use crate::schema::{Field, Schema};

    struct MyPosts;
    impl Resource for MyPosts {
        type Model = Post;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Post.slug),
            )))
        }

        fn query(_cx: &topcoat::context::Cx) -> toasty::stmt::Query<toasty::stmt::List<Post>> {
            toasty::stmt::Query::<toasty::stmt::List<Post>>::all()
                .filter(Post::fields().owner().eq("me"))
        }
    }

    let mut db = memory_db(toasty::models!(Post)).await;
    for (owner, slug) in [("me", "about"), ("them", "contact")] {
        toasty::create!(Post { owner, slug })
            .exec(&mut db)
            .await
            .unwrap();
    }
    let cx = crate::test_support::panel_cx::<MyPosts>(&db);
    let mut ex = crate::db::db(&cx);
    let schema = Schema::new(Field::text(Post::fields().slug()));
    let resource = crate::resource::require_mounted::<MyPosts>(&cx).unwrap();
    let mut check = async |slug: &str| {
        let values = FormValues::from_iter([("slug".to_string(), slug.to_string())]);
        let errors = check_unique(
            &cx,
            &resource,
            &schema,
            &values,
            &FormValues::new(),
            &mut ex,
        )
        .await
        .unwrap();
        messages(&errors, "slug")
    };

    assert_eq!(check("about").await, ["Slug has already been taken"]);
    assert!(check("contact").await.is_empty(), "another owner's slug");
}
