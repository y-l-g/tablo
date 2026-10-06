//! Hidden variant groups and validation.
//!
//! An embedded enum's form renders every variant's payload in a marked `Group`,
//! and the page shows only the group the discriminant names. Validation has
//! to agree with that render: a value the user cannot see must not block the
//! submit. The state is the showcase's own declaration shape — a derived
//! embedded value over a real model and a real panel — with a typed leaf in the
//! inactive variant so a validated group would refuse the submission.

use tablo_core::{
    Ability, Auth, Field, Panel, Resource, ResourceDef, Schema, Table, TextColumn, lens,
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount};

/// The embedded value under test: one variant whose payload is a typed leaf.
#[derive(Debug, Clone, PartialEq, toasty::Embed, tablo_core::EmbeddedForm)]
enum Body {
    #[column(variant = 1)]
    Text { note: String },
    #[column(variant = 2)]
    Video {
        /// `twelve` is not a whole number, so the typed parser refuses it —
        /// which is exactly what a validated inactive group would do.
        seconds: i64,
    },
}

#[derive(Debug, Clone, toasty::Model)]
struct Clip {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    body: Body,
}

struct ClipResource;

impl Resource for ClipResource {
    type Model = Clip;
    type Form = ClipForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("clips")
            .policy(|_cx: &topcoat::context::Cx, ability: Ability<'_, Clip>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            })
            .table(Table::new(TextColumn::new(lens!(Clip.title))))
            .form(
                Schema::new(Field::text(Clip::fields().title()))
                    .extend(Body::form(Clip::fields().body())),
            )
    }
}
#[derive(tablo_core::RecordForm)]
#[form(model = Clip)]
struct ClipForm {
    title: String,
    #[form(embed)]
    body: Body,
}
#[tokio::test]
async fn a_hidden_variant_groups_fields_do_not_block_the_submit() {
    let db = Db::builder()
        .models(toasty::models!(Clip))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push schema");
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<ClipResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);

    let csrf = Uuid::new_v4().to_string();
    // `body=1` names `Text`, so `Video`'s group is the one the page hides —
    // and its typed leaf holds a value the type cannot parse.
    let response = client
        .csrf(&csrf)
        .post_form(
            "/admin/clips/create",
            format!("title=Clip&body=1&body_note=hello&body_seconds=twelve&csrf_token={csrf}"),
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "the hidden variant must not block the submit, got {} {}",
        response.status(),
        body_string(response).await
    );

    let mut db_q = db.clone();
    let clip = Clip::all().exec(&mut db_q).await.unwrap().remove(0);
    assert_eq!(
        clip.body,
        Body::Text {
            note: "hello".to_string()
        },
        "the named variant is the one the submission read"
    );
}

/// The other half of the rule: the variant the discriminant *does* name still
/// validates its own fields, hidden or not.
#[tokio::test]
async fn the_named_variants_fields_still_validate() {
    let db = Db::builder()
        .models(toasty::models!(Clip))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push schema");
    let router = mount(
        db.clone(),
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<ClipResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);

    let csrf = Uuid::new_v4().to_string();
    // `body=2` names `Video`, so its typed leaf is the visible one and must
    // refuse `twelve`.
    let response = client
        .csrf(&csrf)
        .post_form(
            "/admin/clips/create",
            format!("title=Clip&body=2&body_seconds=twelve&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(response.status(), 200, "the visible field re-renders");
    // The refusal's wording is `typed_leaves`'s; this pins the HTTP wiring: the
    // named variant's invalid leaf re-renders the create form and writes nothing.
    let mut db_q = db.clone();
    assert!(
        Clip::all().exec(&mut db_q).await.unwrap().is_empty(),
        "a refused create writes nothing"
    );

    // An emptied visible leaf carries the control's label, not the flattened
    // column: an `i64` leaf has no blank answer, so the parse refuses it as required.
    let response = client
        .csrf(&csrf)
        .post_form(
            "/admin/clips/create",
            format!("title=Clip&body=2&body_seconds=&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(response.status(), 200, "the refused blank re-renders");
    let html = body_string(response).await;
    assert_eq!(
        tablo_test::field_error(&html, "body_seconds").as_deref(),
        Some("Seconds is required"),
        "the refusal must be worded with the control's label, got {html}"
    );
}
