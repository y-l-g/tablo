//! Embedded lens resolution through the request's app schema.
//!
//! A plain lens binds a top-level field: it resolves against the owned
//! `app::Model`, which cannot see embedded models, so a path through an
//! embedded struct is rejected as a traversal lens. `ResolvedLens::new`
//! resolves through the request's app schema instead, so the leaf arrives as
//! its flattened storage column.
//!
//! This is the render-path proof: the flattened name is what the form posts
//! and what the unknown-key allow-list accepts, or a bound embedded field
//! would render blank and then be refused as an unknown key. The resolver walk
//! itself is covered by `schema::lenses`'s own tests; the two refusals at the
//! bottom drive the panicking entry a form binding uses, which that module
//! does not.

use std::collections::HashMap;

use tablo_core::{Field, FieldErrors, ResolvedLens, Schema, Source};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

#[derive(Debug, Clone, toasty::Embed)]
struct Seo {
    title: String,
    description: String,
}

#[derive(Debug, Clone, toasty::Embed)]
struct Meta {
    seo: Seo,
    note: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Author {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Article {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[index]
    title: String,
    meta: Meta,
    #[index]
    author_id: uuid::Uuid,
    #[belongs_to(key = author_id, references = id)]
    author: toasty::Deferred<Author>,
}

/// A `Db` built from the article model — the app schema comes with it.
async fn article_cx() -> Cx {
    let db = toasty::Db::builder()
        .models(toasty::models!(Article, Author))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    CxTestBuilder::new().app_context(db).build()
}

async fn render(schema: &Schema, cx: &Cx, values: HashMap<String, String>) -> String {
    schema
        .render(cx, Source::form(&values, &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(cx)
}

#[tokio::test]
async fn embedded_leaf_resolves_to_its_flattened_column() {
    let cx = article_cx().await;
    // Two levels deep: Article.meta.seo.title -> meta_seo_title.
    let input = Field::text(ResolvedLens::new(
        &tablo_core::DeclCx::from_cx(&cx),
        Article::fields().meta().seo().title(),
    ));
    assert_eq!(
        input.name(),
        "meta_seo_title",
        "an embedded leaf must resolve to its flattened storage column"
    );

    let mut values = HashMap::new();
    values.insert("meta_seo_title".to_string(), "Nested title".to_string());
    let html = render(&Schema::new(input), &cx, values).await;
    assert!(
        html.contains("value=\"Nested title\""),
        "the flattened value must hydrate into the control, got {html}"
    );
    assert!(
        html.contains("name=\"meta_seo_title\""),
        "the control must post the flattened column, got {html}"
    );
}

/// The allow-list and validation read the same name the control posts, so a
/// bound embedded field is neither rejected as unknown nor silently unvalidated.
#[tokio::test]
async fn the_flattened_name_participates_in_allow_list_and_validation() {
    let cx = article_cx().await;
    let schema = Schema::new((
        Field::text(ResolvedLens::new(
            &tablo_core::DeclCx::from_cx(&cx),
            Article::fields().title(),
        )),
        Field::text(ResolvedLens::new(
            &tablo_core::DeclCx::from_cx(&cx),
            Article::fields().meta().seo().title(),
        )),
    ));

    let mut values = HashMap::new();
    values.insert("title".to_string(), "Top".to_string());
    values.insert("meta_seo_title".to_string(), "Nested".to_string());
    assert!(
        schema.unknown_keys(&values).is_empty(),
        "declared embedded fields must be allow-listed, got {:?}",
        schema.unknown_keys(&values)
    );

    // An embedded leaf is never required by default (binding policy: the
    // resolver reports `nullable=true` even though a required embedded
    // struct's flattened column is `NOT NULL`): an absent value must not fail
    // the submit.
    let mut only_title = HashMap::new();
    only_title.insert("title".to_string(), "Top".to_string());
    assert!(
        schema.validate(&only_title).is_empty(),
        "an embedded leaf must not be required by default, got {:?}",
        schema.validate(&only_title)
    );

    // ...but a required one is still required when asked for explicitly.
    let required = Schema::new(
        Field::text(ResolvedLens::new(
            &tablo_core::DeclCx::from_cx(&cx),
            Article::fields().meta().seo().description(),
        ))
        .required(),
    );
    assert!(
        required
            .validate(&HashMap::new())
            .contains_key("meta_seo_description"),
        "an explicitly required embedded leaf must validate presence"
    );
}

/// A traversal lens over a relation is not an embedded step, and this walk is
/// for embedded binding only. It is refused rather than bound to anything —
/// `author_id` and `name` are different columns, so a silent misbind here would
/// write the wrong one. No `lenses.rs` test reaches this branch: its
/// refusals come from the missing/foreign-root model check, not from a
/// relation hop in the walk.
#[tokio::test]
async fn a_relation_traversal_is_refused_rather_than_misbound() {
    let cx = article_cx().await;
    let errors = Schema::new(Field::text(ResolvedLens::new(
        &tablo_core::DeclCx::from_cx(&cx),
        Article::fields().author().name(),
    )))
    .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("only embedded steps")),
        "{errors:?}"
    );
}

/// Without an app schema the single-segment rule still refuses a traversal
/// lens rather than bind the wrong column. `lenses.rs` covers `resolve_enum`
/// without a schema, which returns `None`; this pins the `resolve` entry a
/// form binding uses.
#[tokio::test]
async fn without_a_schema_a_traversal_lens_is_still_refused() {
    let errors = Schema::new(Field::text(ResolvedLens::new(
        &tablo_core::DeclCx::empty(),
        Article::fields().meta().note(),
    )))
    .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("single-field lens")),
        "{errors:?}"
    );
}

/// Every kind of field binds an embedded leaf through `ResolvedLens::new`,
/// and posts its flattened column.
#[tokio::test]
async fn a_choice_and_a_file_bind_an_embedded_leaf() {
    let cx = article_cx().await;
    let choice = Field::choice(ResolvedLens::new(
        &tablo_core::DeclCx::from_cx(&cx),
        Article::fields().meta().seo().title(),
    ))
    .options(vec!["draft".to_string()]);
    let file = Field::file(ResolvedLens::new(
        &tablo_core::DeclCx::from_cx(&cx),
        Article::fields().meta().seo().description(),
    ));
    assert_eq!(choice.name(), "meta_seo_title");
    assert_eq!(file.name(), "meta_seo_description");

    let schema = Schema::new((choice, file));
    let html = render(&schema, &cx, HashMap::new()).await;
    assert!(
        html.contains("<select") && html.contains("name=\"meta_seo_title\""),
        "the choice posts the flattened column, got {html}"
    );
    assert!(
        html.contains("type=\"file\"") && html.contains("name=\"meta_seo_description\""),
        "the file field posts the flattened column, got {html}"
    );
    assert!(
        schema.validate(&HashMap::new()).is_empty(),
        "an embedded leaf is optional by default, for every kind"
    );
}
