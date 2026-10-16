use super::*;
use crate::test_support::tableless_db;

// One model carries every shape the walk reads.

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

/// The embedded enum at `path`.
fn enum_at<M: toasty::schema::Model, T>(path: impl Into<Path<M, T>>) -> Option<EnumShape> {
    resolve_enum(&path.into())
}

/// The leaf `path` names, as a declaration binding it would resolve it.
fn leaf<M: toasty::schema::Model, T>(path: Path<M, T>) -> Result<LeafField, DeclarationErrorKind> {
    resolve::<M>(&ModelPath::of(&path))
}

/// A plain leaf: a single segment on a model root never enters the walk, so
/// the owned app field answers name, label and nullability.
#[test]
fn a_plain_leaf_resolves_through_the_app_field() {
    let leaf = leaf(LensPost::fields().title()).unwrap();
    assert_eq!(leaf.name, "title");
    assert_eq!(leaf.label, "Title");
    assert!(
        !leaf.nullable,
        "a required top-level column is not storage-nullable"
    );
}

/// Resolves one embedded step to its flattened column.
#[test]
fn a_leaf_in_an_embedded_struct_resolves_to_its_flattened_column() {
    let leaf = leaf(LensPost::fields().seo().title()).unwrap();
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
#[test]
fn a_leaf_in_a_struct_nested_in_an_enum_variant_resolves_to_one_column() {
    // media.image().url() — the other variant, one level down.
    assert_eq!(
        leaf(LensPost::fields().media().image().url()).unwrap().name,
        "media_url"
    );
    // media.video().poster().url() — a struct one level inside the variant.
    assert_eq!(
        leaf(LensPost::fields().media().video().poster().url())
            .unwrap()
            .name,
        "media_poster_url"
    );
    // media.video().poster().credit().author() — three levels, one column.
    let leaf = leaf(
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
#[test]
fn a_variant_rooted_path_through_an_embedded_struct_resolves() {
    assert_eq!(
        leaf(LensPost::fields().wrapper().inner().image().url())
            .unwrap()
            .name,
        "wrapper_inner_url"
    );
    assert_eq!(
        leaf(LensPost::fields().wrapper().inner().video().video_url())
            .unwrap()
            .name,
        "wrapper_inner_video_url"
    );
}

/// Resolves a shared column for every variant that declares it.
#[test]
fn a_shared_column_resolves_for_every_variant_that_declares_it() {
    assert_eq!(
        leaf(LensPost::fields().publication().scheduled().scheduled_at())
            .unwrap()
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        leaf(LensPost::fields().publication().published().published_at())
            .unwrap()
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        leaf(LensPost::fields().publication().published().canonical_url())
            .unwrap()
            .name,
        "publication_canonical_url"
    );
}

/// Resolves a document leaf to the document column.
#[test]
fn a_document_leaf_resolves_to_the_document_column() {
    let leaf = leaf(LensPost::fields().stats().word_count()).unwrap();
    assert_eq!(leaf.name, "stats");
    assert_eq!(leaf.label, "Stats");
    assert!(
        leaf.nullable,
        "a document leaf is never required by default either"
    );
}

/// Resolves an embedded enum's discriminant and variants in declaration order.
#[test]
fn an_embedded_enum_resolves_its_discriminant_and_variants() {
    let shape = enum_at(LensPost::fields().publication()).expect("an embedded enum resolves");
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
#[test]
fn an_enum_inside_a_struct_resolves_through_its_path() {
    let shape = enum_at(LensPost::fields().wrapper().inner()).expect("the nested enum resolves");
    assert_eq!(shape.discriminant, "wrapper_inner");
}

/// Resolves anything but an enum to nothing.
#[test]
fn anything_but_an_enum_resolves_to_nothing() {
    assert!(
        enum_at(LensPost::fields().seo()).is_none(),
        "a struct has no variant"
    );
    assert!(
        enum_at(LensPost::fields().title()).is_none(),
        "a primitive field is a leaf"
    );
    assert!(
        enum_at(LensPost::fields().stats()).is_none(),
        "a #[document] stores as one column"
    );
    assert!(
        enum_at(LensPost::fields().media().video().video_url()).is_none(),
        "a variant-rooted path names one variant, not the value"
    );
}

/// The same embedded type under another model resolves to that model's columns: a path's root
/// model, not the value's type, picks the schema it walks.
#[test]
fn an_embedded_value_under_another_model_resolves_against_that_model() {
    assert_eq!(
        leaf(Impostor::fields().seo().title()).unwrap().name,
        "seo_title"
    );
    assert_eq!(
        leaf(Impostor::fields().wrapper().inner().image().url())
            .unwrap()
            .name,
        "wrapper_inner_url"
    );
    assert_eq!(
        enum_at(Impostor::fields().wrapper().inner())
            .expect("the nested enum resolves")
            .discriminant,
        "wrapper_inner"
    );
}

/// A lens binds, without a `Db`, to the column the app's own `Db` stores it in: the compiled
/// schema of the model alone names every column as a `Db` registering it does.
#[tokio::test]
async fn a_lens_binds_to_the_column_the_db_stores_it_in() {
    let db = tableless_db(toasty::models!(LensPost)).await;
    let columns = |schema: &toasty_core::Schema| {
        schema
            .db
            .tables
            .iter()
            .flat_map(|table| table.columns.iter().map(|column| column.name.clone()))
            .collect::<Vec<_>>()
    };
    let compiled = AppSchema::of_model::<LensPost>().expect("the model compiles");
    assert_eq!(columns(db.schema()), columns(compiled.inner()));
}
