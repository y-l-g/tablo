use super::*;
use crate::values::FieldErrorKind;

#[test]
fn a_list_parses_each_item_and_an_absent_one_as_empty() {
    let values = FormValues::from_iter([("ids", vec!["1".to_string(), "3".to_string()])]);
    assert_eq!(parse_list::<i64>("ids", &values), Ok(vec![1, 3]));
    assert_eq!(parse_list::<i64>("other", &values), Ok(Vec::new()));
    let refused = parse_list::<i64>(
        "ids",
        &FormValues::from_iter([("ids", vec!["x".to_string()])]),
    );
    assert!(matches!(
        refused,
        Err(FieldError {
            kind: FieldErrorKind::Invalid(_),
            ..
        })
    ));
}

#[test]
fn a_key_of_another_shape_refuses_as_invalid() {
    let values = FormValues::from_iter([
        ("text", FormValue::Text("1".into())),
        ("list", FormValue::List(vec!["1".into()])),
    ]);
    fn invalid<T>(result: Result<T, FieldError>, at: &str) -> bool {
        matches!(result, Err(FieldError { key, kind: FieldErrorKind::Invalid(_) }) if key == at)
    }
    assert!(invalid(parse_list::<i64>("text", &values), "text"));
    assert!(invalid(parse_scalar::<i64>("list", &values, None), "list"));
}

/// `value` posted under `key`, as a completed submission holds it.
fn posted(key: &str, value: &str) -> FormValues {
    FormValues::from_iter([(key.to_string(), value.to_string())])
}

/// `parsed` with its error's key and English message.
fn spelled<T>(parsed: Result<T, FieldError>) -> Result<T, (String, String)> {
    parsed.map_err(|error| {
        (
            error.key.clone(),
            error.message(error.key.as_str()).english(),
        )
    })
}

#[test]
fn a_bad_submission_is_refused_naming_the_input() {
    assert_eq!(
        spelled(parse_scalar::<i64>(
            "word_count",
            &posted("word_count", "lots"),
            None
        )),
        Err((
            "word_count".to_owned(),
            "`lots` is not a valid whole number".to_owned()
        )),
        "an unparseable integer names the offending input"
    );
    assert_eq!(
        spelled(parse_scalar::<jiff::Timestamp>(
            "recorded_at",
            &posted("recorded_at", "2024-13-01"),
            None
        )),
        Err((
            "recorded_at".to_owned(),
            "`2024-13-01` is not a valid timestamp".to_owned()
        )),
        "an unparseable date names the offending input"
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
        parse_scalar::<i64>("word_count", &FormValues::new(), None).expect_err("no blank answer");
    assert_eq!(refused.kind, FieldErrorKind::Required);
}

#[test]
fn a_text_field_stores_what_was_typed_trimmed() {
    assert_eq!(
        parse_scalar::<String>("label", &posted("label", "  spaced  "), None),
        Ok("spaced".to_string())
    );
}

/// What is written is the type's own spelling, so a re-read is a fixpoint, and no instant is lost
/// to an offset or a fraction of a second.
#[test]
fn a_timestamp_round_trips_offset_and_subsecond_precision() {
    for input in [
        "2024-01-02T03:04:05+00:00[UTC]",
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
        assert_eq!(reread.to_string(), stored, "{input}");
        let original: jiff::Timestamp = input.parse().expect("input parses");
        assert_eq!(original, reread, "no instant is lost from {input}");
    }
}
