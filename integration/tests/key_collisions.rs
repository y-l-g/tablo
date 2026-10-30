//! A string key that spells a fixed route segment still reaches its record, and a composite key
//! has no record URL.

use http::{StatusCode, header::CONTENT_TYPE};
use tablo::{
    Ability, Detail, Resource, ResourceDef, ResourcePage, Schema, Table, TextColumn, lens,
};
use toasty::Db;

use crate::common::{TestClient, body_string, get, panel_router, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Tag {
    #[key]
    code: String,
    label: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Tag)]
struct TagForm {
    label: String,
}

struct TagResource;

impl Resource for TagResource {
    type Model = Tag;
    type Form = TagForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tags")
            .policy_fn(|_cx, ability| !matches!(ability, Ability::Create))
            .without_page(ResourcePage::Create)
            .table(Table::new(TextColumn::new(lens!(Tag.label))))
            .form(Schema::new(TagForm::controls().label))
            .detail(Detail::new(TextColumn::new(lens!(Tag.label))))
    }
}

async fn tag_db(codes: &[&str]) -> Db {
    let db = test_db(toasty::models!(Tag)).await;
    let mut handle = db.clone();
    for code in codes {
        toasty::create!(Tag {
            code: code.to_string(),
            label: format!("Label of {code}"),
        })
        .exec(&mut handle)
        .await
        .expect("seed tag");
    }
    db
}

/// Each key reaches its detail page, not the list route its key spells.
#[tokio::test]
async fn a_key_spelling_a_list_route_serves_its_record() {
    let codes = ["export", "create", "options", "bulk-delete", "-"];
    let router = panel_router::<TagResource>(tag_db(&codes).await);
    let mut misrouted = Vec::new();
    for code in codes {
        let response = get(&router, &format!("/admin/tags/{code}")).await;
        let status = response.status();
        let html = response
            .headers()
            .get(CONTENT_TYPE)
            .is_some_and(|value| value.as_bytes().starts_with(b"text/html"));
        let body = body_string(response).await;
        if status != StatusCode::OK || !html || !body.contains(&format!("Label of {code}")) {
            misrouted.push(format!("{code}: {status}"));
        }
    }
    assert!(misrouted.is_empty(), "misrouted: {misrouted:?}");
}

/// A record keyed `-` keeps its edit and delete routes beside the list's `-/` routes.
#[tokio::test]
async fn a_dash_key_reaches_its_edit_and_delete_routes() {
    let router = panel_router::<TagResource>(tag_db(&["-"]).await);

    let edit = get(&router, "/admin/tags/-/edit").await;
    assert_eq!(edit.status(), StatusCode::OK);
    assert!(body_string(edit).await.contains("Label of -"));

    let delete = TestClient::new(&router)
        .submit("/admin/tags/-/delete", "confirm=1")
        .await;
    assert!(delete.status().is_redirection(), "{}", delete.status());
    let gone = get(&router, "/admin/tags/-").await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
#[key(owner, slot)]
struct Seat {
    owner: String,
    slot: i64,
    label: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Seat)]
struct SeatForm {
    label: String,
}

/// Seats anyone may edit, had they a URL; the form creates none, as it sets no key.
struct SeatResource;

impl Resource for SeatResource {
    type Model = Seat;
    type Form = SeatForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .without_page(ResourcePage::Create)
            .slug("seats")
            .policy(tablo::Allow)
            .table(Table::new(TextColumn::new(lens!(Seat.label))))
    }
}

/// A composite key has no URL form, so its row lists with no action to reach it.
#[tokio::test]
async fn a_composite_key_row_lists_with_no_action() {
    let db = test_db(toasty::models!(Seat)).await;
    toasty::create!(Seat {
        owner: "ada".to_string(),
        slot: 2,
        label: "Aisle".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .expect("seed seat");
    let html = body_string(get(&panel_router::<SeatResource>(db), "/admin/seats").await).await;
    let rows = crate::common::rows(&html);
    assert_eq!(rows.len(), 1, "{html}");
    assert!(rows[0].cells.iter().any(|cell| cell == "Aisle"), "{html}");
    assert_eq!(rows[0].select_value, None, "{html}");
    assert!(
        html.match_indices("/admin/seats/")
            .all(|(at, needle)| html[at + needle.len()..].starts_with("-/")),
        "no link reaches the row's record: {html}"
    );
}

/// A record URL under a composite key is a programming error the URL scheme cannot serve: it
/// fails loudly, never as a 404 per id.
#[tokio::test]
async fn a_composite_key_record_url_fails_loudly() {
    let router = panel_router::<SeatResource>(test_db(toasty::models!(Seat)).await);
    assert_eq!(
        get(&router, "/admin/seats/whatever/edit").await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
