use topcoat::context::CxTestBuilder;

use super::*;
use crate::test_support::tableless_db;

// `FieldResolver` reads the compiled schema the request carries, so these tests build a `Db` over
// one model carrying every shape the resolver reads.

/// An embedded struct: its leaves flatten into the parent's table.
#[derive(Debug, Clone, toasty::Embed)]
struct Seo {
    title: String,
    description: String,
}

/// The deepest level of `media_poster_credit_author`.
#[derive(Debug, Clone, toasty::Embed)]
struct Credit {
    author: String,
}

#[derive(Debug, Clone, toasty::Embed)]
struct Poster {
    url: String,
    credit: Credit,
}

/// An embedded enum with a struct nested inside a variant: the path
/// `media().video().poster().credit().author()` is three levels deep and
/// still lands on one flat column.
#[derive(Debug, Clone, toasty::Embed)]
enum Media {
    #[column(variant = 1)]
    Image { url: String, alt: String },
    #[column(variant = 2)]
    Video { video_url: String, poster: Poster },
}

/// Two variants declaring one `#[shared(timestamp)]` column.
#[derive(Debug, Clone, toasty::Embed)]
enum Publication {
    #[column(variant = 1)]
    Scheduled {
        #[shared(timestamp)]
        scheduled_at: String,
        scheduled_for: String,
    },
    #[column(variant = 2)]
    Published {
        #[shared(timestamp)]
        published_at: String,
        canonical_url: String,
    },
}

/// A struct holding an enum: the nested discriminant is a key of the value
/// too, not only its payloads.
#[derive(Debug, Clone, toasty::Embed)]
struct Wrapper {
    label: String,
    inner: Media,
}

/// A `#[document]`: a primitive whose storage is a model, so its inner
/// fields share one column named after the field.
#[derive(Debug, Clone, toasty::Embed)]
struct Stats {
    word_count: i64,
    read_minutes: i64,
}

#[derive(Debug, Clone, toasty::Model)]
struct LensPost {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    seo: Seo,
    media: Media,
    publication: Publication,
    wrapper: Wrapper,
    #[document]
    stats: Stats,
}

/// A second root model, embedding what `LensPost` embeds.
#[derive(Debug, Clone, toasty::Model)]
struct Impostor {
    #[key]
    #[auto]
    id: uuid::Uuid,
    seo: Seo,
    wrapper: Wrapper,
}

/// The request context a resolver reads its schema from.
async fn cx_with(models: toasty::schema::ModelSet) -> Cx {
    let db = tableless_db(models).await;
    CxTestBuilder::new().app_context(db).build()
}

/// The context `LensPost`'s lenses resolve against.
async fn lens_cx() -> Cx {
    cx_with(toasty::models!(LensPost)).await
}

/// A plain leaf: a single segment on a model root never enters the walk, so
/// the owned app field answers name, label and nullability.
#[tokio::test]
async fn a_plain_leaf_resolves_through_the_app_field() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::of(&cx)
        .resolve(LensPost::fields().title())
        .unwrap();
    assert_eq!(leaf.name, "title");
    assert_eq!(leaf.label, "Title");
    assert!(
        !leaf.nullable,
        "a required top-level column is not storage-nullable"
    );
}

/// Resolves one embedded step to its flattened column.
#[tokio::test]
async fn a_leaf_in_an_embedded_struct_resolves_to_its_flattened_column() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::of(&cx)
        .resolve(LensPost::fields().seo().title())
        .unwrap();
    assert_eq!(leaf.name, "seo_title");
    assert_eq!(
        leaf.label, "Seo title",
        "a label reads the storage name as prose"
    );
    assert!(
        leaf.nullable,
        "an embedded leaf is never required by default"
    );
}

/// Resolves a variant-rooted path through nested structs to one flat column.
#[tokio::test]
async fn a_leaf_in_a_struct_nested_in_an_enum_variant_resolves_to_one_column() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::of(&cx);

    // media.image().url() — the other variant, one level down.
    assert_eq!(
        resolver
            .resolve(LensPost::fields().media().image().url())
            .unwrap()
            .name,
        "media_url"
    );
    // media.video().poster().url() — a struct one level inside the variant.
    assert_eq!(
        resolver
            .resolve(LensPost::fields().media().video().poster().url())
            .unwrap()
            .name,
        "media_poster_url"
    );
    // media.video().poster().credit().author() — three levels, one column.
    let leaf = resolver
        .resolve(
            LensPost::fields()
                .media()
                .video()
                .poster()
                .credit()
                .author(),
        )
        .unwrap();
    assert_eq!(leaf.name, "media_poster_credit_author");
    assert_eq!(leaf.label, "Media poster credit author");
    assert!(leaf.nullable);
}

/// Resolves a variant-rooted path through an embedded struct.
#[tokio::test]
async fn a_variant_rooted_path_through_an_embedded_struct_resolves() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::of(&cx);
    assert_eq!(
        resolver
            .resolve(LensPost::fields().wrapper().inner().image().url())
            .unwrap()
            .name,
        "wrapper_inner_url"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().wrapper().inner().video().video_url())
            .unwrap()
            .name,
        "wrapper_inner_video_url"
    );
}

/// Resolves a shared column for every variant that declares it.
#[tokio::test]
async fn a_shared_column_resolves_for_every_variant_that_declares_it() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::of(&cx);
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().scheduled().scheduled_at())
            .unwrap()
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().published().published_at())
            .unwrap()
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().published().canonical_url())
            .unwrap()
            .name,
        "publication_canonical_url"
    );
}

/// Resolves a document leaf to the document column.
#[tokio::test]
async fn a_document_leaf_resolves_to_the_document_column() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::of(&cx)
        .resolve(LensPost::fields().stats().word_count())
        .unwrap();
    assert_eq!(leaf.name, "stats");
    assert_eq!(leaf.label, "Stats");
    assert!(
        leaf.nullable,
        "a document leaf is never required by default either"
    );
}

/// Resolves to nothing when the schema does not carry the lens's root model.
#[tokio::test]
async fn a_root_model_the_schema_does_not_carry_resolves_to_nothing() {
    let cx = cx_with(toasty::models!(Impostor)).await;
    let resolver = FieldResolver::of(&cx);
    assert!(resolver.has_schema(), "the Db carries the app schema");
    assert!(
        !AppSchema::of(&cx)
            .expect("the Db carries the app schema")
            .registers("LensPost"),
        "this schema carries no model under the lens's name"
    );
    assert!(
        resolver
            .resolve_enum(LensPost::fields().publication().into())
            .is_none(),
        "an enum cannot resolve against a schema that has no LensPost"
    );
}

/// Refuses a leaf lens the schema does not carry.
#[tokio::test]
async fn a_root_model_the_schema_does_not_carry_refuses_a_leaf_lens() {
    let cx = cx_with(toasty::models!(Impostor)).await;
    let error = FieldResolver::of(&cx)
        .resolve(LensPost::fields().seo().title())
        .expect_err("a lens the schema cannot bind is refused");
    assert!(
        matches!(error, DeclarationErrorKind::UnresolvedLens { .. }),
        "{error}"
    );
}

/// Resolves an embedded enum's discriminant and variants in declaration order.
#[tokio::test]
async fn an_embedded_enum_resolves_its_discriminant_and_variants() {
    let cx = lens_cx().await;
    let shape = FieldResolver::of(&cx)
        .resolve_enum(LensPost::fields().publication().into())
        .expect("an embedded enum resolves");
    assert_eq!(shape.discriminant, "publication");
    assert_eq!(
        shape.variants,
        [
            ("1".to_string(), "Scheduled".to_string()),
            ("2".to_string(), "Published".to_string()),
        ]
    );
}

/// Resolves an enum nested in a struct through the struct's path.
#[tokio::test]
async fn an_enum_inside_a_struct_resolves_through_its_path() {
    let cx = lens_cx().await;
    let shape = FieldResolver::of(&cx)
        .resolve_enum(LensPost::fields().wrapper().inner().into())
        .expect("the nested enum resolves");
    assert_eq!(shape.discriminant, "wrapper_inner");
}

/// Resolves anything but an enum to nothing.
#[tokio::test]
async fn anything_but_an_enum_resolves_to_nothing() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::of(&cx);
    assert!(
        resolver
            .resolve_enum(LensPost::fields().seo().into())
            .is_none(),
        "a struct has no variant"
    );
    assert!(
        resolver.resolve_enum(LensPost::fields().title()).is_none(),
        "a primitive field is a leaf"
    );
    assert!(
        resolver
            .resolve_enum(LensPost::fields().stats().into())
            .is_none(),
        "a #[document] stores as one column"
    );
    assert!(
        resolver
            .resolve_enum(LensPost::fields().media().video().video_url())
            .is_none(),
        "a variant-rooted path names one variant, not the value"
    );
}

/// Resolves to nothing without a schema.
#[test]
fn without_a_schema_there_is_no_walk() {
    let cx = CxTestBuilder::new().build();
    let resolver = FieldResolver::of(&cx);
    assert!(!resolver.has_schema(), "a bare Cx carries no Db");
    assert!(
        resolver
            .resolve_enum(LensPost::fields().publication().into())
            .is_none(),
        "an enum needs the app schema"
    );
}
