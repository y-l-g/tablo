use std::collections::HashMap;

use super::*;
use crate::{
    form::FieldErrors,
    schema::{Group, Section, Source},
    test_support::{Html, cx},
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
}

fn kind() -> ChoiceField {
    Field::choice(Customer::fields().kind()).options(["person", "company"])
}

fn values(kind: &str) -> HashMap<String, String> {
    HashMap::from([("kind".to_string(), kind.to_string())])
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
        schema.hidden_fields(&HashMap::new()).len(),
        2,
        "a submission without the watched field hides what it guards"
    );
}

/// The browser disables a hidden field, so it neither validates nor posts it, and enables a shown
/// one.
#[tokio::test]
async fn a_hidden_field_renders_in_a_disabled_fieldset() {
    let cx = cx();
    let render = |kind_value: &'static str| {
        let cx = &cx;
        async move {
            let kind = kind();
            let vat = Field::text(Customer::fields().vat()).visible_when(&kind, ["company"]);
            let values = values(kind_value);
            let errors = FieldErrors::new();
            Schema::new((kind, vat))
                .render(cx, Source::form(&values, &errors))
                .await
                .html(cx)
                .await
        }
    };
    for (kind, disabled) in [("person", true), ("company", false)] {
        let html = render(kind).await;
        let input = html.find("name=\"vat\"").expect("the vat input");
        let fieldset = &html[html[..input].rfind("<fieldset").expect("its fieldset")..input];
        let tag = &fieldset[..fieldset.find('>').expect("a closed tag")];
        assert_eq!(
            tag.contains(" disabled"),
            disabled,
            "kind={kind}: the fieldset must be disabled exactly when hidden, got {tag}"
        );
        assert_eq!(tag.contains(" hidden"), disabled, "kind={kind}, got {tag}");
        let select = html.find("<select").expect("the kind select");
        assert!(
            html[..select].contains("data-topcoat-on:change"),
            "the watched field renders inside its change handler, got {html}"
        );
    }
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
