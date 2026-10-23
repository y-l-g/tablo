use super::RichTextControl;
use crate::FieldRuleFault;

fn lengths(min: Option<usize>, max: Option<usize>) -> RichTextControl {
    RichTextControl {
        min_length: min,
        max_length: max,
        ..RichTextControl::default()
    }
}

#[test]
fn a_length_counts_the_text_not_the_markup() {
    let value = "<p><strong>abc</strong></p><p>de</p>";
    assert_eq!(lengths(None, Some(5)).refusal(&"Body".into(), value), None);
    assert!(
        lengths(None, Some(4))
            .refusal(&"Body".into(), value)
            .is_some()
    );
    assert_eq!(lengths(Some(5), None).refusal(&"Body".into(), value), None);
    assert!(
        lengths(Some(6), None)
            .refusal(&"Body".into(), value)
            .is_some()
    );
}

#[test]
fn crossed_lengths_are_a_fault() {
    assert!(matches!(
        lengths(Some(5), Some(4)).rule_faults().as_slice(),
        [FieldRuleFault::EmptyRange]
    ));
    assert!(lengths(Some(4), Some(4)).rule_faults().is_empty());
}

#[test]
fn rich_text_over_a_field_of_no_text_is_a_fault() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Post {
        #[key]
        #[auto]
        id: uuid::Uuid,
        body: String,
        views: i64,
    }
    let faults = |field: crate::Field| field.rule_errors("body");
    assert!(faults(crate::Field::text(Post::fields().body()).rich_text().0).is_empty());
    assert!(matches!(
        faults(crate::Field::text(Post::fields().views()).rich_text().0).as_slice(),
        [crate::DeclarationErrorKind::FieldRule {
            fault: FieldRuleFault::NotText("number"),
            ..
        }]
    ));
}

#[test]
fn a_posted_value_keeps_only_its_toolbar_s_tools() {
    let control = RichTextControl {
        toolbar: vec![vec![crate::Tool::Italic]],
        ..RichTextControl::default()
    };
    assert_eq!(
        control.clean("<p><strong>a</strong><em>b</em></p>"),
        "<p>a<em>b</em></p>"
    );
}
