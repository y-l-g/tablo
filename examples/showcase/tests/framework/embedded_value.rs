//! Embedded values.

use std::collections::HashMap;

use tablo::{
    Detail, EmbeddedColumn, EmbeddedForm, Field, FieldErrorKind, FieldErrors, IntoSchema, NoForm,
    Resource, ResourceDef, Schema, Section, Source, Table, TextColumn, lens,
};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

fn mentions(schema: &Schema, values: &HashMap<String, String>) -> bool {
    schema
        .fields()
        .any(|field| values.contains_key(field.name()))
}

#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
struct Seo {
    title: String,
    description: String,
}

#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
struct Credit {
    author: String,
    licence: String,
}

#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
struct Poster {
    url: String,
    #[form(embed)]
    credit: Credit,
}

/// A shared column across three variants.
#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
enum Publication {
    #[column(variant = 1)]
    Scheduled {
        #[shared(timestamp)]
        scheduled_at: String,
        #[form(optional)]
        scheduled_for: String,
    },
    #[column(variant = 2)]
    Published {
        #[shared(timestamp)]
        published_at: String,
        canonical_url: String,
    },
    #[column(variant = 3)]
    Archived {
        #[shared(timestamp)]
        archived_at: String,
        reason: String,
    },
}

/// An enum carrying a nested struct inside a variant.
#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
enum Media {
    #[column(variant = 1)]
    Image { url: String, alt: String },
    #[column(variant = 2)]
    Video {
        video_url: String,
        #[form(embed)]
        poster: Poster,
    },
}

/// A unit variant carries no payload.
#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
enum Visibility {
    #[column(variant = 1)]
    Public,
    #[column(variant = 2)]
    Private { reason: String },
}

/// A struct holding an enum.
#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
struct Wrapper {
    label: String,
    #[form(embed)]
    inner: Media,
}

/// Variant idents the schema normalises.
#[derive(Debug, Clone, PartialEq, toasty::Embed, EmbeddedForm)]
enum Casing {
    #[column(variant = 1)]
    OK { at: String },
    #[column(variant = 2)]
    Draft,
}

/// The leaf types the panel spells.
#[derive(Debug, Clone, Default, PartialEq, toasty::Embed, EmbeddedForm)]
struct Flags {
    featured: bool,
    level: u8,
    revision: u32,
}

#[derive(Debug, Clone, Default, PartialEq, toasty::Embed, EmbeddedForm)]
struct PostStats {
    #[form(label = "Word count")]
    word_count: i64,
    #[form(blank = 0)]
    read_minutes: i64,
}

#[derive(Debug, Clone, toasty::Model)]
struct Post {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    seo: Seo,
    publication: Publication,
    media: Media,
    post_stats: PostStats,
    visibility: Visibility,
    wrapper: Wrapper,
    casing: Casing,
    flags: Flags,
}

async fn post_cx() -> Cx {
    let db = toasty::Db::builder()
        .models(toasty::models!(Post))
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

fn first_name(schema: &Schema) -> &str {
    schema.fields().next().expect("one field").name()
}

fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The framework names every key.
#[tokio::test]
async fn keys_come_from_the_compiled_mapping() {
    let cx = post_cx().await;

    assert_eq!(
        first_name(&bound(&cx, Field::text(Post::fields().seo().title()))),
        "seo_title",
        "an embedded struct's leaf is its flattened column"
    );
    assert_eq!(
        first_name(&bound(
            &cx,
            Field::text(Post::fields().post_stats().word_count())
        )),
        "post_stats_word_count"
    );

    // The enum's variant control comes first, on the discriminant column
    // named after the field; then the shared column, once, and each
    // variant's own payload.
    let publication = bound(&cx, Publication::form(Post::fields().publication()));
    assert_eq!(
        publication.fields().map(Field::name).collect::<Vec<_>>(),
        [
            "publication",
            "publication_timestamp",
            "publication_scheduled_for",
            "publication_canonical_url",
            "publication_reason",
        ]
    );

    // A value knows which keys are its own.
    assert!(mentions(
        &bound(&cx, Publication::form(Post::fields().publication())),
        &map(&[("publication", "1")])
    ));
    assert!(mentions(
        &bound(&cx, Publication::form(Post::fields().publication())),
        &map(&[("publication_timestamp", "t")])
    ));
    assert!(mentions(
        &bound(&cx, Publication::form(Post::fields().publication())),
        &map(&[("publication_canonical_url", "/x")])
    ));
}

/// A struct round-trips through the flat map.
#[tokio::test]
async fn a_struct_round_trips_through_the_flat_map() {
    let cx = post_cx().await;
    let seo = Seo {
        title: "Hello".to_string(),
        description: "World".to_string(),
    };

    let mut values = HashMap::new();
    seo.write_form(&cx, Post::fields().seo(), &mut values);
    assert_eq!(
        values,
        map(&[("seo_title", "Hello"), ("seo_description", "World")]),
        "a struct writes exactly its leaves"
    );

    let read: Seo =
        EmbeddedForm::read_form(&cx, Post::fields().seo(), &values).expect("the value reads");
    assert_eq!(read, seo);
}

/// An enum round-trips with an explicit discriminant.
#[tokio::test]
async fn an_enum_round_trips_with_an_explicit_discriminant() {
    let cx = post_cx().await;
    let published = Publication::Published {
        published_at: "2026-09-22T00:00:00Z".to_string(),
        canonical_url: "/hello".to_string(),
    };

    let mut values = HashMap::new();
    published.write_form(&cx, Post::fields().publication(), &mut values);
    assert_eq!(
        values,
        map(&[
            ("publication", "2"),
            ("publication_timestamp", "2026-09-22T00:00:00Z"),
            ("publication_canonical_url", "/hello"),
        ]),
        "an enum writes its discriminant and the active variant's leaves"
    );

    let read: Publication = EmbeddedForm::read_form(&cx, Post::fields().publication(), &values)
        .expect("the value reads");
    assert_eq!(read, published);

    // The discriminating case.
    let contradictory = map(&[
        ("publication", "3"),
        ("publication_timestamp", "2026-09-22T00:00:00Z"),
        ("publication_canonical_url", "/hello"),
        ("publication_reason", "superseded"),
    ]);
    let read: Publication =
        EmbeddedForm::read_form(&cx, Post::fields().publication(), &contradictory)
            .expect("the value reads");
    assert_eq!(
        read,
        Publication::Archived {
            archived_at: "2026-09-22T00:00:00Z".to_string(),
            reason: "superseded".to_string(),
        },
        "the discriminant decides the variant, not which payloads are non-empty"
    );
}

/// A missing discriminant infers the variant from its payload.
#[tokio::test]
async fn a_missing_discriminant_infers_the_variant_from_its_payload() {
    let cx = post_cx().await;

    // Filling the Published payload creates a Published value.
    let published = map(&[
        ("publication_timestamp", "2026-09-22T00:00:00Z"),
        ("publication_canonical_url", "/hello"),
    ]);
    let read: Publication = EmbeddedForm::read_form(&cx, Post::fields().publication(), &published)
        .expect("the value reads");
    assert_eq!(
        read,
        Publication::Published {
            published_at: "2026-09-22T00:00:00Z".to_string(),
            canonical_url: "/hello".to_string(),
        },
        "a submitted Published payload must infer Published"
    );

    let archived = map(&[
        ("publication_timestamp", "2026-09-22T00:00:00Z"),
        ("publication_reason", "superseded"),
    ]);
    let read: Publication = EmbeddedForm::read_form(&cx, Post::fields().publication(), &archived)
        .expect("the value reads");
    assert_eq!(
        read,
        Publication::Archived {
            archived_at: "2026-09-22T00:00:00Z".to_string(),
            reason: "superseded".to_string(),
        },
        "a submitted Archived payload must infer Archived"
    );

    // A shared payload infers nothing.
    let shared_only = map(&[("publication_timestamp", "2026-09-22T00:00:00Z")]);
    let read: Publication =
        EmbeddedForm::read_form(&cx, Post::fields().publication(), &shared_only)
            .expect("the value reads");
    assert_eq!(
        read,
        Publication::Scheduled {
            scheduled_at: "2026-09-22T00:00:00Z".to_string(),
            scheduled_for: String::new(),
        },
        "a shared column never selects a variant"
    );
}

/// An unknown discriminant is refused.
#[tokio::test]
async fn an_unknown_discriminant_is_refused() {
    let cx = post_cx().await;
    let values = map(&[
        ("publication", "99"),
        ("publication_canonical_url", "/hello"),
    ]);
    let errors = EmbeddedForm::read_form(&cx, Post::fields().publication(), &values)
        .expect_err("an undeclared variant is refused");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].key, "publication");
    assert!(matches!(errors[0].kind, FieldErrorKind::Invalid(_)));
}

/// Nested values delegate to their own codec.
#[tokio::test]
async fn nested_values_delegate_to_their_own_codec() {
    let cx = post_cx().await;
    let video = Media::Video {
        video_url: "/v.mp4".to_string(),
        poster: Poster {
            url: "/p.jpg".to_string(),
            credit: Credit {
                author: "Ada".to_string(),
                licence: "CC-BY".to_string(),
            },
        },
    };

    let mut values = HashMap::new();
    video.write_form(&cx, Post::fields().media(), &mut values);
    assert_eq!(
        values,
        map(&[
            ("media", "2"),
            ("media_video_url", "/v.mp4"),
            ("media_poster_url", "/p.jpg"),
            ("media_poster_credit_author", "Ada"),
            ("media_poster_credit_licence", "CC-BY"),
        ])
    );

    let read: Media =
        EmbeddedForm::read_form(&cx, Post::fields().media(), &values).expect("the value reads");
    assert_eq!(read, video);
}

/// A unit variant round-trips on its discriminant alone.
#[tokio::test]
async fn a_unit_variant_round_trips_on_its_discriminant_alone() {
    let cx = post_cx().await;

    let mut values = HashMap::new();
    Visibility::Public.write_form(&cx, Post::fields().visibility(), &mut values);
    assert_eq!(values, map(&[("visibility", "1")]));
    let read: Visibility = EmbeddedForm::read_form(&cx, Post::fields().visibility(), &values)
        .expect("the value reads");
    assert_eq!(read, Visibility::Public);

    let mut values = HashMap::new();
    Visibility::Private {
        reason: "draft".to_string(),
    }
    .write_form(&cx, Post::fields().visibility(), &mut values);
    assert_eq!(
        values,
        map(&[("visibility", "2"), ("visibility_reason", "draft")])
    );
    let read: Visibility = EmbeddedForm::read_form(&cx, Post::fields().visibility(), &values)
        .expect("the value reads");
    assert_eq!(
        read,
        Visibility::Private {
            reason: "draft".to_string()
        }
    );
}

/// A typed leaf keeps its own spelling rule.
#[tokio::test]
async fn typed_leaves_round_trip() {
    let cx = post_cx().await;
    let stats = PostStats {
        word_count: 1200,
        read_minutes: 6,
    };

    let mut values = HashMap::new();
    stats.write_form(&cx, Post::fields().post_stats(), &mut values);
    assert_eq!(
        values,
        map(&[
            ("post_stats_word_count", "1200"),
            ("post_stats_read_minutes", "6"),
        ])
    );
    let read: PostStats = EmbeddedForm::read_form(&cx, Post::fields().post_stats(), &values)
        .expect("the value reads");
    assert_eq!(read, stats);
}

/// A blank leaf takes its own answer, or is refused on its key.
#[tokio::test]
async fn a_blank_leaf_takes_its_answer_or_is_refused() {
    let cx = post_cx().await;

    // `read_minutes` declares `#[form(blank = 0)]`; `word_count` declares none.
    let declared = map(&[
        ("post_stats_word_count", "1200"),
        ("post_stats_read_minutes", "  "),
    ]);
    let read: PostStats = EmbeddedForm::read_form(&cx, Post::fields().post_stats(), &declared)
        .expect("the value reads");
    assert_eq!(
        read,
        PostStats {
            word_count: 1200,
            read_minutes: 0,
        },
        "a declared blank answers itself"
    );

    let empty = map(&[
        ("post_stats_word_count", ""),
        ("post_stats_read_minutes", ""),
    ]);
    let errors = EmbeddedForm::read_form(&cx, Post::fields().post_stats(), &empty)
        .expect_err("a leaf with no blank answer is refused");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].key, "post_stats_word_count");
    assert_eq!(errors[0].kind, FieldErrorKind::Required);
}

/// An unparseable typed leaf is refused.
#[tokio::test]
async fn an_unparseable_typed_leaf_is_refused() {
    let cx = post_cx().await;
    let values = map(&[("post_stats_word_count", "many")]);
    let errors = EmbeddedForm::read_form(&cx, Post::fields().post_stats(), &values)
        .expect_err("an unparseable leaf is refused");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].key, "post_stats_word_count");
    assert_eq!(
        errors[0].kind,
        FieldErrorKind::Invalid("`many` is not a valid whole number".to_string())
    );
}

/// Value keys name every key of a value.
#[tokio::test]
async fn value_keys_name_every_key_of_a_value() {
    let cx = post_cx().await;
    assert!(mentions(
        &bound(&cx, Seo::form(Post::fields().seo())),
        &map(&[("seo_title", "x")])
    ));
    assert!(!mentions(
        &bound(&cx, Seo::form(Post::fields().seo())),
        &map(&[("title", "x")])
    ));
    // The discriminant counts: a form that only posts the variant mentioned it.
    assert!(mentions(
        &bound(&cx, Publication::form(Post::fields().publication())),
        &map(&[("publication", "1")])
    ));
}

/// A nested enum contributes its discriminant.
#[tokio::test]
async fn a_nested_enum_contributes_its_discriminant() {
    let cx = post_cx().await;

    // The nested enum's discriminant is a key of the value: naming only it
    // mentions the wrapper.
    assert!(
        mentions(
            &bound(&cx, Wrapper::form(Post::fields().wrapper())),
            &map(&[("wrapper_inner", "1")])
        ),
        "naming only the nested variant mentions the value"
    );
    assert!(
        !mentions(
            &bound(&cx, Wrapper::form(Post::fields().wrapper())),
            &map(&[("title", "x")])
        ),
        "a key outside the value does not mention it"
    );

    let wrapper = Wrapper {
        label: "w".to_string(),
        inner: Media::Image {
            url: "/i.jpg".to_string(),
            alt: "i".to_string(),
        },
    };
    let mut values = HashMap::new();
    wrapper.write_form(&cx, Post::fields().wrapper(), &mut values);
    assert_eq!(
        values,
        map(&[
            ("wrapper_label", "w"),
            ("wrapper_inner", "1"),
            ("wrapper_inner_url", "/i.jpg"),
            ("wrapper_inner_alt", "i"),
        ])
    );
    let read: Wrapper =
        EmbeddedForm::read_form(&cx, Post::fields().wrapper(), &values).expect("the value reads");
    assert_eq!(read, wrapper);
}

/// Variant casing needs no normalisation.
#[tokio::test]
async fn variant_casing_needs_no_normalisation() {
    let cx = post_cx().await;

    for casing in [
        Casing::OK {
            at: "now".to_string(),
        },
        Casing::Draft,
    ] {
        let mut values = HashMap::new();
        casing.write_form(&cx, Post::fields().casing(), &mut values);
        let read: Casing = EmbeddedForm::read_form(&cx, Post::fields().casing(), &values)
            .expect("the value reads");
        assert_eq!(read, casing, "wrote {values:?}");
    }

    // And the derived form renders (a name mismatch would panic here).
    let html = Schema::new(bound(&cx, Casing::form(Post::fields().casing())))
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("name=\"casing\""), "got {html}");
    assert!(html.contains("name=\"casing_at\""), "got {html}");
    // The label is the normalised name.
    assert_eq!(
        variant_option_labels(&html),
        vec!["-- Select --", "Ok", "Draft"],
        "got {html}"
    );
}

/// Typed leaves cover bool and the integer family.
#[tokio::test]
async fn typed_leaves_cover_bool_and_the_integer_family() {
    let cx = post_cx().await;
    let flags = Flags {
        featured: true,
        level: 3,
        revision: 7,
    };

    let mut values = HashMap::new();
    flags.write_form(&cx, Post::fields().flags(), &mut values);
    assert_eq!(
        values,
        map(&[
            ("flags_featured", "true"),
            ("flags_level", "3"),
            ("flags_revision", "7"),
        ])
    );
    let read: Flags =
        EmbeddedForm::read_form(&cx, Post::fields().flags(), &values).expect("the value reads");
    assert_eq!(read, flags);

    // A bad `bool` is refused before a record fn runs.
    let bad = map(&[("flags_featured", "yes")]);
    let errors = <Flags as EmbeddedForm>::read_form(&cx, Post::fields().flags(), &bad)
        .expect_err("the leaf's type refuses `yes`");
    assert!(
        errors.iter().any(|error| error.key == "flags_featured"),
        "the derived leaf parses its own type: {errors:?}"
    );
}

/// The derived form renders the variant select and every payload.
#[tokio::test]
async fn the_derived_form_renders_the_variant_select_and_every_payload() {
    let cx = post_cx().await;
    let schema = Schema::new(bound(&cx, Publication::form(Post::fields().publication())));
    let mut values = HashMap::new();
    Publication::Archived {
        archived_at: "2026-09-22T00:00:00Z".to_string(),
        reason: "superseded".to_string(),
    }
    .write_form(&cx, Post::fields().publication(), &mut values);

    let html = render_form(&cx, &schema, &values).await;

    assert!(
        html.contains("<select") && html.contains("name=\"publication\""),
        "the discriminant must ride the form as a visible control, got {html}"
    );
    assert_eq!(
        html.matches("data-topcoat-bind:hidden").count(),
        3,
        "each variant group follows the chosen variant, got {html}"
    );
    assert!(
        html.contains("value=\"3\" selected"),
        "the stored variant hydrates as the selected option, got {html}"
    );
    for name in [
        "publication_timestamp",
        "publication_scheduled_for",
        "publication_canonical_url",
        "publication_reason",
    ] {
        assert!(html.contains(name), "missing control {name} in {html}");
    }
    assert!(
        html.contains(">Canonical url<"),
        "an unlabelled field is humanized from its name, got {html}"
    );
}

/// A detail page's read-only view of a post.
struct PostDetail;

impl Resource for PostDetail {
    type Model = Post;
    type Form = NoForm<Post>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("posts")
            .policy(tablo::ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Post.title))))
            .detail(Detail::new((
                Section::new("SEO").columns(EmbeddedColumn::new(lens!(Post.seo))),
                Section::new("Publication").columns(EmbeddedColumn::new(lens!(Post.publication))),
                EmbeddedColumn::new(lens!(Post.visibility)),
            )))
    }
}

/// A detail page shows an embedded struct leaf by leaf, and an embedded enum as its variant's name
/// and that variant's leaves alone, with no control.
#[tokio::test]
async fn a_detail_page_shows_an_embedded_value_leaf_by_leaf() {
    let mut db = crate::framework::common::memory_db(toasty::models!(Post)).await;
    let post = toasty::create!(Post {
        title: "Launch".to_string(),
        seo: Seo {
            title: "Launch day".to_string(),
            description: "All about it".to_string(),
        },
        publication: Publication::Archived {
            archived_at: "2026-09-22T00:00:00Z".to_string(),
            reason: "superseded".to_string(),
        },
        media: Media::Image {
            url: "/a.png".to_string(),
            alt: "A".to_string(),
        },
        post_stats: PostStats::default(),
        visibility: Visibility::Public,
        wrapper: Wrapper {
            label: "w".to_string(),
            inner: Media::Image {
                url: "/b.png".to_string(),
                alt: "B".to_string(),
            },
        },
        casing: Casing::Draft,
        flags: Flags::default(),
    })
    .exec(&mut db)
    .await
    .expect("seed post");
    let router = crate::framework::common::panel_router::<PostDetail>(db);
    let view = crate::framework::common::body_string(
        crate::framework::common::get(&router, &format!("/admin/posts/{}", post.id)).await,
    )
    .await;

    assert!(
        !view.contains("<select") && !view.contains("<input"),
        "a detail page renders no control, got {view}"
    );
    assert!(
        view.contains("Launch day") && view.contains("All about it"),
        "each leaf of a struct reads, got {view}"
    );
    assert!(
        view.contains(">Publication<") && view.contains(">Archived<"),
        "the detail page names the stored variant, got {view}"
    );
    assert!(
        !view.contains(">3<"),
        "the variant's name, never its discriminant, got {view}"
    );
    // Only the stored variant's payload reads: the other variants hold no values on this record.
    assert!(view.contains("superseded"), "got {view}");
    assert!(
        !view.contains("Canonical url") && !view.contains("Scheduled for"),
        "another variant's payload does not render, got {view}"
    );
    let visibility = &view[view.find(">Visibility<").expect("the visibility entry")..];
    assert!(
        visibility.contains(">Public<") && !visibility.contains(">Reason<"),
        "a unit variant reads as its name alone, got {view}"
    );
}

/// The form's HTML, hydrated with `values` (empty for a create form).
async fn render_form(cx: &Cx, schema: &Schema, values: &HashMap<String, String>) -> String {
    schema
        .render(cx, Source::form(values, &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(cx)
}

/// Every `data-variant="…"` value in `html`, in document order.
fn variant_markers(html: &str) -> Vec<String> {
    html.match_indices("data-variant=\"")
        .map(|(at, needle)| {
            let rest = &html[at + needle.len()..];
            rest[..rest.find('"').expect("a closed marker")].to_string()
        })
        .collect()
}

/// The variant control's own markup: the form's first `<select>`.
fn variant_select(html: &str) -> &str {
    html.split_once("<select")
        .and_then(|(_, rest)| rest.split_once("</select>").map(|(select, _)| select))
        .expect("a variant select")
}

/// The `value="…"` of every option in the first variant control of `html`, in
/// document order — the placeholder first, then one per variant.
fn variant_options(html: &str) -> Vec<String> {
    let select = variant_select(html);
    select
        .match_indices("value=\"")
        .map(|(at, needle)| {
            let rest = &select[at + needle.len()..];
            rest[..rest.find('"').expect("a closed value")].to_string()
        })
        .collect()
}

/// The text of every option in the first variant control of `html`, in
/// document order — what a person reads, the placeholder included.
fn variant_option_labels(html: &str) -> Vec<String> {
    let select = variant_select(html);
    select
        .match_indices("<option")
        .map(|(at, _)| {
            let option = &select[at..];
            let text = option.find('>').expect("a closed option tag") + 1;
            let end = option[text..].find("</option>").expect("a closed option");
            option[text..text + end].to_string()
        })
        .collect()
}

/// The variant groups are exactly the schema's variants.
#[tokio::test]
async fn the_variant_groups_are_exactly_the_schemas_variants() {
    let cx = post_cx().await;
    let html = render_form(
        &cx,
        &Schema::new(bound(&cx, Publication::form(Post::fields().publication()))),
        &HashMap::new(),
    )
    .await;

    // `Publication` stores 1, 2, and 3.
    let expected = ["1", "2", "3"];
    assert_eq!(
        variant_markers(&html),
        expected,
        "one group per variant, in declaration order, got {html}"
    );
    let mut offered = variant_options(&html);
    assert_eq!(
        offered.first().map(String::as_str),
        Some(""),
        "the control opens on the empty choice — the create form has no stored \
         variant — so the marker values follow it, got {html}"
    );
    offered.remove(0);
    assert_eq!(
        offered, expected,
        "the control must offer every variant, and only those, in declaration \
         order: the marker a group carries is the option that shows it, got {html}"
    );

    let expected_names = ["Scheduled", "Published", "Archived"];
    let mut labels = variant_option_labels(&html);
    assert_eq!(
        labels.first().map(String::as_str),
        Some("-- Select --"),
        "the empty choice reads as prose too, got {html}"
    );
    labels.remove(0);
    assert_eq!(
        labels, expected_names,
        "every option reads as its variant's name, got {html}"
    );
}

/// Only the chosen variant's controls are enabled: a required control in a hidden group would
/// still fail the browser's validation and block the submit.
#[tokio::test]
async fn only_the_chosen_variants_controls_are_enabled() {
    let cx = post_cx().await;
    let html = render_form(
        &cx,
        &Schema::new(bound(&cx, Publication::form(Post::fields().publication()))),
        &map(&[("publication", "2")]),
    )
    .await;

    for marker in variant_markers(&html) {
        let group = html
            .find(&format!("data-variant=\"{marker}\""))
            .expect("the group");
        let fieldset = &html[html[..group].rfind("<fieldset").expect("a fieldset")..group];
        let disabled = fieldset.contains(" disabled");
        assert_eq!(
            disabled,
            marker != "2",
            "variant {marker}'s group must be disabled exactly when not chosen, got {html}"
        );
    }
}

/// A unit variant still gets its group.
#[tokio::test]
async fn a_unit_variant_still_gets_its_group() {
    let cx = post_cx().await;
    let html = render_form(
        &cx,
        &Schema::new(bound(&cx, Visibility::form(Post::fields().visibility()))),
        &HashMap::new(),
    )
    .await;
    assert_eq!(variant_markers(&html), vec!["1", "2"], "got {html}");
    assert_eq!(
        variant_option_labels(&html),
        vec!["-- Select --", "Public", "Private"],
        "a unit variant is still named in the chooser, got {html}"
    );
}

/// Each variant's payload sits in its own group.
#[tokio::test]
async fn each_variants_payload_sits_in_its_own_group() {
    let cx = post_cx().await;
    let html = render_form(
        &cx,
        &Schema::new(bound(&cx, Publication::form(Post::fields().publication()))),
        &HashMap::new(),
    )
    .await;

    let group_at = |marker: &str| {
        html.find(&format!("data-variant=\"{marker}\""))
            .unwrap_or_else(|| panic!("no group for variant {marker} in {html}"))
    };
    let bounds = [
        ("1", "publication_scheduled_for"),
        ("2", "publication_canonical_url"),
        ("3", "publication_reason"),
    ];
    for (index, (marker, leaf)) in bounds.iter().enumerate() {
        let start = group_at(marker);
        let end = bounds
            .get(index + 1)
            .map(|(next, _)| group_at(next))
            .unwrap_or(html.len());
        let leaf_at = html
            .find(leaf)
            .unwrap_or_else(|| panic!("no {leaf} in {html}"));
        assert!(
            start < leaf_at && leaf_at < end,
            "{leaf} must render inside variant {marker}'s group, got {html}"
        );
    }

    let shared_at = html
        .find("publication_timestamp")
        .expect("the shared column");
    assert!(
        shared_at < group_at("1"),
        "a shared column belongs to every variant, so it renders outside the \
         groups, got {html}"
    );
}
