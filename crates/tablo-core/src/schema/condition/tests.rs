use super::*;
use crate::{
    schema::{Group, Section},
    values::FormValues,
};

#[derive(Debug, toasty::Model)]
struct Customer {
    #[key]
    #[auto]
    id: uuid::Uuid,
    kind: String,
    vat: Option<String>,
    company: Option<String>,
    note: Option<String>,
    active: bool,
}

fn kind() -> ChoiceField {
    Field::choice(Customer::fields().kind()).options(["person", "company"])
}

fn values(kind: &str) -> FormValues {
    FormValues::from_iter([("kind".to_string(), kind.to_string())])
}

/// A field's own condition and a block's both hide what they guard, and a submission naming one
/// of the condition's values shows it.
#[test]
fn a_failing_condition_hides_the_fields_it_guards() {
    let kind = kind();
    let vat = Field::text(Customer::fields().vat()).visible_when(&kind, ["company"]);
    let block = Group::new()
        .schema(Field::text(Customer::fields().company()))
        .visible_when(&kind, ["company", "partner"]);
    let schema = Schema::new((kind, vat, block, Field::text(Customer::fields().note())));

    let mut hidden = schema.condition_hidden(&values("person"));
    hidden.sort();
    assert_eq!(hidden, ["company", "vat"]);
    assert!(schema.condition_hidden(&values("company")).is_empty());
    assert_eq!(
        schema.condition_hidden(&values("partner")),
        ["vat"],
        "a block shows for any of its values"
    );
    assert_eq!(
        schema.hidden_fields(&FormValues::new()).len(),
        2,
        "a submission without the watched field hides what it guards"
    );
}

/// Two forms on one page, such as an action's input in a dialog over a record's form, follow
/// their own watched fields though they post the same keys: the browser keeps one value per
/// signal, so a shared one would show both forms' fields from whichever changed last. Another
/// page's form starts from its own values.
#[test]
fn each_form_and_page_names_its_own_signal() {
    let company = Condition::watching("kind", ["company"]);
    let key = |page, scope| signal_key(page, scope, &company);
    assert_eq!(key("/a", "first"), key("/a", "first"));
    assert_ne!(key("/a", "first"), key("/a", "second"));
    assert_ne!(key("/a", "first"), key("/b", "first"));
    assert_ne!(
        key("/a", "first"),
        signal_key("/a", "first", &Condition::watching("kind", ["person"]))
    );
}

#[test]
fn a_condition_must_watch_a_placed_field() {
    let kind = kind();
    let vat = Field::text(Customer::fields().vat()).visible_when(&kind, ["company"]);
    assert_eq!(
        Schema::new(vat).declaration_errors(),
        [DeclarationErrorKind::UnplacedWatchedField {
            field: "kind".to_string()
        }]
    );
}

/// A submission hiding a field posts nothing for it, which a field with no blank answer refuses.
#[test]
fn a_conditional_field_needs_a_blank_answer() {
    let kind = kind();
    let note = Field::text(Customer::fields().note()).required();
    let block = Section::new("Company")
        .schema(note)
        .visible_when(&kind, ["company"]);
    assert_eq!(
        Schema::new((kind, block)).declaration_errors(),
        [DeclarationErrorKind::RequiredConditionalField {
            field: "note".to_string()
        }]
    );
}

/// A watched field inside a hidden block posts nothing, so what it guards must sit in that block
/// too, where the browser hides it as well.
#[test]
fn a_hidden_watched_field_guards_only_inside_its_block() {
    let nested = || {
        let kind = kind();
        let company = Field::choice(Customer::fields().company()).options(["eu", "other"]);
        let vat = Field::text(Customer::fields().vat()).visible_when(&company, ["eu"]);
        (kind, company, vat)
    };

    let (kind, company, vat) = nested();
    let inside = Group::new()
        .schema((company, vat))
        .visible_when(&kind, ["company"]);
    assert!(Schema::new((kind, inside)).declaration_errors().is_empty());

    let (kind, company, vat) = nested();
    let outside = Group::new()
        .schema(company)
        .visible_when(&kind, ["company"]);
    assert_eq!(
        Schema::new((kind, outside, vat)).declaration_errors(),
        [DeclarationErrorKind::HiddenWatchedField {
            field: "company".to_string()
        }]
    );
}

/// An unchecked toggle renders and posts `false`, so a form with no value for it yet shows what a
/// `false` condition guards.
#[test]
fn a_toggle_without_a_value_reads_as_unchecked() {
    let active = Field::toggle(Customer::fields().active());
    let note = Field::text(Customer::fields().note()).visible_when(&active, ["false"]);
    let schema = Schema::new((active, note));
    assert!(schema.condition_hidden(&FormValues::new()).is_empty());
    let checked = FormValues::from_iter([("active".to_string(), "true".to_string())]);
    assert_eq!(schema.condition_hidden(&checked), ["note"]);
}

/// A value the watched field never posts would keep what it guards hidden for good.
#[test]
fn a_condition_names_values_the_watched_field_posts() {
    let kind = kind();
    let vat = Field::text(Customer::fields().vat()).visible_when(&kind, ["Company"]);
    let active = Field::toggle(Customer::fields().active());
    let note = Field::text(Customer::fields().note()).visible_when(&active, ["yes", "true"]);
    assert_eq!(
        Schema::new((kind, vat, active, note)).declaration_errors(),
        [
            DeclarationErrorKind::UnpostedConditionValue {
                field: "kind".to_string(),
                value: "Company".to_string()
            },
            DeclarationErrorKind::UnpostedConditionValue {
                field: "active".to_string(),
                value: "yes".to_string()
            },
        ]
    );
}
