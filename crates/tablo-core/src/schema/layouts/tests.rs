use std::collections::HashMap;

use super::*;
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
    test_support::{DummyUser, Html as _, cx},
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
async fn section_renders_title_and_child() {
    let cx = cx();
    let schema =
        Schema::new(Section::new("Account").schema(Field::text(DummyUser::fields().name())));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
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
        .html(&cx)
        .await;
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
        .html(&cx)
        .await;
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
