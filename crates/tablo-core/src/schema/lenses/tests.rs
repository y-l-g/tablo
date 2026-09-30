use toasty::schema::Model;
use topcoat::context::CxTestBuilder;

use super::*;

#[derive(Debug, toasty::Model)]
struct DummyUser {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[unique]
    email: String,
}

#[test]
fn single_segment_lens_passes_traversal_panics() {
    use toasty::schema::Model;
    let single = toasty_core::stmt::Path::field(DummyUser::id(), 0);
    require_single_segment(&single, "lens");
    let mut two = toasty_core::stmt::Path::field(DummyUser::id(), 0);
    two.chain(&toasty_core::stmt::Path::field(DummyUser::id(), 1));
    let result = std::panic::catch_unwind(|| require_single_segment(&two, "lens"));
    assert!(result.is_err(), "traversal lens must panic, not misbind");
}

/// The lens resolves to the field it names, labels it, and reports the
/// nullability the required-default reads.
#[test]
fn lens_field_resolves_name_label_and_nullability() {
    use toasty::schema::Model;
    let model = DummyUser::schema();

    let email = lens_field(DummyUser::fields().email(), &model);
    assert_eq!(email.name.app_unwrap(), "email");
    assert_eq!(lens_label(&email), "Email");
    assert!(!email.nullable());
    assert_eq!(email.name.storage_name(), Some("email"));

    let name = lens_field(DummyUser::fields().name(), &model);
    assert_eq!(name.name.app_unwrap(), "name");
    assert_eq!(lens_label(&name), "Name");
}

/// `#[unique]` lives on the model's index list, not the field, so
/// uniqueness is a separate lookup — and a bare field is not unique.
#[test]
fn lens_field_unique_reads_the_model_index_list() {
    use toasty::schema::Model;
    let model = DummyUser::schema();
    let root = model.as_root_unwrap();

    let email = lens_field(DummyUser::fields().email(), &model);
    assert!(
        lens_field_unique(&email, root),
        "#[unique] on email must surface as a single-field unique index"
    );

    let name = lens_field(DummyUser::fields().name(), &model);
    assert!(
        !lens_field_unique(&name, root),
        "a field with no unique index must not report unique"
    );

    let id = lens_field(DummyUser::fields().id(), &model);
    assert!(
        !lens_field_unique(&id, root),
        "the primary key is unique by construction, not by declared constraint"
    );
}

// === the resolver walk ==================================================
//
// `FieldResolver` reads the compiled schema the request carries, so these
// tests build a `Db` over one model carrying every shape the walk branches
// on, and drive the walk through its two production entries.

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

/// A second root model for the identity guard. Its fields are ordered so
/// that a lookup trusting the *id* would land on a different column than
/// the lens names: `LensPost.seo` is index 2, `Impostor.wrapper` is index 2.
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
    let db = toasty::Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    CxTestBuilder::new().app_context(db).build()
}

/// The context `LensPost`'s lenses resolve against.
async fn lens_cx() -> Cx {
    cx_with(toasty::models!(LensPost)).await
}

/// A model set whose root is `Impostor` wearing `LensPost`'s id.
///
/// Two Rust model types never share a `ModelId` in one process, so the one
/// way an id can name a foreign model is an assembled set — which is what
/// the guard exists for: it trusts the root's **name**, never the id.
fn models_with_a_foreign_root() -> toasty::schema::ModelSet {
    let mut set = toasty::schema::ModelSet::new();
    let mut model = Impostor::schema();
    let toasty_core::schema::app::Model::Root(root) = &mut model else {
        panic!("a #[derive(Model)] type builds a root model");
    };
    let id = <LensPost as toasty::schema::Model>::id();
    root.id = id;
    // A field's own `FieldId` names its model too, so the forgery has to
    // carry through or the root would be inconsistent with its fields.
    for field in &mut root.fields {
        field.id.model = id;
    }
    set.add(model);
    // The forged root embeds both, so their models must be in the set too.
    <Seo as toasty::schema::Field>::register(&mut set);
    <Wrapper as toasty::schema::Field>::register(&mut set);
    set
}

/// A plain leaf: a single segment on a model root never enters the walk, so
/// the owned app field answers name, label and nullability.
#[tokio::test]
async fn a_plain_leaf_resolves_through_the_app_field() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::from_cx(&cx).resolve(LensPost::fields().title());
    assert_eq!(leaf.name, "title");
    assert_eq!(leaf.label, "Title");
    assert!(
        !leaf.nullable,
        "a required top-level column is not storage-nullable"
    );
}

/// One embedded step: the walk follows it and names the flattened column.
#[tokio::test]
async fn a_leaf_in_an_embedded_struct_resolves_to_its_flattened_column() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::from_cx(&cx).resolve(LensPost::fields().seo().title());
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

/// A variant-rooted path through nested structs, three levels deep, landing
/// on one flat column.
#[tokio::test]
async fn a_leaf_in_a_struct_nested_in_an_enum_variant_resolves_to_one_column() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::from_cx(&cx);

    // media.image().url() — the other variant, one level down.
    assert_eq!(
        resolver
            .resolve(LensPost::fields().media().image().url())
            .name,
        "media_url"
    );
    // media.video().poster().url() — a struct one level inside the variant.
    assert_eq!(
        resolver
            .resolve(LensPost::fields().media().video().poster().url())
            .name,
        "media_poster_url"
    );
    // media.video().poster().credit().author() — three levels, one column.
    let leaf = resolver.resolve(
        LensPost::fields()
            .media()
            .video()
            .poster()
            .credit()
            .author(),
    );
    assert_eq!(leaf.name, "media_poster_credit_author");
    assert_eq!(leaf.label, "Media poster credit author");
    assert!(leaf.nullable);
}

/// A variant-rooted path whose *parent* walks through an embedded struct:
/// the payload accessor rebases onto the variant, so the parent path has
/// two steps and both halves of the walk have to follow them to reach the
/// enum — the app side to find its payload list, the mapping side to find
/// its per-variant columns.
#[tokio::test]
async fn a_variant_rooted_path_through_an_embedded_struct_resolves() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::from_cx(&cx);
    assert_eq!(
        resolver
            .resolve(LensPost::fields().wrapper().inner().image().url())
            .name,
        "wrapper_inner_url"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().wrapper().inner().video().video_url())
            .name,
        "wrapper_inner_video_url"
    );
}

/// `#[shared(timestamp)]`: both variants' leaves name the one column the
/// identifier declares, and a non-shared payload keeps its own.
#[tokio::test]
async fn a_shared_column_resolves_for_every_variant_that_declares_it() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::from_cx(&cx);
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().scheduled().scheduled_at())
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().published().published_at())
            .name,
        "publication_timestamp"
    );
    assert_eq!(
        resolver
            .resolve(LensPost::fields().publication().published().canonical_url())
            .name,
        "publication_canonical_url"
    );
}

/// A `#[document]`: its inner fields share its one column, so the walk
/// stops at the document however many steps remain.
#[tokio::test]
async fn a_document_leaf_resolves_to_the_document_column() {
    let cx = lens_cx().await;
    let leaf = FieldResolver::from_cx(&cx).resolve(LensPost::fields().stats().word_count());
    assert_eq!(leaf.name, "stats");
    assert_eq!(leaf.label, "Stats");
    assert!(
        leaf.nullable,
        "a document leaf is never required by default either"
    );
}

/// The identity guard, on the id lookup: a schema that does not carry the
/// lens's root model at all resolves to nothing.
#[tokio::test]
async fn a_root_model_the_schema_does_not_carry_resolves_to_nothing() {
    let cx = cx_with(toasty::models!(Impostor)).await;
    let resolver = FieldResolver::from_cx(&cx);
    assert!(resolver.has_schema(), "the Db carries the app schema");
    assert!(
        request_schema(&cx)
            .expect("the Db carries the app schema")
            .app
            .get_model(<LensPost as Model>::id())
            .is_none(),
        "this schema carries no model under the lens's id"
    );
    assert!(
        resolver
            .resolve_embedded_value(LensPost::fields().seo().into())
            .is_none(),
        "a value cannot resolve against a schema that has no LensPost"
    );
}

/// ... and through `resolve` the same path panics instead of quietly
/// resolving to nothing (the policy).
#[tokio::test]
#[should_panic(expected = "does not resolve to a single column")]
async fn a_root_model_the_schema_does_not_carry_refuses_a_leaf_lens() {
    let cx = cx_with(toasty::models!(Impostor)).await;
    let _ = FieldResolver::from_cx(&cx).resolve(LensPost::fields().seo().title());
}

/// The identity guard, on the name: an id that *is* in the schema but names
/// another model resolves to nothing, even though the impostor's field at
/// that index would have answered with a column (`wrapper_label`).
#[tokio::test]
async fn an_id_that_names_another_model_resolves_to_nothing() {
    let cx = cx_with(models_with_a_foreign_root()).await;
    let resolver = FieldResolver::from_cx(&cx);
    let schema = request_schema(&cx).expect("the Db carries the app schema");
    let root = schema
        .app
        .get_model(<LensPost as Model>::id())
        .expect("the forged root answers the lens's id")
        .as_root()
        .expect("a root model");
    assert_eq!(
        root.name.upper_camel_case(),
        "Impostor",
        "the id is the lens's, the name is another model's"
    );
    // What an id-trusting walk would bind: the forged root's field at the
    // lens's first step is `wrapper`, whose own leaf is another column.
    let mapping = schema
        .mapping
        .models
        .get(&root.id)
        .expect("the forged root is mapped");
    assert_eq!(
        descend(schema, &root.fields, &mapping.fields, &[2, 0])
            .expect("the forged root's field 2 has a leaf")
            .name,
        "wrapper_label",
        "only the name check keeps this column from being bound for `seo.title`"
    );
    assert!(
        resolver
            .resolve_embedded_value(LensPost::fields().seo().into())
            .is_none(),
        "the root at this id is Impostor, so no index may be trusted"
    );
}

/// ... and through `resolve` it panics rather than misbinding.
#[tokio::test]
#[should_panic(expected = "does not resolve to a single column")]
async fn an_id_that_names_another_model_refuses_a_leaf_lens() {
    let cx = cx_with(models_with_a_foreign_root()).await;
    let _ = FieldResolver::from_cx(&cx).resolve(LensPost::fields().seo().title());
}

/// A struct value: its leaf columns in declaration order, and no
/// discriminant.
#[tokio::test]
async fn an_embedded_struct_value_lists_its_leaf_columns() {
    let cx = lens_cx().await;
    let spec = FieldResolver::from_cx(&cx)
        .resolve_embedded_value(LensPost::fields().seo().into())
        .expect("an embedded struct is a value");
    assert_eq!(spec.columns, ["seo_title", "seo_description"]);
    assert!(
        spec.enum_spec.is_none(),
        "a struct has no variant to choose"
    );
}

/// An enum value: the discriminant column first, then each variant's
/// payload — the `#[shared]` column once, because it is one column.
#[tokio::test]
async fn an_embedded_enum_value_lists_its_discriminant_and_every_variant() {
    let cx = lens_cx().await;
    let spec = FieldResolver::from_cx(&cx)
        .resolve_embedded_value(LensPost::fields().publication().into())
        .expect("an embedded enum is a value");
    assert_eq!(
        spec.columns,
        [
            "publication",
            "publication_timestamp",
            "publication_scheduled_for",
            "publication_canonical_url",
        ],
        "the shared column appears once, and the discriminant is a key too"
    );
    let enum_spec = spec.enum_spec.expect("an enum has a variant to choose");
    assert_eq!(enum_spec.discriminant(), "publication");
    assert_eq!(enum_spec.len(), 2);
    assert_eq!(enum_spec.value_of_index(0), Some("1"));
    assert_eq!(enum_spec.name_of_index(1), Some("Published"));
}

/// A struct holding an enum: the nested enum's discriminant is a key of the
/// value too, while the value itself stays a struct.
#[tokio::test]
async fn a_struct_value_lists_the_discriminant_of_an_enum_it_holds() {
    let cx = lens_cx().await;
    let spec = FieldResolver::from_cx(&cx)
        .resolve_embedded_value(LensPost::fields().wrapper().into())
        .expect("an embedded struct is a value");
    assert_eq!(
        spec.columns,
        [
            "wrapper_label",
            "wrapper_inner",
            "wrapper_inner_url",
            "wrapper_inner_alt",
            "wrapper_inner_video_url",
            "wrapper_inner_poster_url",
            "wrapper_inner_poster_credit_author",
        ]
    );
    assert!(
        spec.enum_spec.is_none(),
        "the value at the path is the struct, not the enum it holds"
    );
}

/// A variant-rooted path names one variant, not the value: value binding
/// starts at the embedded field itself.
#[tokio::test]
async fn a_variant_rooted_path_is_not_an_embedded_value() {
    let cx = lens_cx().await;
    assert!(
        FieldResolver::from_cx(&cx)
            .resolve_embedded_value(LensPost::fields().media().video().video_url())
            .is_none()
    );
}

/// A plain column and a `#[document]` are leaves, not values: neither has a
/// per-field surface a codec could bind.
#[tokio::test]
async fn a_leaf_is_not_an_embedded_value() {
    let cx = lens_cx().await;
    let resolver = FieldResolver::from_cx(&cx);
    assert!(
        resolver
            .resolve_embedded_value(LensPost::fields().title())
            .is_none(),
        "a primitive field is a leaf"
    );
    assert!(
        resolver
            .resolve_embedded_value(LensPost::fields().stats().into())
            .is_none(),
        "a #[document] stores as one column, so it has no per-field value"
    );
}

/// Without a `Db` there is no schema, and a value binding has no fallback.
#[test]
fn without_a_schema_there_is_no_walk() {
    let cx = CxTestBuilder::new().build();
    let resolver = FieldResolver::from_cx(&cx);
    assert!(!resolver.has_schema(), "a bare Cx carries no Db");
    assert!(
        resolver
            .resolve_embedded_value(LensPost::fields().seo().into())
            .is_none(),
        "a value binding needs the app schema"
    );
}
