use toasty::Db;
use topcoat::router::Body;

use super::*;
use crate::{
    ResourceDef, lens,
    panel::test_support::{Subscriber, Tagged, TaggedResource, mount, panel_for, response_html},
};

fn messages<'a>(errors: &'a FieldErrors, key: &str) -> Vec<&'a str> {
    errors
        .iter()
        .filter(|error| error.key == key)
        .map(|error| error.message.as_str())
        .collect()
}

#[tokio::test]
async fn unique_check_flags_duplicates_for_marked_fields() {
    use topcoat::context::CxTestBuilder;

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

    let mut db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(Subscriber { email: "a@b.c" })
        .exec(&mut db)
        .await
        .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(Field::text(Subscriber::fields().email()).unique());
    let mut values = HashMap::new();
    values.insert("email".to_string(), "a@b.c".to_string());

    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &values,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert_eq!(
        messages(&errors, "email"),
        ["Email has already been taken"],
        "duplicate must be flagged, got {errors:?}"
    );

    let mut fresh = HashMap::new();
    fresh.insert("email".to_string(), "other@b.c".to_string());
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &fresh,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(errors.is_empty(), "fresh value must pass, got {errors:?}");

    let mut current = HashMap::new();
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

    let mut changed_current = HashMap::new();
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

    let mut empty = HashMap::new();
    empty.insert("email".to_string(), "   ".to_string());
    let optional_schema = Schema::new(
        Field::text(Subscriber::fields().email())
            .optional()
            .unique(),
    );
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &optional_schema,
        &empty,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "an empty unique submit must not be probed, got {errors:?}"
    );
}

/// An explicitly `unique()` field stays required even when marked `.optional()`.
#[tokio::test]
async fn unique_field_is_required_however_it_is_marked() {
    use topcoat::context::CxTestBuilder;

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

    let db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let cx = CxTestBuilder::new().app_context(db.clone()).build();
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(
        Field::text(Subscriber::fields().email())
            .unique()
            .optional(),
    );
    let mut first = HashMap::new();
    first.insert("email".to_string(), "   ".to_string());
    assert_eq!(
        messages(&schema.validate(&first), "email"),
        ["Email is required"],
        "an empty unique field must fail validation as required"
    );

    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &first,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "an empty unique submit must not be probed, got {errors:?}"
    );

    let mut db_check = db;
    let stored = Subscriber::all().exec(&mut db_check).await.unwrap();
    assert!(
        stored.is_empty(),
        "an empty unique submit must not write, got {} rows",
        stored.len()
    );
}

/// Lens-derived `#[unique]` fields are required without a `.unique()` call.
#[tokio::test]
async fn lens_derived_unique_is_required_without_a_unique_call() {
    use topcoat::context::CxTestBuilder;

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

    let db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let mut ex = crate::db::db(&cx);

    let input = Field::text(Subscriber::fields().email());
    assert!(
        input.is_unique(),
        "the index must be recognized without a `.unique()` call"
    );
    assert!(input.is_required(), "derived uniqueness implies presence");

    let schema = Schema::new(input);
    let mut empty = HashMap::new();
    empty.insert("email".to_string(), "".to_string());
    assert_eq!(
        messages(&schema.validate(&empty), "email"),
        ["Email is required"],
        "an empty submit must be refused inline, not probed"
    );
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &schema,
        &empty,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "validation owns the empty case; the probe must add nothing, got {errors:?}"
    );
}

#[tokio::test]
async fn unique_check_propagates_probe_errors() {
    use topcoat::context::CxTestBuilder;

    use crate::schema::{Field, Schema};

    #[derive(Debug, toasty::Model, Clone)]
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
    let db = Db::builder()
        .models(toasty::models!(Probe))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(Field::text(Probe::fields().email()).unique());
    let mut values = HashMap::new();
    values.insert("email".to_string(), "a@b.c".to_string());
    let result = check_unique(
        &cx,
        &crate::resource::require_mounted::<ProbeResource>(&cx).unwrap(),
        &schema,
        &values,
        &HashMap::new(),
        &mut ex,
    )
    .await;
    assert!(
        result.is_err(),
        "a failing probe must fail the submit, got {result:?}"
    );
}

#[tokio::test]
async fn unique_check_ignores_absent_repeater_groups() {
    use topcoat::context::CxTestBuilder;

    use crate::schema::{Field, Repeater, Schema};

    #[derive(Debug, toasty::Model, Clone)]
    struct Nicknamed {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        nickname: String,
    }
    struct TaggedResource;
    impl Resource for TaggedResource {
        type Model = Nicknamed;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Nicknamed.nickname),
            )))
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Nicknamed))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(Nicknamed {
        nickname: "".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let mut ex = crate::db::db(&cx);

    let schema = Schema::new(
        Repeater::new("Tags").schema(
            Field::text(Nicknamed::fields().nickname())
                .unique()
                .optional(),
        ),
    );

    // An absent group validates clean, so the unique check agrees.
    let mut absent = HashMap::new();
    absent.insert("nickname".to_string(), "".to_string());
    assert!(
        schema.validate(&absent).is_empty(),
        "absent group must validate clean"
    );
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<TaggedResource>(&cx).unwrap(),
        &schema,
        &absent,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert!(
        errors.is_empty(),
        "absent group must not be unique-checked, got {errors:?}"
    );

    let mut present = HashMap::new();
    present.insert("nickname".to_string(), "taken".to_string());
    toasty::create!(Nicknamed {
        nickname: "taken".to_string()
    })
    .exec(&mut ex)
    .await
    .unwrap();
    let errors = check_unique(
        &cx,
        &crate::resource::require_mounted::<TaggedResource>(&cx).unwrap(),
        &schema,
        &present,
        &HashMap::new(),
        &mut ex,
    )
    .await
    .unwrap();
    assert_eq!(
        errors.first("nickname").map(|error| error.message.as_str()),
        Some("Nickname has already been taken"),
        "present group must still be unique-checked, got {errors:?}"
    );
}

/// A typed unique field probes the declared type, not its text spelling.
#[tokio::test]
async fn a_typed_unique_field_probes_the_declared_type() {
    const TOKEN: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

    let db = Db::builder()
        .models(toasty::models!(Tagged))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
            .uri("/admin/tagged/create")
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

    let db = Db::builder()
        .models(toasty::models!(Tagged))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
