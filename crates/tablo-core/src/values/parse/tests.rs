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
