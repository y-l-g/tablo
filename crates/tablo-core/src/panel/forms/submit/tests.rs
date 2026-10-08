use topcoat::view::ViewExt;

use super::*;
use crate::{
    Ability, Panel, ResourceDef, lens,
    panel::test_support::{Dummy, Subscriber, dummy_table, mount, panel_for, response_html},
    schema::{Field, Schema},
    test_support::{memory_db, tableless_db},
};

/// Asserts `rendered` carries the opaque write-failure copy and none of the driver's text, of
/// which `marker` is the part every driver message for this failure holds.
fn assert_opaque(rendered: &str, driver: &str, marker: &str) {
    assert!(
        driver.contains(marker),
        "the control must be a driver failure, got {driver:?}"
    );
    assert!(
        rendered.contains("database unavailable"),
        "the opaque message must survive, got {rendered:?}"
    );
    assert!(
        !rendered.contains(driver) && !rendered.contains(marker),
        "driver text must not reach the response: the driver said {driver:?}, the response said \
         {rendered:?}"
    );
}

#[test]
fn completion_fills_unnamed_keys_from_the_stored_projection() {
    let schema = Schema::new(Field::text(Dummy::fields().name()));
    let named = HashSet::new();
    let mut values = HashMap::from([("name".to_string(), "advisory".to_string())]);
    let stored = HashMap::from([("name".to_string(), "authoritative".to_string())]);
    complete(&schema, &mut values, &named, &stored);
    assert_eq!(
        values["name"], "authoritative",
        "an unnamed key reads the stored record"
    );

    let named = HashSet::from(["name".to_string()]);
    let mut values = HashMap::from([("name".to_string(), "posted".to_string())]);
    complete(&schema, &mut values, &named, &stored);
    assert_eq!(
        values["name"], "posted",
        "a named key keeps what was posted"
    );

    let mut values = HashMap::from([("name".to_string(), "stale".to_string())]);
    complete(&schema, &mut values, &HashSet::new(), &HashMap::new());
    assert!(!values.contains_key("name"), "nothing stored, nothing kept");
}

#[tokio::test]
async fn edit_post_requires_view_as_well_as_update() {
    use crate::resource::Resource;

    struct ViewDeniedResource;
    impl Resource for ViewDeniedResource {
        type Model = Dummy;
        type Form = ViewDeniedForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                    Ability::View(_record) => false,
                    Ability::Update(_record) => true,
                    _ => false,
                })
                .table(dummy_table())
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct ViewDeniedForm {
        name: String,
    }
    let mut db = memory_db(toasty::models!(Dummy)).await;
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<ViewDeniedResource>()).expect("panel builds");
    let url = format!("/admin/dummies/{}/edit", row.id);
    let get = router
        .handle(
            http::Request::builder()
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(get.status(), http::StatusCode::FORBIDDEN);
    // A valid token still 403s on policy, not on CSRF.
    let token = uuid::Uuid::new_v4().to_string();
    let post = router
        .handle(
            http::Request::builder()
                .uri(&url)
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("name=Ada&csrf_token={token}")))
                .unwrap(),
        )
        .await;
    assert_eq!(
        post.status(),
        http::StatusCode::FORBIDDEN,
        "view-denied edit POST must not mutate"
    );
    // Missing token is 403 even before policy.
    let no_token = router
        .handle(
            http::Request::builder()
                .uri(&url)
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("name=Ada"))
                .unwrap(),
        )
        .await;
    assert_eq!(no_token.status(), http::StatusCode::FORBIDDEN);
}

/// Transport keys never reach the write.
#[tokio::test]
async fn transport_keys_never_reach_the_write() {
    #[derive(Debug, toasty::Model, Clone)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
        title: String,
    }

    struct CapturingResource;
    impl crate::resource::Resource for CapturingResource {
        type Model = Doc;
        type Form = CapturingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("docs")
                .policy(|_cx: &Cx, ability: Ability<'_, Doc>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Doc.title),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Doc)]
    struct CapturingForm {
        title: String,
        #[form(file)]
        path: String,
    }
    struct NameUploader;
    impl crate::Uploader for NameUploader {
        async fn store(
            &self,
            filename: &str,
            _bytes: &[u8],
        ) -> std::result::Result<String, String> {
            Ok(filename.to_string())
        }
    }
    let db = memory_db(toasty::models!(Doc)).await;
    let router = mount(
        db.clone(),
        panel_for::<CapturingResource>().uploads(NameUploader),
    )
    .expect("panel builds");
    let csrf = uuid::Uuid::new_v4().to_string();
    let boundary = "----TransportBoundary";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nx\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"path\"; filename=\"a.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nBYTES\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"clear_path\"\r\n\r\n1\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"keep_path\"\r\n\r\njavascript:alert(1)\r\n\
             --{b}\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{csrf}\r\n\
             --{b}--\r\n",
        b = boundary
    );
    let resp = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/docs/create")
                .header(
                    http::header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "create succeeds, got {} {}",
        resp.status(),
        String::from_utf8_lossy(
            &http_body_util::BodyExt::collect(resp.into_body())
                .await
                .unwrap()
                .to_bytes()
        )
    );
    let mut db_q = db.clone();
    let docs = Doc::all().exec(&mut db_q).await.unwrap();
    assert_eq!(docs.len(), 1, "one row written");
    assert_eq!(docs[0].title, "x");
    assert_ne!(
        docs[0].path, "javascript:alert(1)",
        "a client-typed carry candidate must never be stored"
    );
}

/// A failing driver write surfaces the opaque mapping, never driver text.
#[tokio::test]
async fn a_driver_create_failure_does_not_echo_driver_text() {
    use topcoat::cookie::CookieJarCell;

    use crate::resource::Resource;

    struct WritingResource;
    impl Resource for WritingResource {
        type Model = Dummy;
        type Form = WritingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct WritingForm {
        name: String,
    }
    let db = tableless_db(toasty::models!(Dummy)).await;

    // Positive control carries driver text, so the assertions cannot pass vacuously.
    let mut raw = db.clone();
    let driver = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut raw)
    .await
    .expect_err("the table is missing")
    .to_string();
    drop(raw);

    let token = uuid::Uuid::new_v4().to_string();
    let parts = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/dummies/create")
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(
            http::header::COOKIE,
            format!("{}={token}", crate::csrf::COOKIE_NAME),
        )
        .body(())
        .unwrap()
        .into_parts()
        .0;
    let cx = crate::test_support::panel_cx::<WritingResource>(&db)
        .with_many((parts, CookieJarCell::new()));

    let error = resource_create_post::<WritingResource>(
        &cx,
        Body::from(format!("name=Ada&csrf_token={token}")),
    )
    .first()
    .await
    .expect_err("the write must fail");

    let rendered = error.to_string();
    assert_opaque(&rendered, &driver, "no such table");
}

/// A failing driver write on update surfaces the opaque mapping, never driver text.
#[tokio::test]
async fn a_driver_update_failure_does_not_echo_driver_text() {
    use topcoat::router::{
        RouteFn, RouteFuture, Router, RouterBuilderDiscoverExt, response::IntoResponse,
    };

    use crate::{RouterBuilderPanelExt, resource::Resource};

    #[derive(Debug, toasty::Model, Clone)]
    struct Ghost {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        name: String,
    }

    struct EditingResource;
    impl Resource for EditingResource {
        type Model = Dummy;
        type Form = EditingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Update(_)
                    )
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }

        async fn update_record(
            _cx: &Cx,
            record: Dummy,
            _posted: crate::form::Posted<EditingForm>,
            ex: &mut dyn toasty::Executor,
        ) -> Result<Dummy> {
            toasty::create!(Ghost {
                name: "taken".to_string(),
            })
            .exec(&mut *ex)
            .await?;
            Ok(record)
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct EditingForm {
        name: String,
    }
    /// Runs the edit handler and returns its error as the response body.
    fn edit_error(cx: &Cx, body: Body) -> RouteFuture<'_> {
        Box::pin(async move {
            let error = resource_edit_post::<EditingResource>(cx, body)
                .first()
                .await
                .expect_err("the write must fail");
            error.to_string().into_response(cx)
        })
    }

    let mut db = memory_db(toasty::models!(Dummy, Ghost)).await;
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Ghost {
        name: "taken".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();

    // Positive control carries driver text, so the assertions cannot pass vacuously.
    let mut raw = db.clone();
    let driver = toasty::create!(Ghost {
        name: "taken".to_string(),
    })
    .exec(&mut raw)
    .await
    .expect_err("the name is taken")
    .to_string();
    drop(raw);

    // The route sits outside the panel's prefix, where the router's one panel still answers.
    let router = Router::builder()
        .discover()
        .app_context(db)
        .panel(
            Panel::new("admin")
                .auth(crate::Auth::disabled())
                .resource::<EditingResource>(),
        )
        .expect("panel builds")
        .route(RouteFn::new(
            http::Method::POST,
            "/capture/{id}",
            edit_error,
        ))
        .build();
    let token = uuid::Uuid::new_v4().to_string();
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(format!("/capture/{}", row.id))
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("name=Ada&csrf_token={token}")))
                .unwrap(),
        )
        .await;
    let rendered = String::from_utf8_lossy(
        &http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .to_string();

    assert_opaque(&rendered, &driver, "UNIQUE constraint failed");
}

/// A mutation answers 303 with the flash cookie, never the query (#126, topcoat#408).
#[tokio::test]
async fn mutation_redirect_carries_the_flash_cookie_instead_of_a_query() {
    use crate::resource::Resource;

    const COOKIE_NAME: &str = crate::notification::COOKIE_NAME;

    struct NotifyingResource;
    impl Resource for NotifyingResource {
        type Model = Dummy;
        type Form = NotifyingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }

        async fn create_record(
            _cx: &Cx,
            _form: NotifyingForm,
            ex: &mut dyn toasty::Executor,
        ) -> Result<Dummy> {
            toasty::create!(Dummy {
                name: "created".to_string(),
            })
            .exec(&mut *ex)
            .await
            .map_err(|error| -> topcoat::Error { error.into() })
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct NotifyingForm {
        // Optional so the csrf-only POST parses.
        #[form(optional)]
        name: String,
    }
    let db = memory_db(toasty::models!(Dummy)).await;
    let router = mount(db, panel_for::<NotifyingResource>()).expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/create")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("csrf_token={token}")))
                .unwrap(),
        )
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::SEE_OTHER,
        "a completed mutation is a 303 Post/Redirect/Get"
    );
    let location = resp
        .headers()
        .get(http::header::LOCATION)
        .expect("the redirect names its target")
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        !location.contains("notification"),
        "the toast must not ride the query, got {location}"
    );
    let set_cookie = resp
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{COOKIE_NAME}=")))
        .expect("the flash cookie flushes on the Err redirect")
        .to_string();
    assert!(
        set_cookie.contains("success") && set_cookie.contains("Created"),
        "the cookie carries the toast status and title: {set_cookie}"
    );
    assert!(
        set_cookie.contains("Secure") && set_cookie.contains("HttpOnly"),
        "the flushed cookie keeps the __Host- contract: {set_cookie}"
    );
}

/// Through the real panel, two empty `unique()` submits re-render inline and write nothing.
#[tokio::test]
async fn two_empty_submits_on_a_unique_field_re_render_and_write_nothing() {
    use crate::{
        resource::{Resource, ResourceDef},
        schema::Schema,
        table::{Table, TextColumn},
    };

    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = SubscriberForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("subscribers")
                .policy(|_cx: &Cx, ability: Ability<'_, Subscriber>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Subscriber.email))))
                .form(Schema::new(SubscriberForm::controls().email.unique()))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Subscriber)]
    struct SubscriberForm {
        email: String,
    }
    let db = memory_db(toasty::models!(Subscriber)).await;
    let router = mount(db.clone(), panel_for::<SubscriberResource>()).expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    for (attempt, submitted) in ["+", ""].into_iter().enumerate() {
        let attempt = attempt + 1;
        let resp = router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/admin/subscribers/create")
                    .header(
                        http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .header(
                        http::header::COOKIE,
                        format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                    )
                    .body(Body::from(format!("email={submitted}&csrf_token={csrf}")))
                    .unwrap(),
            )
            .await;
        assert_eq!(
            resp.status(),
            http::StatusCode::OK,
            "empty submit {attempt} must re-render, not redirect or fail"
        );
        let body = http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes();
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains("Email is required"),
            "empty submit {attempt} must carry the presence error, got {html}"
        );
    }

    let mut db_check = db;
    let stored = Subscriber::all().exec(&mut db_check).await.unwrap();
    assert!(
        stored.is_empty(),
        "two empty submits must write nothing, got {} rows",
        stored.len()
    );
}

/// A forged `keep_<field>` without `holds` leaves the field empty.
#[tokio::test]
async fn a_forged_carry_is_refused_by_the_default_holds() {
    #[derive(Debug, toasty::Model, Clone)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        title: String,
        path: String,
    }

    struct NoHoldsUploader;

    impl crate::Uploader for NoHoldsUploader {
        async fn store(
            &self,
            _filename: &str,
            _bytes: &[u8],
        ) -> std::result::Result<String, String> {
            Ok("/uploads/stored.bin".to_string())
        }
    }

    struct DocResource;

    impl crate::resource::Resource for DocResource {
        type Model = Doc;
        type Form = DocForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("docs")
                .policy(|_cx: &Cx, ability: Ability<'_, Doc>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Doc.title),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Doc)]
    struct DocForm {
        title: String,
        #[form(file)]
        path: String,
    }
    let db = memory_db(toasty::models!(Doc)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .uploads(NoHoldsUploader)
            .resource::<DocResource>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/docs/create")
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={csrf}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!(
                    "title=Doc&keep_path=javascript:alert(1)&csrf_token={csrf}"
                )))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), 200, "the forged carry must re-render");
    let html = response_html(response).await;
    assert!(
        html.contains("Path is required"),
        "the forged carry must leave the field empty, got {html}"
    );
    let mut db_q = db.clone();
    assert!(
        Doc::all().exec(&mut db_q).await.unwrap().is_empty(),
        "a forged carry must not create a record"
    );
}
