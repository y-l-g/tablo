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

#[derive(Debug, toasty::Model)]
struct Team {
    #[key]
    #[auto]
    id: uuid::Uuid,
}

#[derive(Debug, toasty::Model)]
struct Player {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    squad_id: uuid::Uuid,
    #[belongs_to(key = squad_id, references = id)]
    squad: toasty::Deferred<Team>,
    badge_id: String,
}

/// A `belongs_to` relation's foreign key reads the relation's label, named after the relation
/// rather than the model it targets; another field ending in `_id` keeps its own.
#[test]
fn a_foreign_key_takes_its_relations_label() {
    let label = |path: &ModelPath| field::<Player>(path).unwrap().label.key();
    assert_eq!(
        label(&ModelPath::of(&Player::fields().squad_id())),
        "field-squad"
    );
    assert_eq!(
        label(&ModelPath::of(&Player::fields().badge_id())),
        "field-badge_id"
    );
}
