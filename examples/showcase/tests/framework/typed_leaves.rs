use std::collections::HashMap;

use tablo::{
    Ability, FieldError, FieldErrorKind, FieldErrors, FormScalar,
    form::parse_scalar,
    schema::{Field, Schema, Source},
};
use toasty::Db;
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

use crate::framework::common::{field_error, input_value, mount};

#[derive(Debug, toasty::Model)]
struct Measurement {
    #[key]
    #[auto]
    id: uuid::Uuid,
    label: String,
    word_count: i64,
    recorded_at: jiff::Timestamp,
}

/// `value` posted under `key`, as a completed submission holds it.
fn posted(key: &str, value: &str) -> HashMap<String, String> {
    HashMap::from([(key.to_string(), value.to_string())])
}

async fn cx() -> Cx {
    let db = Db::builder()
        .models(toasty::models!(Measurement))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    CxTestBuilder::new().app_context(db).build()
}

#[tokio::test]
async fn a_typed_field_renders_the_values_display() {
    let cx = cx().await;
    let schema = Schema::new((
        Field::text(Measurement::fields().word_count()),
        Field::text(Measurement::fields().recorded_at()),
    ));
    let values = HashMap::from([
        ("word_count".to_string(), "1240".to_string()),
        (
            "recorded_at".to_string(),
            "2024-01-02T03:04:05Z".to_string(),
        ),
    ]);
    let html = schema
        .render(&cx, Source::form(&values, &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        input_value(&html, "word_count").as_deref(),
        Some("1240"),
        "an integer field renders its value: {html}"
    );
    assert!(
        html.contains("type=\"datetime-local\""),
        "a typed timestamp is a datetime-local input: {html}"
    );
    assert_eq!(
        input_value(&html, "recorded_at").as_deref(),
        Some("2024-01-02T03:04"),
        "the timestamp renders in UTC for the control: {html}"
    );
    assert!(html.contains("Word count"), "label from the lens: {html}");
}

#[test]
fn a_bad_submission_is_refused_naming_the_input() {
    assert_eq!(
        parse_scalar::<i64>("word_count", &posted("word_count", "lots"), None),
        Err(FieldError::invalid(
            "word_count",
            "`lots` is not a valid whole number"
        )),
        "an unparseable integer names the offending input"
    );
    assert_eq!(
        parse_scalar::<jiff::Timestamp>("recorded_at", &posted("recorded_at", "2024-13-01"), None),
        Err(FieldError::invalid(
            "recorded_at",
            "`2024-13-01` is not a valid timestamp"
        )),
        "an unparseable date names the offending input"
    );
}

#[test]
fn a_valid_submission_is_stored_in_the_types_spelling() {
    let parsed = parse_scalar::<jiff::Timestamp>(
        "recorded_at",
        &posted("recorded_at", "2024-01-02T03:04:05+00:00[UTC]"),
        None,
    )
    .expect("the parser accepts what the type accepts");
    let stored = parsed.to_form();
    let reread: jiff::Timestamp = stored.parse().expect("the stored spelling parses");
    assert_eq!(
        reread.to_string(),
        stored,
        "what is written is the type's own Display, so a re-read is a fixpoint"
    );
}

/// An empty submission is the field's blank answer, or refused as required when it has none,
/// never a parse failure.
#[test]
fn an_empty_submission_is_the_blank_answer_or_required() {
    assert_eq!(
        parse_scalar::<i64>("word_count", &posted("word_count", ""), Some(0)),
        Ok(0)
    );
    let refused =
        parse_scalar::<i64>("word_count", &HashMap::new(), None).expect_err("no blank answer");
    assert_eq!(refused.kind, FieldErrorKind::Required);
    assert_eq!(refused.message("Word count"), "Word count is required");
}

#[test]
fn a_text_field_stores_what_was_typed_trimmed() {
    assert_eq!(
        parse_scalar::<String>("label", &posted("label", "  spaced  "), None),
        Ok("spaced".to_string())
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
    use tablo::{Auth, Panel, Resource, ResourceDef};
    use topcoat::router::{Body, Router};

    #[derive(Debug, toasty::Model, Clone)]
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
                .policy(|_cx: &Cx, ability: Ability<'_, Reading>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(tablo::Table::new(tablo::ComputedColumn::new(
                    "Words",
                    |r: &Reading| r.word_count.to_string(),
                )))
                .form(Schema::new(Field::text(Reading::fields().word_count())))
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
                .uri("/admin/readings/create")
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
        field_error(&html, "word_count").as_deref(),
        Some("`lots` is not a valid whole number"),
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

#[test]
fn a_timestamp_round_trips_offset_and_subsecond_precision() {
    for input in [
        "2024-01-02T03:04:05.123456789Z",
        "2024-01-02T03:04:05+05:30",
        "2024-06-30T23:59:59Z",
    ] {
        let parsed =
            parse_scalar::<jiff::Timestamp>("recorded_at", &posted("recorded_at", input), None)
                .unwrap_or_else(|error| panic!("{input} must parse: {error:?}"));
        let stored = parsed.to_form();
        let reread: jiff::Timestamp = stored
            .parse()
            .unwrap_or_else(|e| panic!("{stored:?} must parse back: {e}"));
        assert_eq!(
            reread.to_string(),
            stored,
            "the stored spelling is the type's fixpoint for {input}"
        );
        let original: jiff::Timestamp = input.parse().expect("input parses");
        assert_eq!(
            original, reread,
            "no instant is lost between {input} and {stored}"
        );
    }
}
