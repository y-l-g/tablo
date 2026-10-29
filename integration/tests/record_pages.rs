//! A resource's record pages and relation tabs: the policy's `View` of the record gates them, and
//! the panel's field access reaches what a record page renders only through `can_view_field`.

use tablo::{
    Ability, Detail, FieldAccess, RecordPage, RecordPageDef, Relation, Resource, ResourceDef,
    Table, TextColumn, can_view_field, lens,
};
use topcoat::{
    Result,
    context::Cx,
    router::Router,
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{body_string, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    secret: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Comment {
    #[key]
    #[auto]
    id: Uuid,
    note_id: Uuid,
    body: String,
}

/// Comments anyone may list, a tab of their note.
struct CommentResource;

impl Resource for CommentResource {
    type Model = Comment;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|_, ability| matches!(ability, Ability::ViewAny | Ability::View(_)))
            .table(Table::new(TextColumn::new(lens!(Comment.body))))
    }
}

/// Notes whose `View` the header `x-deny-view` refuses, and whose secret `x-hide-secret` hides.
struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|cx, ability| match ability {
                Ability::ViewAny => true,
                Ability::View(_) => !has_header(cx, "x-deny-view"),
                _ => false,
            })
            .field_access(
                lens!(Note.secret),
                FieldAccess::new().view(|cx| !has_header(cx, "x-hide-secret")),
            )
            .table(Table::new(TextColumn::new(lens!(Note.title))))
            .detail(Detail::new(TextColumn::new(lens!(Note.title))))
            .record_page::<SecretPage>()
            .relation(Relation::has_many::<CommentResource>(Comment::fields().note_id()).tab())
    }
}

/// Shows the note's secret to the requests the field access lets view it.
struct SecretPage;

impl RecordPage<NoteResource> for SecretPage {
    fn declare() -> RecordPageDef<Note> {
        RecordPageDef::new().slug("secret")
    }

    async fn render(cx: &Cx, note: Note) -> Result<impl View> {
        let secret = if can_view_field::<NoteResource, _>(cx, lens!(Note.secret)) {
            note.secret
        } else {
            String::new()
        };
        Ok(view! { cx => <p>(secret)</p> })
    }
}

/// Whether the request carries `name`, which the policy and the field access read. A build-time
/// check runs without a request and reads `false`.
fn has_header(cx: &Cx, name: &str) -> bool {
    topcoat::context::try_context::<http::request::Parts>(cx)
        .is_some_and(|parts| parts.headers.contains_key(name))
}

/// A GET carrying the headers `names`: its status and body.
async fn get_with(router: &Router, uri: &str, names: &[&str]) -> (u16, String) {
    let mut request = http::Request::builder().uri(uri);
    for name in names {
        request = request.header(*name, "1");
    }
    let response = router
        .handle(request.body(topcoat::router::Body::empty()).unwrap())
        .await;
    (response.status().as_u16(), body_string(response).await)
}

/// The router, and the note's URL.
async fn fixture() -> (Router, String) {
    let mut db = test_db(toasty::models!(Note, Comment)).await;
    let note = toasty::create!(Note {
        title: "Plans",
        secret: "the-launch-date",
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(
        db,
        panel()
            .resource::<NoteResource>()
            .resource::<CommentResource>(),
    )
    .expect("panel builds");
    (router, format!("/admin/notes/{}", note.id))
}

#[tokio::test]
async fn a_record_page_and_a_tab_answer_403_to_a_record_the_policy_refuses() {
    let (router, note) = fixture().await;
    for page in [format!("{note}/secret"), format!("{note}/comments")] {
        assert_eq!(get_with(&router, &page, &[]).await.0, 200, "{page}");
        assert_eq!(
            get_with(&router, &page, &["x-deny-view"]).await.0,
            403,
            "{page}"
        );
    }
}

#[tokio::test]
async fn can_view_field_answers_the_panels_field_access() {
    let (router, note) = fixture().await;
    let page = format!("{note}/secret");
    let (status, html) = get_with(&router, &page, &[]).await;
    assert_eq!(status, 200);
    assert!(html.contains("the-launch-date"), "{html}");

    let (status, html) = get_with(&router, &page, &["x-hide-secret"]).await;
    assert_eq!(status, 200);
    assert!(!html.contains("the-launch-date"), "{html}");
}
