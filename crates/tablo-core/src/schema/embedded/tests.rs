use super::*;
use crate::{
    test_support::{DummyUser, Html as _},
    values::FormValues,
};

/// An enum node over the discriminant column `kind`, one variant per value,
/// built as `EmbeddedBuilder::enumeration` builds it from the app schema.
fn enumeration(values: &[&str]) -> EmbeddedBuilder {
    let variants: Vec<(String, String)> = values
        .iter()
        .map(|value| (value.to_string(), format!("Variant {value}")))
        .collect();
    EmbeddedBuilder {
        fields: vec![Field::discriminant(
            Binding::named("kind".to_string(), "Kind".to_string()),
            variants.clone(),
        )],
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
        refused: false,
    }
}

/// Variant "1" holds `email`, variant "2" holds `id`, and `name` is a shared
/// column both declare.
fn two_variants() -> Schema {
    let mut builder = enumeration(&["1", "2"]);
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()), false);
    builder.leaf(Field::text(DummyUser::fields().email()).email(), true);
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()), false);
    builder.leaf(Field::text(DummyUser::fields().id()), true);
    builder.finish()
}

fn map(pairs: &[(&str, &str)]) -> FormValues {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A variant group the submission's discriminant does not name is the one the page hides, so
/// its controls' rules cannot fail the submit. The named variant's controls still check, and a
/// submission that names no variant hides nothing: the payload fallback may still read any group.
#[tokio::test]
async fn a_hidden_variant_group_is_not_checked() {
    let cx = crate::test_support::cx();
    let schema = Schema::new(two_variants());
    let values = map(&[
        ("kind", "2"),
        ("email", "not-an-email"),
        ("id", "0f8fad5b-d9cb-469f-a165-70867728950e"),
    ]);
    let errors = schema.checked(&cx, &values).await;
    assert!(
        !errors.contains_key("email"),
        "a hidden variant's field must not block the submit, got {errors:?}"
    );

    let mut named = values.clone();
    named.insert("kind".to_string(), "1".to_string());
    assert!(
        schema.checked(&cx, &named).await.contains_key("email"),
        "the named variant's field must still check"
    );

    let mut unnamed = values.clone();
    unnamed.insert("kind".to_string(), String::new());
    assert!(
        schema.checked(&cx, &unnamed).await.contains_key("email"),
        "an unnamed submission checks every variant's fields"
    );
}

/// A submission hides the leaves of every variant it does not name; the discriminant and a shared
/// column render under every variant.
#[test]
fn a_submission_hides_the_variants_it_does_not_name() {
    let schema = Schema::new(two_variants());
    let hidden = |pairs: &[(&str, &str)]| {
        let mut keys: Vec<String> = schema.hidden_fields(&map(pairs)).into_iter().collect();
        keys.sort();
        keys
    };
    assert_eq!(hidden(&[("kind", "1")]), ["id"]);
    assert_eq!(hidden(&[("kind", "2")]), ["email"]);
    assert!(
        hidden(&[("kind", "")]).is_empty(),
        "a submission naming no variant hides nothing"
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
        builder.leaf(Field::text(DummyUser::fields().name()), false);
        builder.finish()
    }));
    assert_eq!(
        schema.fields().map(Field::name).collect::<Vec<_>>(),
        ["email", "kind", "name"]
    );
    let mut slots = Vec::new();
    let Node::Embedded(node) = &schema.nodes[1] else {
        panic!("the second node is the embedded value");
    };
    node.visit_fields(&mut |index| slots.push(schema.fields[index].name()));
    assert_eq!(slots, ["kind", "name"], "the node reads its own fields");
}

/// A shared column a variant does not declare shows only while the discriminant names a variant
/// that does — or names none at all, where the codec reads the variant the payload names: the
/// browser disables it otherwise, and the parse drops its key there.
#[tokio::test]
async fn a_shared_column_shows_for_the_variants_that_declare_it() {
    let cx = crate::test_support::cx();
    let mut builder = enumeration(&["1", "2", "3"]);
    builder.variant();
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()), true);
    builder.variant();
    builder.shared(Field::text(DummyUser::fields().name()), true);
    let schema = Schema::new(builder.finish());

    assert!(
        schema.declaration_errors().is_empty(),
        "a shared column the codec skips where it is hidden needs no blank answer: {:?}",
        schema.declaration_errors()
    );
    assert_eq!(
        schema.condition_hidden(&map(&[("kind", "1")])),
        ["name"],
        "a variant that declares no shared column hides it"
    );
    assert!(
        schema.condition_hidden(&map(&[("kind", "2")])).is_empty(),
        "a variant that declares it shows it"
    );
    assert!(
        schema
            .condition_hidden(&map(&[("name", "noon")]))
            .is_empty(),
        "a submission naming no variant keeps the column its payload may name"
    );
    assert!(
        schema.watched().contains_key("kind"),
        "the discriminant drives the shared column's condition"
    );

    let render = |kind: &'static str| {
        let schema = &schema;
        let cx = &cx;
        async move {
            let values = map(&[("kind", kind)]);
            schema
                .render(
                    cx,
                    Source::form(&values, &crate::values::FieldErrors::new()),
                )
                .await
                .html(cx)
                .await
        }
    };
    for (kind, disabled) in [("1", true), ("2", false)] {
        let html = render(kind).await;
        let input = html
            .find("name=\"name\"")
            .unwrap_or_else(|| panic!("no shared control in {html}"));
        let start = html[..input].rfind("<fieldset").expect("its fieldset");
        let fieldset = &html[start..input];
        let tag = &fieldset[..fieldset.find('>').expect("a closed tag")];
        assert_eq!(
            tag.contains(" disabled"),
            disabled,
            "kind={kind}: the shared column's fieldset is disabled exactly when the variant does \
             not declare it, got {tag}"
        );
        assert_eq!(tag.contains(" hidden"), disabled, "kind={kind}, got {tag}");
    }
}

/// A read-only display renders a shared column only when the stored variant declares it: a unit
/// variant stores nothing in the column, so its key is absent from the record's values and must
/// not render as an empty field.
#[tokio::test]
async fn a_display_renders_a_shared_column_only_for_a_variant_declaring_it() {
    let cx = crate::test_support::cx();
    let mut builder = enumeration(&["1", "2", "3"]);
    builder.variant();
    builder.variant();
    builder.shared(
        Field::text(DummyUser::fields().name()).label("Stamp"),
        false,
    );
    builder.variant();
    builder.shared(
        Field::text(DummyUser::fields().name()).label("Stamp"),
        false,
    );
    let schema = Schema::new(builder.finish());
    let render = |values: FormValues| {
        let schema = &schema;
        let cx = &cx;
        async move {
            schema
                .embedded_root()
                .display(cx, &schema.fields, &values)
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
        !unit.contains("Stamp"),
        "a unit variant declares no shared column: {unit}"
    );

    let stamped = render(map(&[("kind", "3"), ("name", "noon")])).await;
    assert!(
        stamped.contains("Stamp") && stamped.contains("noon"),
        "{stamped}"
    );
}
