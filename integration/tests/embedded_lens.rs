//! Embedded lens resolution.
//!
//! A path through an embedded value binds the flattened storage column the
//! app's `Db` stores it in, as soon as its field is declared.
//!
//! This is the render-path proof: the flattened name is what the form posts
//! and what the unknown-key allow-list accepts, or a bound embedded field
//! would render blank and then be refused as an unknown key. The walk itself is
//! covered by `schema::lenses`'s own tests.

use tablo::{DeclarationErrorKind, Field, FieldErrors, FormValues, ResourceDef, Schema, Source};
use topcoat::{context::Cx, view::ViewExt};

use crate::common::{input_value, test_db};

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

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
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
    Cx::default().with(db)
}

fn names(schema: &Schema) -> Vec<&str> {
    schema.fields().map(Field::name).collect()
}

async fn render(schema: &Schema, cx: &Cx, values: FormValues) -> String {
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
    let schema = Schema::new(Field::text(Article::fields().meta().seo().title()));
    assert_eq!(
        names(&schema),
        ["meta_seo_title"],
        "an embedded leaf must resolve to its flattened storage column"
    );

    let mut values = FormValues::new();
    values.insert("meta_seo_title".to_string(), "Nested title".to_string());
    let html = render(&schema, &cx, values).await;
    assert_eq!(
        input_value(&html, "meta_seo_title").as_deref(),
        Some("Nested title"),
        "the flattened value must hydrate into the control posting the flattened column, got {html}"
    );
}

/// A traversal lens over a relation is not an embedded step, and this walk is
/// for embedded binding only. It is refused rather than bound to anything —
/// `author_id` and `name` are different columns, so a silent misbind here would
/// write the wrong one.
#[test]
fn a_relation_traversal_is_refused_rather_than_misbound() {
    let errors = Schema::new(Field::text(Article::fields().author().name())).declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::UnresolvedLens { .. })),
        "{errors:?}"
    );
}

/// Every kind of field binds an embedded leaf and posts its flattened column.
#[tokio::test]
async fn a_choice_and_a_file_bind_an_embedded_leaf() {
    let cx = article_cx().await;
    let schema = Schema::new((
        Field::choice(Article::fields().meta().seo().title()).options(vec!["draft".to_string()]),
        Field::file(Article::fields().meta().seo().description()),
    ));
    assert_eq!(names(&schema), ["meta_seo_title", "meta_seo_description"]);

    let html = render(&schema, &cx, FormValues::new()).await;
    assert!(
        html.contains("<select") && html.contains("name=\"meta_seo_title\""),
        "the choice posts the flattened column, got {html}"
    );
    assert!(
        html.contains("type=\"file\"") && html.contains("name=\"meta_seo_description\""),
        "the file field posts the flattened column, got {html}"
    );
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Page {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    seo: Seo,
}

struct PageResource;

impl tablo::Resource for PageResource {
    type Model = Page;
    type Form = tablo::NoForm<Page>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::ReadOnly)
            .table(tablo::Table::new((
                tablo::TextColumn::new(tablo::lens!(Page.title)),
                tablo::TextColumn::new(tablo::lens!(Page.seo.title))
                    .sortable()
                    .searchable(),
            )))
    }
}

/// A column binds an embedded leaf: it renders, sorts and searches by the flattened
/// column.
#[tokio::test]
async fn a_column_binds_an_embedded_leaf() {
    let mut db = test_db(toasty::models!(Page)).await;
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
    let router = crate::common::panel_router::<PageResource>(db);
    let list = |uri: &'static str| {
        let router = &router;
        async move { crate::common::body_string(crate::common::get(router, uri).await).await }
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

#[derive(Debug, Clone, toasty::Embed, tablo::EmbeddedForm)]
struct Byline {
    #[column("headline")]
    title: String,
    note: String,
}

#[derive(Debug, Clone, toasty::Embed, tablo::EmbeddedForm)]
enum Cover {
    #[column(variant = 1)]
    Image { url: String },
    #[column(variant = 2)]
    Video {
        #[column("clip")]
        video_url: String,
    },
}

#[derive(Debug, Clone, toasty::Model)]
struct Writer {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[has_many]
    stories: toasty::Deferred<Vec<Story>>,
}

#[derive(Debug, Clone, toasty::Model)]
#[table = "stories"]
struct Story {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[column("by")]
    byline: Byline,
    cover: Cover,
    #[index]
    writer_id: uuid::Uuid,
    #[belongs_to(key = writer_id, references = id)]
    writer: toasty::Deferred<Writer>,
}

/// A lens binds, before any `Db` exists, to the column the app's own `Db` stores it in, whatever
/// the table prefix, the other models it registers, and the renames on and inside the value.
#[tokio::test]
async fn a_lens_binds_to_the_column_the_db_stores_it_in() {
    let mut keys = names(&Schema::new((
        Field::text(Story::fields().byline().title()),
        Field::text(Story::fields().byline().note()),
    )))
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    keys.extend(
        Cover::form(Story::fields().cover())
            .fields()
            .map(|field| field.name().to_string()),
    );
    assert_eq!(
        keys,
        ["by_headline", "by_note", "cover", "cover_url", "cover_clip"]
    );

    let db = toasty::Db::builder()
        .table_name_prefix("app_")
        .models(toasty::models!(Author, Writer, Story))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    let table = db
        .schema()
        .db
        .tables
        .iter()
        .find(|table| table.name == "app_stories")
        .expect("the story table");
    for key in &keys {
        assert!(
            table.columns.iter().any(|column| &column.name == key),
            "the Db stores no `{key}` column"
        );
    }
}

#[derive(Debug, Clone, toasty::Model)]
struct Oversized {
    #[key]
    #[auto]
    id: uuid::Uuid,
    /// No driver takes a 2 GB `VARCHAR`, so the model compiles to no schema.
    #[column(type = varchar(2000000000))]
    body: String,
    cover: Cover,
}

/// An embedded enum of a model no driver compiles is refused at declaration, like its leaves,
/// rather than panicking when its form is built.
#[test]
fn an_uncompiled_enum_is_refused_not_a_panic() {
    let errors = Cover::form(Oversized::fields().cover()).declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::Uncompiled { .. })),
        "{errors:?}"
    );
}
