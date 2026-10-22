use topcoat::view::ViewExt;

use super::*;
use crate::{
    Ability, Panel, ResourceDef,
    form::FormValues,
    lens,
    panel::test_support::Dummy,
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
    let mut values = FormValues::from_iter([("name".to_string(), "advisory".to_string())]);
    let stored = FormValues::from_iter([("name".to_string(), "authoritative".to_string())]);
    complete(&schema, &mut values, &named, &stored);
    assert_eq!(
        values.text("name"),
        Some("authoritative"),
        "an unnamed key reads the stored record"
    );

    let named = HashSet::from(["name".to_string()]);
    let mut values = FormValues::from_iter([("name".to_string(), "posted".to_string())]);
    complete(&schema, &mut values, &named, &stored);
    assert_eq!(
        values.text("name"),
        Some("posted"),
        "a named key keeps what was posted"
    );

    let mut values = FormValues::from_iter([("name".to_string(), "stale".to_string())]);
    complete(&schema, &mut values, &HashSet::new(), &FormValues::new());
    assert!(!values.contains("name"), "nothing stored, nothing kept");
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
                .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
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
        .uri("/admin/dummies/-/create")
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
                .policy_fn(|_cx, ability| {
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
        ) -> Result<Dummy, crate::WriteError<EditingFormField>> {
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

    // The route sits under the panel's prefix, so the request answers as the panel.
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

    assert_opaque(&rendered, &driver, "UNIQUE constraint failed");
}

#[test]
fn only_a_repeater_owns_the_errors_under_its_key() {
    let form = Schema::new(Field::text(crate::test_support::DummyUser::fields().name()));
    assert!(renders_under(&form, "name", "name"));
    assert!(
        !renders_under(&form, "name", "name.0.label"),
        "a text field posts no rows"
    );
    assert!(!renders_under(&form, "name", "names"));
}
