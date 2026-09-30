use toasty::Db;
use topcoat::view::ViewExt;

use super::*;
use crate::{
    Panel,
    panel::test_support::{Dummy, dummy_table, panel_for, response_html},
    schema::{Field, Schema},
};

/// Completion fills an unnamed key from the stored projection it is
/// handed — the edit path hands it the in-transaction record — keeps a
/// named key as posted, and drops an unnamed key the projection lacks.
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
async fn edit_post_requires_can_view_as_well_as_can_update() {
    use crate::resource::Resource;

    struct ViewDeniedResource;
    impl Resource for ViewDeniedResource {
        type Model = Dummy;
        type Form = ViewDeniedForm;
        fn form(_cx: &Cx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            false
        }
        fn can_update(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
            dummy_table(cx)
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct ViewDeniedForm {
        name: String,
    }
    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = panel_for::<ViewDeniedResource>(db)
        .build()
        .expect("panel builds");
    let url = format!("/admin/dummies/{}/edit", row.id);
    // GET already required both; POST must match.
    let get = router
        .handle(
            http::Request::builder()
                .uri(&url)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(get.status(), http::StatusCode::FORBIDDEN);
    // Valid CSRF token still 403 on policy (not on CSRF).
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

/// Framework transport keys never reach the write: the create POST carries
/// `csrf_token` (and, for file schemas, `clear_<field>` and the
/// `keep_<field>` candidate a re-rendered form adds), which the framework
/// strips before the parse, and the client-typed candidate is never stored.
#[tokio::test]
async fn transport_keys_never_reach_the_write() {
    use crate::schema::{Field, Schema};

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
        fn form(_cx: &Cx) -> Schema {
            Schema::new((
                Field::text(Doc::fields().title()),
                Field::file(Doc::fields().path()),
            ))
        }

        fn slug() -> String {
            "docs".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table(_cx: &Cx) -> crate::resource::Table<Doc> {
            crate::resource::Table::new(
                |d: &Doc| d.id.to_string(),
                crate::resource::TextColumn::r#for(Doc::fields().title(), |d: &Doc| {
                    d.title.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Doc)]
    struct CapturingForm {
        title: String,
        path: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Doc))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = panel_for::<CapturingResource>(db.clone())
        .build()
        .expect("panel builds");
    let csrf = uuid::Uuid::new_v4().to_string();
    // `path` is a `FileUpload`, so it arrives as a file part;
    // `clear_path`, the client-typed `keep_path` candidate and
    // `csrf_token` are the transport keys under test.
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

/// GH #229, create half: a write that fails at the driver surfaces the
/// opaque mapping, never the driver's own text — the property
/// `db.rs` pins for `unavailable`, one layer up and through the real
/// create handler.
#[tokio::test]
async fn a_driver_create_failure_does_not_echo_driver_text() {
    use topcoat::{context::CxTestBuilder, cookie::CookieJarCell};

    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

    struct WritingResource;
    impl Resource for WritingResource {
        type Model = Dummy;
        type Form = WritingForm;
        fn form(_cx: &Cx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }

        fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
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
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct WritingForm {
        name: String,
    }
    // Schema never pushed: the INSERT cannot run, so the failure is the
    // driver's own (the `unique_check_propagates_probe_errors` setup).
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();

    // Positive control: the same insert outside the handler really does
    // carry driver text, so the assertions below cannot pass vacuously.
    let mut raw = db.clone();
    let driver = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut raw)
    .await
    .expect_err("the table is missing")
    .to_string();
    drop(raw);
    assert!(
        driver.contains("no such table"),
        "the control must be a driver failure, got {driver:?}"
    );

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
    let cx = CxTestBuilder::new()
        .app_context(db)
        .request_context(parts)
        .request_context(CookieJarCell::new())
        .build();

    let error = resource_create_post::<WritingResource>(
        &cx,
        Body::from(format!("name=Ada&csrf_token={token}")),
    )
    .first()
    .await
    .expect_err("the write must fail");

    let rendered = error.to_string();
    assert!(
        rendered.contains("database unavailable"),
        "the opaque message must survive, got {rendered:?}"
    );
    assert!(
        !rendered.contains(&driver) && !rendered.contains("no such table"),
        "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
    );
}

/// GH #229, edit half: the update arm is the same seam as create's, and a
/// write that fails at the driver must not echo the driver's text there
/// either. The failing write is a unique violation the app-side check
/// never saw — the case the arm's own comment names (upstream gap #117).
///
/// The edit handler needs the `{id}` the router captures, so the test
/// mounts it behind a route of its own and renders the error it returns —
/// the body is exactly what a page would be handed.
#[tokio::test]
async fn a_driver_update_failure_does_not_echo_driver_text() {
    use topcoat::{
        cookie::RouterBuilderCookieExt,
        router::{RouteFn, RouteFuture, Router, response::IntoResponse},
    };

    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

    // The hook's own write targets this model: its unique column is not
    // one the panel's form probes, so the duplicate is the driver's to
    // refuse.
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
        fn form(_cx: &Cx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }
        async fn update_record(
            _cx: &Cx,
            record: Dummy,
            _posted: crate::form::Posted<EditingForm>,
            ex: &mut dyn toasty::Executor,
        ) -> Result<Dummy> {
            // The write the hook performs is the one that fails: the name
            // is taken, and only the database knows it.
            toasty::create!(Ghost {
                name: "taken".to_string(),
            })
            .exec(&mut *ex)
            .await?;
            Ok(record)
        }

        fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
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
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn can_update(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct EditingForm {
        name: String,
    }
    /// Runs the edit handler under a route that captures `{id}`, and hands
    /// its error back as the body.
    fn edit_error(cx: &Cx, body: Body) -> RouteFuture<'_> {
        Box::pin(async move {
            let error = resource_edit_post::<EditingResource>(cx, body)
                .first()
                .await
                .expect_err("the write must fail");
            error.to_string().into_response(cx)
        })
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy, Ghost))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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

    // Positive control: the hook's own write really does carry driver
    // text, so the assertions below cannot pass vacuously.
    let mut raw = db.clone();
    let driver = toasty::create!(Ghost {
        name: "taken".to_string(),
    })
    .exec(&mut raw)
    .await
    .expect_err("the name is taken")
    .to_string();
    drop(raw);
    assert!(
        driver.contains("UNIQUE constraint failed"),
        "the control must be a driver failure, got {driver:?}"
    );

    let router = Router::builder()
        .cookies()
        .app_context(db)
        .route(RouteFn::new(
            http::Method::POST,
            "/admin/capture/{id}",
            edit_error,
        ))
        .build();
    let token = uuid::Uuid::new_v4().to_string();
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(format!("/admin/capture/{}", row.id))
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

    assert!(
        rendered.contains("database unavailable"),
        "the opaque message must survive, got {rendered:?}"
    );
    assert!(
        !rendered.contains(&driver) && !rendered.contains("UNIQUE constraint failed"),
        "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
    );
}

/// Post/Redirect/Get (#126): a mutation answers 303, the flash
/// cookie rides the error response (Topcoat flushes `Set-Cookie` on `Err`,
/// topcoat#408), and nothing rides the `Location` query. Following the
/// redirect consumes the cookie, so a reload does not replay the toast.
#[tokio::test]
async fn mutation_redirect_carries_the_flash_cookie_instead_of_a_query() {
    use crate::resource::Resource;

    const COOKIE_NAME: &str = crate::notification::COOKIE_NAME;

    struct NotifyingResource;
    impl Resource for NotifyingResource {
        type Model = Dummy;
        type Form = NotifyingForm;
        fn form(_cx: &Cx) -> crate::schema::Schema {
            // A real field, optional so the test's csrf-only POST still
            // passes validation: the record form's field needs a control
            // to bind (the key-agreement build check).
            crate::schema::Schema::new(
                crate::schema::Field::text(Dummy::fields().name()).optional(),
            )
        }
        async fn create_record(
            _cx: &Cx,
            _form: NotifyingForm,
            ex: &mut dyn toasty::Executor,
        ) -> Result<Dummy> {
            // The row the write produced is what the handler needs back
            // so a test double writes a real one.
            toasty::create!(Dummy {
                name: "created".to_string(),
            })
            .exec(&mut *ex)
            .await
            .map_err(|error| -> topcoat::Error { error.into() })
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_create(_cx: &Cx) -> bool {
            true
        }
        fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct NotifyingForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = panel_for::<NotifyingResource>(db)
        .build()
        .expect("panel builds");
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

/// GH #189 acceptance, through the real panel: two submits with an empty
/// `unique()` field re-render inline and write nothing. Before the fix the
/// first empty submit *succeeded* — it stored `""` — so the panel had
/// already broken the promise its own unique index makes, and the second
/// empty submit met the constraint instead of the form rule: 500 when the
/// record fn stores the value as submitted, or a misleading "has already
/// been taken" when it trims first.
#[tokio::test]
async fn two_empty_submits_on_a_unique_field_re_render_and_write_nothing() {
    use crate::{
        resource::{Resource, Table, TextColumn},
        schema::{Field, Schema},
    };

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        email: String,
    }
    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = SubscriberForm;
        fn form(_cx: &Cx) -> Schema {
            // `.optional()` lets an empty submit probe instead of failing
            // on presence: uniqueness wins.
            Schema::new(
                Field::text(Subscriber::fields().email())
                    .unique()
                    .optional(),
            )
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
        fn table(_cx: &Cx) -> Table<Subscriber> {
            Table::new(
                |s: &Subscriber| s.id.to_string(),
                TextColumn::r#for(Subscriber::fields().email(), |s: &Subscriber| {
                    s.email.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Subscriber)]
    struct SubscriberForm {
        email: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = panel_for::<SubscriberResource>(db.clone())
        .build()
        .expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    // `+` decodes to a space and an empty pair to `""`: both trim to an
    // empty submit, which the presence rule refuses and which must not
    // reach the database. Neither may write.
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

/// `Uploader::holds` defaults to `false`, so a store that does not
/// implement it cannot vouch for a carried path — a forged `keep_<field>`
/// leaves the field empty and the create refuses.
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

    /// A store that implements only `store`: `holds` stays the default.
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
        fn form(_cx: &Cx) -> Schema {
            Schema::new((
                Field::text(Doc::fields().title()),
                Field::file(Doc::fields().path()),
            ))
        }

        fn slug() -> String {
            "docs".to_string()
        }

        fn can_view_any(_cx: &Cx) -> bool {
            true
        }

        fn can_create(_cx: &Cx) -> bool {
            true
        }

        fn table(_cx: &Cx) -> crate::resource::Table<Doc> {
            crate::resource::Table::new(
                |row: &Doc| row.id.to_string(),
                crate::resource::TextColumn::r#for(Doc::fields().title(), |row: &Doc| {
                    row.title.clone()
                }),
            )
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Doc)]
    struct DocForm {
        title: String,
        path: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Doc))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = Panel::new("admin")
        .app_context(db.clone())
        .uploads(NoHoldsUploader)
        .resource::<DocResource>()
        .auth(crate::Auth::disabled())
        .build()
        .expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    // A forged candidate with no file part: nothing stored the path.
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
