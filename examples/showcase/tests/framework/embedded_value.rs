//! Embedded values.

use std::collections::HashMap;

use tablo_core::{EmbeddedForm, Field, FieldErrorKind, FieldErrors, Schema, Source};
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

/// Run `declarations` with the app schema of the `Db` `cx` carries in scope, as a panel mount does.
fn declared<T>(cx: &Cx, declarations: impl FnOnce() -> T) -> T {
    let db = topcoat::context::try_app_context::<toasty::Db>(cx).expect("the cx carries a Db");
    tablo_core::declare(db, declarations)
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
        declared(&cx, || Field::text(Post::fields().seo().title())).name(),
        "seo_title",
        "an embedded struct's leaf is its flattened column"
    );
    assert_eq!(
        declared(&cx, || Field::text(
            Post::fields().post_stats().word_count()
        ))
        .name(),
        "post_stats_word_count"
    );

    // The enum's variant control comes first, on the discriminant column
    // named after the field; then the shared column, once, and each
    // variant's own payload.
    let publication = declared(&cx, || Publication::form(Post::fields().publication()));
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
        &declared(&cx, || Publication::form(Post::fields().publication())),
        &map(&[("publication", "1")])
    ));
    assert!(mentions(
        &declared(&cx, || Publication::form(Post::fields().publication())),
        &map(&[("publication_timestamp", "t")])
    ));
    assert!(mentions(
        &declared(&cx, || Publication::form(Post::fields().publication())),
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
    assert_eq!(errors[0].kind, FieldErrorKind::Invalid);
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

/// A value answers a blank when every leaf does.
#[tokio::test]
async fn a_value_answers_a_blank_when_every_leaf_does() {
    assert!(Seo::answers_blank());
    assert!(
        Media::answers_blank(),
        "a nested value's leaves answer with their own"
    );
    assert!(
        !PostStats::answers_blank(),
        "a bare `i64` leaf answers none"
    );
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
    assert_eq!(errors[0].message, "`many` is not a valid whole number");
}

/// Value keys name every key of a value.
#[tokio::test]
async fn value_keys_name_every_key_of_a_value() {
    let cx = post_cx().await;
    assert!(mentions(
        &declared(&cx, || Seo::form(Post::fields().seo())),
        &map(&[("seo_title", "x")])
    ));
    assert!(!mentions(
        &declared(&cx, || Seo::form(Post::fields().seo())),
        &map(&[("title", "x")])
    ));
    // The discriminant counts: a form that only posts the variant mentioned it.
    assert!(mentions(
        &declared(&cx, || Publication::form(Post::fields().publication())),
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
            &declared(&cx, || Wrapper::form(Post::fields().wrapper())),
            &map(&[("wrapper_inner", "1")])
        ),
        "naming only the nested variant mentions the value"
    );
    assert!(
        !mentions(
            &declared(&cx, || Wrapper::form(Post::fields().wrapper())),
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
    let html = Schema::new(declared(&cx, || Casing::form(Post::fields().casing())))
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
    assert!(
        Schema::new(declared(&cx, || Flags::form(Post::fields().flags())))
            .validate(&bad)
            .contains_key("flags_featured"),
        "the derived control validates its own type"
    );
}

/// The derived form renders the variant select and every payload.
#[tokio::test]
async fn the_derived_form_renders_the_variant_select_and_every_payload() {
    let cx = post_cx().await;
    let schema = Schema::new(declared(&cx, || {
        Publication::form(Post::fields().publication())
    }));
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
    assert!(
        html.contains("data-variant-select=\"publication\""),
        "the control must name the groups it drives, got {html}"
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
        html.contains(">Canonical Url<"),
        "an unlabelled field is humanized from its name, got {html}"
    );

    // Read-only: no control renders.
    let view = render_view(&cx, &schema, &values).await;
    assert!(
        !view.contains("<select") && !view.contains("<input"),
        "a variant control must not render in view mode, got {view}"
    );
    assert!(
        view.contains(">Publication<") && view.contains(">Archived<"),
        "the view must name the stored variant, got {view}"
    );
    assert!(
        !view.contains(">3<"),
        "the view must print the variant's name, never its discriminant, got {view}"
    );
    // Only the stored variant's payload reads: the other variants hold no
    // values on this record.
    assert!(view.contains("superseded"), "got {view}");
    assert!(
        !view.contains("Canonical Url") && !view.contains("Scheduled for"),
        "another variant's payload must not render on the detail page, got {view}"
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

/// The form's read-only HTML, hydrated with `values`.
async fn render_view(cx: &Cx, schema: &Schema, values: &HashMap<String, String>) -> String {
    schema
        .render(cx, Source::view(values))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(cx)
}

/// Every `data-variant="…"` value in `html`, in document order.
///
/// The marker's own attribute — the `="` is what tells it from
/// `data-variant-of` and `data-variant-select`, whose prefixes it shares.
fn variant_markers(html: &str) -> Vec<String> {
    html.match_indices("data-variant=\"")
        .map(|(at, needle)| {
            let rest = &html[at + needle.len()..];
            rest[..rest.find('"').expect("a closed marker")].to_string()
        })
        .collect()
}

/// The variant control's own markup: from its hook to the first `</select>`.
fn variant_select(html: &str) -> &str {
    html.split_once("data-variant-select=")
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
        &Schema::new(declared(&cx, || {
            Publication::form(Post::fields().publication())
        })),
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
    assert_eq!(
        html.matches("data-variant-of=\"publication\"").count(),
        expected.len(),
        "every group must name the enum it belongs to, got {html}"
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

/// A unit variant still gets its group.
#[tokio::test]
async fn a_unit_variant_still_gets_its_group() {
    let cx = post_cx().await;
    let html = render_form(
        &cx,
        &Schema::new(declared(&cx, || {
            Visibility::form(Post::fields().visibility())
        })),
        &HashMap::new(),
    )
    .await;
    assert_eq!(variant_markers(&html), vec!["1", "2"], "got {html}");
    assert_eq!(
        html.matches("data-variant-of=\"visibility\"").count(),
        2,
        "got {html}"
    );
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
        &Schema::new(declared(&cx, || {
            Publication::form(Post::fields().publication())
        })),
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
