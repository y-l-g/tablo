//! Embedded lens resolution through the request's app schema.
//!
//! A path through an embedded value names a flattened storage column only the
//! app schema knows, so it stays unbound until its schema binds (`Schema::bind`,
//! or a panel mount), and the leaf then arrives as its flattened storage column.
//!
//! This is the render-path proof: the flattened name is what the form posts
//! and what the unknown-key allow-list accepts, or a bound embedded field
//! would render blank and then be refused as an unknown key. The resolver walk
//! itself is covered by `schema::lenses`'s own tests; the two refusals at the
//! bottom drive the panicking entry a form binding uses, which that module
//! does not.

use std::collections::HashMap;

use tablo_core::{
    DeclarationErrorKind, Field, FieldErrors, IntoSchema, ResourceDef, Schema, Source,
};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

use crate::framework::common::input_value;

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

/// `children` as a schema bound to the app schema of the `Db` `cx` carries, as a panel mount binds
/// it.
fn bound(cx: &Cx, children: impl IntoSchema) -> Schema {
    let db = topcoat::context::try_app_context::<toasty::Db>(cx).expect("the cx carries a Db");
    Schema::new(children).bind(db)
}

fn names(schema: &Schema) -> Vec<&str> {
    schema.fields().map(Field::name).collect()
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
    let schema = bound(&cx, Field::text(Article::fields().meta().seo().title()));
    assert_eq!(
        names(&schema),
        ["meta_seo_title"],
        "an embedded leaf must resolve to its flattened storage column"
    );

    let mut values = HashMap::new();
    values.insert("meta_seo_title".to_string(), "Nested title".to_string());
    let html = render(&schema, &cx, values).await;
    assert_eq!(
        input_value(&html, "meta_seo_title").as_deref(),
        Some("Nested title"),
        "the flattened value must hydrate into the control posting the flattened column, got {html}"
    );
}

/// The allow-list and validation read the same name the control posts, so a
/// bound embedded field is neither rejected as unknown nor silently unvalidated.
#[tokio::test]
async fn the_flattened_name_participates_in_allow_list_and_validation() {
    let cx = article_cx().await;
    let schema = bound(
        &cx,
        (
            Field::text(Article::fields().title()),
            Field::text(Article::fields().meta().seo().title()),
        ),
    );

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
    let required = bound(
        &cx,
        Field::text(Article::fields().meta().seo().description()).required(),
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
    let errors = bound(&cx, Field::text(Article::fields().author().name())).declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::UnresolvedLens { .. })),
        "{errors:?}"
    );
}

/// An embedded path never bound to the app schema refuses to render rather than bind the wrong
/// column.
#[tokio::test]
async fn an_unbound_embedded_path_is_refused() {
    let cx = article_cx().await;
    let schema = Schema::new(Field::text(Article::fields().meta().note()));
    assert!(
        matches!(
            schema.declaration_errors().as_slice(),
            [DeclarationErrorKind::Unbound { .. }]
        ),
        "{:?}",
        schema.declaration_errors()
    );
    assert!(
        schema
            .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
            .await
            .is_err(),
        "an unbound schema fails to render"
    );
}

/// Every kind of field binds an embedded leaf and posts its flattened column.
#[tokio::test]
async fn a_choice_and_a_file_bind_an_embedded_leaf() {
    let cx = article_cx().await;
    let schema = bound(
        &cx,
        (
            Field::choice(Article::fields().meta().seo().title())
                .options(vec!["draft".to_string()]),
            Field::file(Article::fields().meta().seo().description()),
        ),
    );
    assert_eq!(names(&schema), ["meta_seo_title", "meta_seo_description"]);

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

#[derive(Debug, Clone, toasty::Model)]
struct Page {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    seo: Seo,
}

struct PageResource;

impl tablo_core::Resource for PageResource {
    type Model = Page;
    type Form = tablo_core::NoForm<Page>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo_core::ReadOnly)
            .table(tablo_core::Table::new((
                tablo_core::TextColumn::new(tablo_core::lens!(Page.title)),
                tablo_core::TextColumn::new(tablo_core::lens!(Page.seo.title))
                    .sortable()
                    .searchable(),
            )))
    }
}

/// A column binds an embedded leaf at mount: it renders, sorts and searches by the flattened
/// column.
#[tokio::test]
async fn a_column_binds_an_embedded_leaf() {
    let mut db = crate::framework::common::memory_db(toasty::models!(Page)).await;
    for (title, seo) in [("First", "Zulu"), ("Second", "Alpha")] {
        toasty::create!(Page {
            title: title.to_string(),
            seo: Seo {
                title: seo.to_string(),
                description: String::new(),
            },
        })
        .exec(&mut db)
        .await
        .expect("create");
    }
    let router = crate::framework::common::panel_router::<PageResource>(db);
    let list = |uri: &'static str| {
        let router = &router;
        async move {
            crate::framework::common::body_string(crate::framework::common::get(router, uri).await)
                .await
        }
    };

    let sorted = list("/admin/pages?sort=seo_title&dir=asc").await;
    assert!(sorted.contains("Seo title"), "{sorted}");
    let (alpha, zulu) = (sorted.find("Alpha").unwrap(), sorted.find("Zulu").unwrap());
    assert!(alpha < zulu, "sorted by the embedded leaf, got {sorted}");

    let searched = list("/admin/pages?q=zul").await;
    assert!(
        searched.contains("First") && !searched.contains("Second"),
        "searched by the embedded leaf, got {searched}"
    );
}
