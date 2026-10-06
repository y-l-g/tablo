use std::collections::HashMap;

use super::*;
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
    test_support::{DummyUser, cx},
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

#[tokio::test]
async fn text_input_inside_section_and_grid() {
    let cx = cx();
    let schema = Schema::new(Section::new("Account").schema(Grid::new(2).schema((
        Field::text(DummyUser::fields().name()).required(),
        Field::text(DummyUser::fields().email()).email(),
    ))));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // No Tailwind-class assertions. What the layout has to prove
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
        Schema::new(Section::new("Account").schema(Field::text(DummyUser::fields().name())));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
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
    // The field sits in a content wrapper below its title, stated as nesting rather than as a
    // class.
    assert!(
        div_depth_of(&html, "data-slot=\"field\"") > div_depth_of(&html, "Account"),
        "the section's child must sit in a content wrapper below its title, got {html}"
    );
}

#[tokio::test]
async fn group_renders_children() {
    let cx = cx();
    let schema = Schema::new(
        Group::new().schema(Field::text(DummyUser::fields().name()).label("Inside group")),
    );
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // The layout owes the child it holds — once.
    assert!(html.contains("Inside group"), "missing child in {html}");
    assert_eq!(
        html.matches("data-slot=\"field\"").count(),
        1,
        "the group must render its one child, got {html}"
    );
}

#[tokio::test]
async fn grid_renders_with_cols_and_children() {
    // Production emits one static literal per count, so the class is asserted as a derived
    // `grid-cols-{cols}` over the whole table.
    let cx = cx();
    for cols in 1..=12u8 {
        let html = Schema::new(Grid::new(cols).schema((
            Field::text(DummyUser::fields().name()),
            Field::text(DummyUser::fields().email()),
        )))
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
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
async fn nested_grid_inside_section() {
    let cx = cx();
    let schema = Schema::new(Section::new("Outer").schema(Grid::new(2).schema((
        Field::text(DummyUser::fields().name()).label("Left"),
        Field::text(DummyUser::fields().email()).label("Right"),
    ))));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
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
