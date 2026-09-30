use std::collections::HashMap;

use super::*;
use crate::{
    schema::{Schema, TextInput},
    test_support::cx,
};

/// The `<div>` nesting depth at the first occurrence of `marker` in `html`,
/// the outermost `<div>` counting as 1.
fn div_depth_of(html: &str, marker: &str) -> usize {
    let at = html
        .find(marker)
        .unwrap_or_else(|| panic!("the rendered markup carries no {marker}: {html}"));
    let mut depth = 0usize;
    for tag in html[..at].split('<').skip(1) {
        if tag
            .strip_prefix("div")
            .is_some_and(|rest| rest.starts_with([' ', '>']))
        {
            depth += 1;
        } else if tag.starts_with("/div") {
            depth = depth.saturating_sub(1);
        }
    }
    depth
}

#[derive(Debug, toasty::Model)]
struct DummyUser {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[unique]
    email: String,
}

#[tokio::test]
async fn text_input_inside_section_and_grid() {
    let cx = cx();
    let schema = Schema::new(Section::new("Account").schema(Grid::new(2).schema((
        TextInput::r#for(DummyUser::fields().name()).required(),
        TextInput::r#for(DummyUser::fields().email()).email(),
    ))));
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // GH #216: no Tailwind-class assertions. What the layout has to prove
    // is structural: the section's title, then the field it wraps, once.
    assert!(html.contains("Account"), "missing section title in {html}");
    assert_eq!(
        html.matches("data-slot=\"field\"").count(),
        2,
        "the grid inside the section must render both fields, got {html}"
    );
    assert!(
        html.find("Account").expect("the title") < html.find("data-slot=\"field\"").unwrap(),
        "the field must sit inside the section's panel, got {html}"
    );
}

#[tokio::test]
async fn section_renders_title_and_child() {
    let cx = cx();
    let schema =
        Schema::new(Section::new("Account").schema(TextInput::r#for(DummyUser::fields().name())));
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Account"), "missing title in {html}");
    assert!(
        html.contains("name=\"name\""),
        "missing child field in {html}"
    );
    // GH #238: the field sits one `<div>` deeper than the title text, inside
    // `card_content` — the sibling of `card_header` that carries the gap, so
    // never a direct child of the card, where it would be flush against the
    // title. The title text's depth is 2 only because `card_title` renders
    // an `<h3>`; a `<div>` title would sit at the field's own depth and this
    // comparison would have to anchor on the header element instead. The
    // wrapper's gap is a class and class literals are not asserted
    // that the wrapper exists is structure, so it is stated as
    // nesting rather than as a class.
    assert!(
        div_depth_of(&html, "data-slot=\"field\"") > div_depth_of(&html, "Account"),
        "the section's child must sit in a content wrapper below its title, got {html}"
    );
}

#[tokio::test]
async fn group_renders_children() {
    let cx = cx();
    let schema = Schema::new(
        Group::new().schema(TextInput::r#for(DummyUser::fields().name()).label("Inside group")),
    );
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // GH #216: the `field_group` wrapper's only observable is its utility
    // class, and that is the showcase's business (#136). What the layout
    // owes is the child it holds — once.
    assert!(html.contains("Inside group"), "missing child in {html}");
    assert_eq!(
        html.matches("data-slot=\"field\"").count(),
        1,
        "the group must render its one child, got {html}"
    );
}

#[tokio::test]
async fn grid_renders_with_cols_and_children() {
    // The declared column count is the caller's value, and production emits
    // one static literal per count (Tailwind only sees literal substrings —
    // see `Grid::render_source`), so the class is its *only* transport. It
    // is asserted as a derived `grid-cols-{cols}` over the whole table
    // rather than as one pinned literal per caller: the mapping
    // stays covered, and the other fourteen class literals this test used
    // to pin are gone.
    let cx = cx();
    for cols in 1..=12u8 {
        let html = Schema::new(Grid::new(cols).schema((
            TextInput::r#for(DummyUser::fields().name()),
            TextInput::r#for(DummyUser::fields().email()),
        )))
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
        assert!(
            html.contains(&format!("grid-cols-{cols}")),
            "Grid::new({cols}) must lay out {cols} columns, got {html}"
        );
        assert!(
            html.contains("name=\"name\"") && html.contains("name=\"email\""),
            "Grid::new({cols}) must render both children, got {html}"
        );
    }
}

#[tokio::test]
async fn tabs_render_children_in_one_container() {
    let cx = cx();
    let schema = Schema::new(
        Tabs::new().schema(TextInput::r#for(DummyUser::fields().name()).label("Tabbed")),
    );
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Tabbed"), "missing child in {html}");
    // "One shared container" without naming its classes: the
    // rendering is a single root `<div>`. The bug this guards is a second
    // wrapper around the same children, which would open a second root.
    let opens = |tag: &str| {
        tag.strip_prefix("div")
            .is_some_and(|rest| rest.starts_with([' ', '>']))
    };
    let mut depth = 0usize;
    let mut roots = 0usize;
    for tag in html.split('<').skip(1) {
        if opens(tag) {
            if depth == 0 {
                roots += 1;
            }
            depth += 1;
        } else if tag.starts_with("/div") {
            depth = depth.saturating_sub(1);
        }
    }
    assert_eq!(
        roots, 1,
        "the tabs container must be the only root wrapper, got {html}"
    );
}

#[tokio::test]
async fn tabs_validate_and_render_fields_end_to_end() {
    // GH #136 extension: the container had no end-to-end coverage — the
    // only tabs test was the UI demo `?tab=`, and the showcase wires no
    // layout block as a Schema container here. This pins that required
    // inputs inside the container validate and render with values.
    let cx = cx();
    let schema =
        Schema::new(Tabs::new().schema(TextInput::r#for(DummyUser::fields().name()).required()));
    let errors = schema.validate(&HashMap::new());
    assert!(
        errors.contains_key("name"),
        "empty submit must fail the inner required input, got {errors:?}"
    );
    let mut values = HashMap::new();
    values.insert("name".to_string(), "Ada".to_string());
    let errors = schema.validate(&values);
    assert!(errors.is_empty(), "filled submit must pass, got {errors:?}");
    let html = schema
        .render_with(&cx, &values, &errors)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("value=\"Ada\""),
        "container must render the field value, got {html}"
    );
}

#[tokio::test]
async fn nested_grid_inside_section() {
    let cx = cx();
    let schema = Schema::new(Section::new("Outer").schema(Grid::new(2).schema((
        TextInput::r#for(DummyUser::fields().name()).label("Left"),
        TextInput::r#for(DummyUser::fields().email()).label("Right"),
    ))));
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Outer"), "missing outer title in {html}");
    assert!(html.contains("Left"), "missing left in {html}");
    assert!(html.contains("Right"), "missing right in {html}");
}

#[tokio::test]
async fn repeater_required_error_renders_inline() {
    let cx = cx();
    // Single-entry repeater: the required error is keyed by label
    // until repeaters become field-bound.
    let schema = Schema::new(
        Repeater::new("Tags")
            .required()
            .schema(TextInput::r#for(DummyUser::fields().name()).label("Tag")),
    );
    let values = HashMap::new();
    let errors = schema.validate(&values);
    assert!(
        errors.contains_key("Tags"),
        "required repeater must produce a label-keyed error, got {errors:?}"
    );
    let html = schema
        .render_with(&cx, &values, &errors)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("Tags is required"),
        "repeater error must reach the HTML, got {html}"
    );
    // Same inline error contract as TextInput, wired to the group: the
    // panel carries the invalid state and describes itself with the
    // error node's id. The title's colour is paint, not state:
    // these three state hooks are what a regression would break.
    assert!(
        html.contains("data-invalid=\"true\"")
            && html.contains("aria-invalid=\"true\"")
            && html.contains("aria-describedby=\"tags-error\""),
        "repeater must expose its invalid state in {html}"
    );
    assert!(
        html.contains("id=\"tags-error\"") && html.contains("ac-error"),
        "missing inline error slot in {html}"
    );
    assert!(
        html.contains("aria-live=\"polite\""),
        "missing aria-live in {html}"
    );
    // Non-empty inner value clears the error.
    let mut filled = HashMap::new();
    filled.insert("name".to_string(), "rust".to_string());
    let errors = schema.validate(&filled);
    assert!(
        !errors.contains_key("Tags"),
        "filled repeater must pass, got {errors:?}"
    );
    // A valid group carries no invalid state and no error node.
    let valid_html = schema
        .render_with(&cx, &filled, &errors)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !valid_html.contains("data-invalid")
            && !valid_html.contains("role=\"alert\"")
            && !valid_html.contains("tags-error"),
        "a valid repeater must not render invalid state in {valid_html}"
    );
}

#[test]
fn repeater_error_ids_slug_the_label() {
    // Ids cannot carry the label's whitespace (GH #78 keys the error by
    // label until repeaters are field-bound).
    assert_eq!(repeater_error_id("Tags"), "tags-error");
    assert_eq!(
        repeater_error_id("Shipping Address"),
        "shipping-address-error"
    );
    assert_eq!(
        repeater_error_id("  Billing / Info  "),
        "billing-info-error"
    );
}

/// An optional Repeater with a `required` inner input must not fail an
/// empty submit: group-empty means "absent". A `required`
/// repeater answers an empty submit with exactly one label-keyed error,
/// and a partially filled optional group still enforces inner `required`.
#[test]
fn optional_repeater_with_required_inner_allows_empty_group() {
    // Two inner inputs so "partially filled" is expressible: a required
    // text field and an optional email field.
    let optional = Schema::new(
        Repeater::new("Tags").schema((
            TextInput::r#for(DummyUser::fields().name())
                .required()
                .label("Tag"),
            TextInput::r#for(DummyUser::fields().email())
                .optional()
                .label("Note"),
        )),
    );
    let required = Schema::new(
        Repeater::new("Tags").required().schema((
            TextInput::r#for(DummyUser::fields().name())
                .required()
                .label("Tag"),
            TextInput::r#for(DummyUser::fields().email())
                .optional()
                .label("Note"),
        )),
    );

    // Empty submit: the optional group validates clean...
    let errors = optional.validate(&HashMap::new());
    assert!(
        errors.is_empty(),
        "optional repeater with empty group must validate clean, got {errors:?}"
    );
    // ...the required group answers with exactly one label-keyed error —
    // the inner input's own required error is suppressed with the absent
    // group, so the label carries the whole story.
    let errors = required.validate(&HashMap::new());
    assert_eq!(
        errors.len(),
        1,
        "required repeater + empty submit must yield one error, got {errors:?}"
    );
    assert_eq!(
        errors.get("Tags").map(|errs| errs.as_slice()),
        Some(["Tags is required".to_string()].as_slice()),
        "the label-keyed error is the only one, got {errors:?}"
    );

    // A partially filled optional group counts as present: inner
    // `required` fires for the empty input, not for the optional one.
    let mut partial = HashMap::new();
    partial.insert("email".to_string(), "a@b.c".to_string());
    let errors = optional.validate(&partial);
    assert!(
        errors.contains_key("name"),
        "a partially filled group enforces inner required, got {errors:?}"
    );
    assert!(
        !errors.contains_key("email"),
        "the optional inner input stays optional, got {errors:?}"
    );
}

/// A `required` repeater nested inside an all-empty OPTIONAL group is
/// suppressed with it: an untouched outer group means nothing
/// inside it was intended, so the inner label error must not fire.
#[test]
fn required_repeater_inside_absent_optional_group_is_suppressed() {
    let schema = Schema::new(
        Repeater::new("Outer").schema((
            TextInput::r#for(DummyUser::fields().name()).required(),
            Repeater::new("Inner")
                .required()
                .schema(TextInput::r#for(DummyUser::fields().email()).required()),
        )),
    );
    // Empty submit: the outer group is absent, so the inner required
    // repeater fires no error at all.
    let errors = schema.validate(&HashMap::new());
    assert!(
        errors.is_empty(),
        "an untouched optional outer group must suppress nested required repeaters, got {errors:?}"
    );
    // With the outer group present (a value anywhere in its subtree),
    // the inner required repeater enforces.
    let mut present = HashMap::new();
    present.insert("name".to_string(), "rust".to_string());
    let errors = schema.validate(&present);
    assert!(
        errors.contains_key("Inner"),
        "a present outer group enforces the inner required repeater, got {errors:?}"
    );
}
