use super::*;
use crate::test_support::DummyUser;

#[test]
fn a_single_step_path_names_its_field_and_a_traversal_is_refused() {
    let single = ModelPath(toasty_core::stmt::Path::field(
        <DummyUser as toasty::schema::Model>::id(),
        0,
    ));
    assert_eq!(single_segment(&single), Ok(0));
    let mut two = single.clone();
    two.0.chain(&toasty_core::stmt::Path::field(
        <DummyUser as toasty::schema::Model>::id(),
        1,
    ));
    assert_eq!(
        single_segment(&two),
        Err(DeclarationErrorKind::TraversalLens { steps: 2 })
    );
}

/// Reports the field's index, name, label and nullability.
#[test]
fn a_field_reports_its_index_name_label_and_nullability() {
    let email = field::<DummyUser>(&ModelPath::of(&DummyUser::fields().email())).unwrap();
    assert_eq!(email.index, 2);
    assert_eq!(email.name, "email");
    assert_eq!(email.label, "Email");
    assert!(!email.nullable);
    assert!(!email.relation);

    let name = field::<DummyUser>(&ModelPath::of(&DummyUser::fields().name())).unwrap();
    assert_eq!(name.index, 1);
    assert_eq!(name.label, "Name");
}

/// Reports uniqueness from the model's index list, where `#[unique]` lives.
#[test]
fn uniqueness_reads_the_model_index_list() {
    let unique = |path: &ModelPath| field::<DummyUser>(path).unwrap().unique;
    assert!(
        unique(&ModelPath::of(&DummyUser::fields().email())),
        "#[unique] on email must surface as a single-field unique index"
    );
    assert!(
        !unique(&ModelPath::of(&DummyUser::fields().name())),
        "a field with no unique index must not report unique"
    );
    assert!(
        !unique(&ModelPath::of(&DummyUser::fields().id())),
        "the primary key is unique by construction, not by declared constraint"
    );
}

/// Lists every root field once, at its own index.
#[test]
fn fields_list_the_root_in_declaration_order() {
    let names: Vec<(usize, String)> = fields::<DummyUser>()
        .into_iter()
        .map(|field| (field.index, field.name))
        .collect();
    assert_eq!(
        names,
        [
            (0, "id".to_string()),
            (1, "name".to_string()),
            (2, "email".to_string())
        ]
    );
}
