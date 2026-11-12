use proptest::{collection, prelude::*};

use super::*;
use crate::schema::{Field, TextField};

const ROOTS: usize = 3;

/// A text field's declaration: the `default` it takes, whether it renders disabled, and, for a
/// field past the [`ROOTS`], the root its visibility and its control watch.
#[derive(Debug, Clone)]
struct Spec {
    default: Option<&'static str>,
    disabled: Disabled,
    visible_when: Option<(usize, Vec<&'static str>)>,
    disabled_when: Option<(usize, Vec<&'static str>)>,
    mask: Mask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disabled {
    Never,
    Always,
    OnEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mask {
    None,
    Hidden,
    Locked,
}

#[derive(Debug, Clone)]
struct Case {
    specs: Vec<Spec>,
    editing: bool,
    unposted: Unposted,
    posted: Vec<Option<&'static str>>,
    stored: Vec<Option<&'static str>>,
}

fn name(index: usize) -> String {
    format!("f{index}")
}

fn word() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("a"), Just("b"), Just("")]
}

fn condition() -> impl Strategy<Value = Option<(usize, Vec<&'static str>)>> {
    proptest::option::of((0..ROOTS, collection::vec(word(), 1..3)))
}

fn spec(index: usize) -> impl Strategy<Value = Spec> {
    let conditional = index >= ROOTS;
    (
        proptest::option::of(prop_oneof![Just("a"), Just("b")]),
        prop_oneof![
            Just(Disabled::Never),
            Just(Disabled::Always),
            Just(Disabled::OnEdit)
        ],
        condition(),
        condition(),
        prop_oneof![
            3 => Just(Mask::None),
            1 => Just(Mask::Hidden),
            1 => Just(Mask::Locked)
        ],
    )
        .prop_map(
            move |(default, disabled, visible_when, disabled_when, mask)| Spec {
                default,
                disabled,
                visible_when: visible_when.filter(|_| conditional),
                disabled_when: disabled_when.filter(|_| conditional),
                mask,
            },
        )
}

fn case() -> impl Strategy<Value = Case> {
    (ROOTS..ROOTS + 4)
        .prop_flat_map(|count| {
            (
                (0..count).map(spec).collect::<Vec<_>>(),
                any::<bool>(),
                prop_oneof![Just(Unposted::Missing), Just(Unposted::Defaulted)],
                collection::vec(proptest::option::of(word()), count),
                collection::vec(proptest::option::of(word()), count),
            )
        })
        .prop_map(|(specs, editing, unposted, posted, stored)| Case {
            specs,
            editing,
            // Only an import row defaults what it leaves out, and an import only creates.
            unposted: if editing { Unposted::Missing } else { unposted },
            posted,
            stored,
        })
}

fn field(index: usize, spec: &Spec) -> TextField {
    let watched = |root: usize| Field::text_input::<String>(name(root));
    let mut field = Field::text_input::<String>(name(index));
    if let Some(default) = spec.default {
        field = field.default(default);
    }
    field = match spec.disabled {
        Disabled::Never => field,
        Disabled::Always => field.disabled(),
        Disabled::OnEdit => field.disabled_on_edit(),
    };
    if let Some((root, values)) = &spec.visible_when {
        field = field.visible_when(&watched(*root), values.clone());
    }
    if let Some((root, values)) = &spec.disabled_when {
        field = field.disabled_when(&watched(*root), values.clone());
    }
    field
}

/// The case's schema, declaring its fields in `order`.
fn schema(case: &Case, order: impl Iterator<Item = usize>) -> Schema {
    order
        .map(|index| Schema::new(field(index, &case.specs[index])))
        .reduce(Schema::extend)
        .expect("a case declares ROOTS fields at least")
}

fn values(words: &[Option<&'static str>]) -> FormValues {
    words
        .iter()
        .enumerate()
        .filter_map(|(index, word)| Some((name(index), (*word)?.to_string())))
        .collect()
}

fn mask(case: &Case) -> FormMask {
    let keyed = |mask: Mask| {
        case.specs
            .iter()
            .enumerate()
            .filter(move |(_, spec)| spec.mask == mask)
            .map(|(index, _)| name(index))
    };
    FormMask::new(keyed(Mask::Hidden), keyed(Mask::Locked))
}

fn resolved(case: &Case, order: impl Iterator<Item = usize>) -> (FieldStates, FormValues) {
    let stored = if case.editing {
        values(&case.stored)
    } else {
        FormValues::new()
    };
    schema(case, order).resolve(
        &values(&case.posted),
        Posting {
            editing: case.editing,
            mask: &mask(case),
            unposted: case.unposted,
            stored: &stored,
        },
    )
}

/// Whether the field declared by `spec` renders no enabled control whatever the submission
/// posts.
fn withheld_always(spec: &Spec, editing: bool) -> bool {
    spec.mask != Mask::None
        || match spec.disabled {
            Disabled::Never => false,
            Disabled::Always => true,
            Disabled::OnEdit => editing,
        }
}

/// What a condition reads for the root `index`, which no condition guards, as the browser
/// renders the form: the value its enabled control posts; a withheld control's default on the
/// create form or stored value on the edit form; the stored value of a key the submission leaves
/// out.
fn watched_value(case: &Case, index: usize) -> &'static str {
    let spec = &case.specs[index];
    let stored = case.stored[index].filter(|_| case.editing);
    let read = if withheld_always(spec, case.editing) {
        if case.editing { stored } else { spec.default }
    } else {
        case.posted[index]
            .or(spec
                .default
                .filter(|_| case.unposted == Unposted::Defaulted))
            .or(stored)
    };
    read.unwrap_or("")
}

fn holds(case: &Case, condition: &Option<(usize, Vec<&'static str>)>) -> Option<bool> {
    let (root, values) = condition.as_ref()?;
    Some(values.contains(&watched_value(case, *root)))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// A field's state follows from its declaration, the request's mask, and what its conditions
    /// read in the submission as the browser renders it.
    #[test]
    fn each_state_follows_the_controls_the_browser_renders(case in case()) {
        let (states, _) = resolved(&case, 0..case.specs.len());
        for (index, spec) in case.specs.iter().enumerate() {
            let hidden = holds(&case, &spec.visible_when) == Some(false);
            let expected = if hidden {
                FieldState::Hidden
            } else if withheld_always(spec, case.editing)
                || holds(&case, &spec.disabled_when) == Some(true)
            {
                FieldState::Withheld
            } else {
                FieldState::Posted
            };
            prop_assert_eq!(states.get(&name(index)), Some(expected), "field {}", index);
        }
    }

    /// Only a posted control's value reaches the parse: a withheld field takes its `default` on
    /// a create and nothing on an edit, which keeps the stored value, and a hidden field takes
    /// nothing, whatever the submission carries for either.
    #[test]
    fn only_a_posted_control_keeps_what_the_submission_carries(case in case()) {
        let (states, settled) = resolved(&case, 0..case.specs.len());
        for (index, spec) in case.specs.iter().enumerate() {
            let key = name(index);
            let default = spec.default.filter(|_| !case.editing);
            let expected = match states.get(&key) {
                Some(FieldState::Posted) => case.posted[index]
                    .or(spec.default.filter(|_| case.unposted == Unposted::Defaulted)),
                Some(FieldState::Withheld) => default,
                Some(FieldState::Hidden) | None => None,
            };
            prop_assert_eq!(settled.text(&key), expected, "field {}", index);
            prop_assert_eq!(
                states.posts(&key),
                states.get(&key) == Some(FieldState::Posted),
                "field {}", index
            );
        }
    }

    /// The order a schema declares its fields in changes no state and no value.
    #[test]
    fn declaration_order_changes_nothing(case in case()) {
        let (forward, forward_values) = resolved(&case, 0..case.specs.len());
        let (backward, backward_values) = resolved(&case, (0..case.specs.len()).rev());
        prop_assert_eq!(forward_values, backward_values);
        for index in 0..case.specs.len() {
            prop_assert_eq!(forward.get(&name(index)), backward.get(&name(index)));
        }
    }
}

/// A key no field declares stands as posted.
#[test]
fn a_key_no_field_declares_is_posted() {
    let schema = Schema::new(Field::text_input::<String>("name").disabled());
    let (states, _) = schema.resolve(
        &FormValues::new(),
        Posting {
            editing: false,
            mask: &FormMask::default(),
            unposted: Unposted::Missing,
            stored: &FormValues::new(),
        },
    );
    assert!(!states.posts("name"), "a disabled control posts nothing");
    assert!(states.posts("other"));
}
