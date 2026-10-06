use std::collections::HashMap;

use super::*;
use crate::{schema::Source, test_support::DummyUser};

/// An enum node over the discriminant column `kind`, one variant per value,
/// built as `EmbeddedBuilder::enumeration` builds it from the app schema.
fn enumeration(values: &[&str]) -> EmbeddedBuilder {
    let variants: Vec<(String, String)> = values
        .iter()
        .map(|value| (value.to_string(), format!("Variant {value}")))
        .collect();
    EmbeddedBuilder {
        resolver: FieldResolver::default(),
        fields: vec![Field::discriminant("kind".to_string(), variants.clone())],
        shape: Shape::Enum(EnumNode {
            key: "kind".to_string(),
            discriminant: 0,
            shared: Vec::new(),
            variants: variants
                .into_iter()
                .map(|(value, _)| Variant {
                    value,
                    members: Vec::new(),
                })
                .collect(),
        }),
        variant: None,
    }
}

/// Variant "1" holds `email`, variant "2" holds `id`, and `name` is a shared
/// column both declare.
fn two_variants() -> Schema {
    let mut builder = enumeration(&["1", "2"]);
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()).optional());
    builder.leaf(Field::text(DummyUser::fields().email()).email());
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()).optional());
    builder.leaf(Field::text(DummyUser::fields().id()));
    builder.finish()
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A variant group the submission's discriminant does not name is
/// the one `variant.js` hides, so its fields cannot fail the submit. The
/// named variant's fields still validate, and a submission that names no
/// variant hides nothing — the payload fallback may still read any group.
#[test]
fn a_hidden_variant_group_is_not_validated() {
    let schema = Schema::new(two_variants());
    let values = map(&[
        ("kind", "2"),
        ("email", "not-an-email"),
        ("id", "0f8fad5b-d9cb-469f-a165-70867728950e"),
    ]);
    let errors = schema.validate(&values);
    assert!(
        !errors.contains_key("email"),
        "a hidden variant's field must not block the submit, got {errors:?}"
    );

    let mut named = values.clone();
    named.insert("kind".to_string(), "1".to_string());
    assert!(
        schema.validate(&named).contains_key("email"),
        "the named variant's field must still validate"
    );

    let mut unnamed = values.clone();
    unnamed.insert("kind".to_string(), String::new());
    assert!(
        schema.validate(&unnamed).contains_key("email"),
        "an unnamed submission validates every variant's fields"
    );
}

/// The key space a submission renders: the discriminant, a shared column, and
/// the named variant's leaves. A hidden variant's leaf renders nowhere.
#[test]
fn a_hidden_variants_leaf_is_not_an_error_key() {
    let schema = Schema::new(two_variants());
    let named = map(&[("kind", "1")]);
    assert!(
        schema.renders_error_key(&named, "kind") && schema.renders_error_key(&named, "name"),
        "the discriminant and a shared column render under every variant"
    );
    assert!(
        schema.renders_error_key(&named, "email"),
        "the named variant's leaf renders"
    );
    assert!(
        !schema.renders_error_key(&named, "id"),
        "the other variant's leaf renders nowhere"
    );
    let unnamed = map(&[("kind", "")]);
    assert!(
        schema.renders_error_key(&unnamed, "email") && schema.renders_error_key(&unnamed, "id"),
        "a submission naming no variant hides nothing"
    );
    assert!(
        !schema.renders_error_key(&named, "bogus"),
        "a key no field carries renders nowhere"
    );
}

/// A shared column is one field and one key, whichever variants declare it.
#[test]
fn a_shared_column_is_one_field_and_one_key() {
    let schema = two_variants();
    assert_eq!(
        schema.fields().map(Field::name).collect::<Vec<_>>(),
        ["kind", "name", "email", "id"]
    );
    assert_eq!(
        schema.embedded_root().keys(),
        ["kind", "name", "email", "id"],
        "the discriminant first, then every column once"
    );
}

/// A named discriminant wins; a missing one falls back to the first variant
/// with a payload of its own, which a shared column is not; an unknown one
/// is refused under the discriminant's key.
#[test]
fn the_variant_a_submission_reads_as() {
    let schema = two_variants();
    let node = schema.embedded_root();
    let index = |pairs: &[(&str, &str)]| node.variant_index(&map(pairs));
    assert_eq!(index(&[("kind", "2"), ("email", "a@b.co")]), Ok(1));
    assert_eq!(index(&[("id", "x")]), Ok(1), "the payload names variant 2");
    assert_eq!(
        index(&[("name", "shared")]),
        Ok(0),
        "a shared column selects no variant, so the first one reads"
    );
    let refused = index(&[("kind", "9")]).expect_err("an undeclared variant");
    assert_eq!(refused[0].key, "kind");
}

/// Joining a schema that already holds fields re-slots the node's fields, so
/// every control still renders under its own key.
#[test]
fn an_embedded_node_keeps_its_slots_when_appended() {
    let schema = Schema::new((Field::text(DummyUser::fields().email()), {
        let mut builder = enumeration(&["1"]);
        builder.variant();
        builder.leaf(Field::text(DummyUser::fields().name()));
        builder.finish()
    }));
    assert_eq!(
        schema.fields().map(Field::name).collect::<Vec<_>>(),
        ["email", "kind", "name"]
    );
    let mut slots = Vec::new();
    schema.nodes[1].visit_fields(&mut |index, _| slots.push(schema.fields[index].name()));
    assert_eq!(slots, ["kind", "name"], "the node reads its own fields");
}

/// A view renders a shared column only when the stored variant declares it:
/// a unit variant stores nothing in the column, so its key is absent from the
/// record's values and must not read as a missing field.
#[tokio::test]
async fn a_view_renders_a_shared_column_only_for_a_variant_declaring_it() {
    let cx = crate::test_support::cx();
    let mut builder = enumeration(&["1", "2", "3"]);
    builder.variant();
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()).label("Stamp"));
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()).label("Stamp"));
    let schema = Schema::new(builder.finish());
    let render = |values: HashMap<String, String>| {
        let schema = &schema;
        let cx = &cx;
        async move {
            schema
                .render(cx, Source::view(&values))
                .await
                .unwrap()
                .single()
                .await
                .unwrap()
                .render(cx)
        }
    };

    let unit = render(map(&[("kind", "1")])).await;
    assert!(
        unit.contains("Variant 1"),
        "the stored variant is named: {unit}"
    );
    assert!(
        !unit.contains("Stamp") && !unit.contains("(missing)"),
        "a unit variant declares no shared column: {unit}"
    );

    let stamped = render(map(&[("kind", "3"), ("name", "noon")])).await;
    assert!(
        stamped.contains("Stamp") && stamped.contains("noon"),
        "{stamped}"
    );
}
