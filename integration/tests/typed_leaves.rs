//! Typed form fields over HTTP: each control is the input its browser edits the type with, a bad
//! value is a field error on the page, and an untouched value keeps what the control cannot show.

use tablo::{Ability, Allow, Auth, Panel, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;

use crate::common::{TestClient, body_string, control, field_error, input_value, memory_db, mount};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Measurement {
    #[key]
    #[auto]
    id: uuid::Uuid,
    label: String,
    word_count: i64,
    ratio: Option<f64>,
    day: jiff::civil::Date,
    at: jiff::civil::Time,
    local: Option<jiff::civil::DateTime>,
    recorded_at: jiff::Timestamp,
}

#[derive(tablo::RecordForm)]
#[form(model = Measurement)]
struct MeasurementForm {
    label: String,
    word_count: i64,
    ratio: Option<f64>,
    day: jiff::civil::Date,
    at: jiff::civil::Time,
    local: Option<jiff::civil::DateTime>,
    recorded_at: jiff::Timestamp,
}

struct MeasurementResource;

impl Resource for MeasurementResource {
    type Model = Measurement;
    type Form = MeasurementForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("measurements")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Measurement.label))))
    }
}

/// Each typed control renders the input its browser edits it with, and the stored value in the
/// spelling that input accepts: a timestamp in UTC, to the minute.
#[tokio::test]
async fn a_typed_field_renders_its_input_and_the_stored_value() {
    let db = memory_db(toasty::models!(Measurement)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<MeasurementResource>(),
    )
    .expect("panel builds");
    let stored = toasty::create!(Measurement {
        label: "Morning".to_string(),
        word_count: 1240,
        ratio: Some(1.5),
        day: jiff::civil::date(2024, 1, 15),
        at: jiff::civil::time(9, 30, 15, 0),
        local: Some(jiff::civil::date(2024, 1, 15).at(9, 30, 0, 0)),
        recorded_at: "2024-01-02T03:04:05Z".parse::<jiff::Timestamp>().unwrap(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();

    let html = body_string(
        TestClient::new(&router)
            .get(&format!("/admin/measurements/{}/edit", stored.id))
            .await,
    )
    .await;
    for (name, input_type, value) in [
        ("word_count", "number", "1240"),
        ("ratio", "number", "1.5"),
        ("day", "date", "2024-01-15"),
        ("at", "time", "09:30"),
        ("local", "datetime-local", "2024-01-15T09:30"),
        ("recorded_at", "datetime-local", "2024-01-02T03:04"),
    ] {
        let tag = control(&html, name);
        assert!(
            tag.contains(&format!("type=\"{input_type}\"")),
            "{name}: {tag}"
        );
        assert_eq!(
            input_value(&html, name).as_deref(),
            Some(value),
            "{name}: {tag}"
        );
    }
    assert!(
        !control(&html, "word_count").contains("step="),
        "an integer steps by one"
    );
    assert!(
        control(&html, "ratio").contains("step=\"any\""),
        "a float takes any step"
    );
}

/// The whole point of the typed seam: a bad value typed into a typed
/// column is a **field error on the page**, not a 500 and not a silent default.
///
/// Pinned through the real panel, because the requirement is about what a user sees: the earlier
/// tests prove the parse, this proves the wiring — that the create handler reaches it and
/// re-renders inline.
#[tokio::test]
async fn a_bad_typed_submission_re_renders_inline_and_writes_nothing() {
    use topcoat::router::{Body, Router};

    #[derive(Debug, toasty::Model, tablo::ModelColumns, Clone)]
    struct Reading {
        #[key]
        #[auto]
        id: uuid::Uuid,
        word_count: i64,
    }
    struct ReadingResource;
    impl Resource for ReadingResource {
        type Model = Reading;
        type Form = ReadingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("readings")
                .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny | Ability::Create))
                .table(tablo::Table::new(tablo::ComputedColumn::new(
                    "Words",
                    |r: &Reading| r.word_count.to_string(),
                )))
        }
    }
    #[derive(tablo::RecordForm)]
    #[form(model = Reading)]
    struct ReadingForm {
        word_count: i64,
    }
    let db = Db::builder()
        .models(toasty::models!(Reading))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router: Router = mount(
        db.clone(),
        Panel::new("admin")
            .resource::<ReadingResource>()
            .auth(Auth::disabled()),
    )
    .expect("panel builds");

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/admin/readings/-/create")
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={csrf}", tablo::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("word_count=lots&csrf_token={csrf}")))
                .unwrap(),
        )
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::OK,
        "a bad typed value re-renders the form, it does not 500"
    );
    let body = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert_eq!(
        field_error(&html, "word_count"),
        Some(tablo::testing::ErrorKind::Invalid),
        "the field carries the parse error inline: {html}"
    );
    assert_eq!(
        input_value(&html, "word_count").as_deref(),
        Some("lots"),
        "the control keeps what the user typed so they can fix it: {html}"
    );

    let mut db_check = db;
    let stored = Reading::all().exec(&mut db_check).await.unwrap();
    assert!(
        stored.is_empty(),
        "a rejected submission writes nothing, got {} rows",
        stored.len()
    );
}

/// A timestamp's control edits to the minute: saving the form without touching it keeps the
/// stored seconds, and a changed minute is written as posted.
#[tokio::test]
async fn an_untouched_timestamp_keeps_its_seconds() {
    #[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
    struct Shift {
        #[key]
        #[auto]
        id: uuid::Uuid,
        label: String,
        starts_at: jiff::Timestamp,
    }
    #[derive(tablo::RecordForm)]
    #[form(model = Shift)]
    struct ShiftForm {
        label: String,
        starts_at: jiff::Timestamp,
    }
    struct ShiftResource;
    impl Resource for ShiftResource {
        type Model = Shift;
        type Form = ShiftForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("shifts")
                .policy(Allow)
                .table(Table::new(TextColumn::new(lens!(Shift.label))))
        }
    }
    let db = memory_db(toasty::models!(Shift)).await;
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<ShiftResource>(),
    )
    .expect("panel builds");
    let starts_at: jiff::Timestamp = "2026-03-04T05:06:07.5Z".parse().unwrap();
    let shift = toasty::create!(Shift {
        label: "Early".to_string(),
        starts_at,
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    let client = TestClient::new(&router);
    let edit = format!("/admin/shifts/{}/edit", shift.id);
    let stored = async || {
        Shift::filter(Shift::fields().id().eq(shift.id))
            .first()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .expect("the shift")
    };

    let response = client
        .submit(&edit, "label=Late&starts_at=2026-03-04T05%3A06")
        .await;
    assert_eq!(response.status(), 303);
    let saved = stored().await;
    assert_eq!(saved.label, "Late");
    assert_eq!(saved.starts_at, starts_at, "the untouched timestamp");

    let response = client
        .submit(&edit, "label=Late&starts_at=2026-03-04T05%3A07")
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(
        stored().await.starts_at,
        "2026-03-04T05:07:00Z".parse::<jiff::Timestamp>().unwrap(),
        "a changed minute"
    );
}
