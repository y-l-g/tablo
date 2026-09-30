//! Relation tables, end to end: an owner's detail and edit pages render the
//! child resource's list table narrowed to the owner, its URL state keyed by
//! the relation, and the writes it starts return to the owner's page.

use http::header::LOCATION;
use tablo_core::{Field, Relation, Resource, Schema, Table, TextColumn};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, panel, post_fields};

#[derive(Debug, toasty::Model, Clone)]
struct Owner {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, toasty::Model, Clone)]
struct Child {
    #[key]
    #[auto]
    id: Uuid,
    owner_id: Uuid,
    body: String,
}

struct OwnerResource;

impl Resource for OwnerResource {
    type Model = Owner;
    type Form = OwnerForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new(Field::text(Owner::fields().name()))
    }

    fn view(_cx: &Cx) -> Schema {
        Schema::new(Field::text(Owner::fields().name()))
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &Owner) -> bool {
        true
    }

    fn can_update(_cx: &Cx, _record: &Owner) -> bool {
        true
    }

    fn table(_cx: &Cx) -> Table<Owner> {
        Table::new(
            |owner: &Owner| owner.id.to_string(),
            TextColumn::r#for(Owner::fields().name(), |owner: &Owner| owner.name.clone()),
        )
    }

    fn relations() -> Vec<Relation<Owner>> {
        vec![Relation::has_many::<ChildResource, _>(
            Child::fields().owner_id(),
            |owner: &Owner| owner.id,
        )]
    }
}

#[derive(tablo_core::RecordForm)]
#[form(model = Owner)]
struct OwnerForm {
    name: String,
}

struct ChildResource;

impl Resource for ChildResource {
    type Model = Child;
    type Form = ChildForm;

    fn form(_cx: &Cx) -> Schema {
        Schema::new((
            Field::text(Child::fields().body()),
            Field::choice(Child::fields().owner_id())
                .relationship::<OwnerResource>(
                    OwnerResource::query,
                    |owner: &Owner| owner.id,
                    |owner: &Owner| owner.name.clone(),
                )
                .label("Owner"),
        ))
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn can_view(_cx: &Cx, _record: &Child) -> bool {
        true
    }

    fn can_create(_cx: &Cx) -> bool {
        true
    }

    fn can_update(_cx: &Cx, _record: &Child) -> bool {
        true
    }

    fn can_delete_any(_cx: &Cx) -> bool {
        true
    }

    fn table(_cx: &Cx) -> Table<Child> {
        Table::new(
            |child: &Child| child.id.to_string(),
            TextColumn::r#for(Child::fields().body(), |child: &Child| child.body.clone())
                .searchable()
                .sortable(),
        )
    }
}

#[derive(tablo_core::RecordForm)]
#[form(model = Child)]
struct ChildForm {
    body: String,
    owner_id: Uuid,
}

/// Two owners with two children each, and the router over both resources.
async fn fixture() -> (Router, Db, Owner, Owner) {
    let mut db = memory_db(toasty::models!(Owner, Child)).await;
    let ada = toasty::create!(Owner { name: "Ada" })
        .exec(&mut db)
        .await
        .unwrap();
    let bob = toasty::create!(Owner { name: "Bob" })
        .exec(&mut db)
        .await
        .unwrap();
    for (owner, body) in [
        (&ada, "ada-first"),
        (&ada, "ada-second"),
        (&bob, "bob-first"),
        (&bob, "bob-second"),
    ] {
        toasty::create!(Child {
            owner_id: owner.id,
            body: body.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = panel(db.clone())
        .resource::<OwnerResource>()
        .resource::<ChildResource>()
        .build()
        .expect("panel builds");
    (router, db, ada, bob)
}

#[tokio::test]
async fn the_detail_page_lists_only_the_owners_children() {
    let (router, _db, ada, _bob) = fixture().await;
    let html = body_string(get(&router, &format!("/admin/owners/{}", ada.id)).await).await;
    assert!(
        html.contains("ada-first") && html.contains("ada-second"),
        "{html}"
    );
    assert!(!html.contains("bob-first"), "another owner's rows: {html}");
    assert!(html.contains("data-relation=\"children\""), "{html}");
}

#[tokio::test]
async fn the_relation_reads_and_writes_only_its_keyed_state() {
    let (router, _db, ada, _bob) = fixture().await;
    let page = format!("/admin/owners/{}", ada.id);
    // A bare `q` belongs to no relation; the keyed one narrows the table.
    let html = body_string(get(&router, &format!("{page}?q=zzz&children.q=second")).await).await;
    assert!(
        html.contains("ada-second") && !html.contains("ada-first"),
        "{html}"
    );
    assert!(
        html.contains("name=\"children.q\""),
        "keyed search input: {html}"
    );
    assert!(
        html.contains("children.sort=body"),
        "keyed sort links: {html}"
    );
    assert!(!html.contains("?sort="), "no bare sort link: {html}");
}

#[tokio::test]
async fn the_create_link_seeds_the_owner_and_returns_to_the_page() {
    let (router, _db, ada, _bob) = fixture().await;
    let page = format!("/admin/owners/{}", ada.id);
    let html = body_string(get(&router, &page).await).await;
    let expected = format!(
        "/admin/children/create?owner_id={}&amp;return=%2Fadmin%2Fowners%2F{}",
        ada.id, ada.id
    );
    assert!(html.contains(&expected), "create link {expected}: {html}");

    // The create page preselects the owner and posts back with the return.
    let form = body_string(
        get(
            &router,
            &format!(
                "/admin/children/create?owner_id={}&return=%2Fadmin%2Fowners%2F{}",
                ada.id, ada.id
            ),
        )
        .await,
    )
    .await;
    assert!(
        form.contains(&format!("value=\"{}\" selected", ada.id)),
        "the owner is preselected: {form}"
    );
    assert!(
        form.contains(&format!(
            "action=\"/admin/children/create?return=%2Fadmin%2Fowners%2F{}\"",
            ada.id
        )),
        "the form keeps the return: {form}"
    );
}

#[tokio::test]
async fn a_write_returns_to_a_panel_page_and_ignores_any_other_target() {
    let (router, mut db, ada, _bob) = fixture().await;
    let child = Child::filter(Child::fields().owner_id().eq(ada.id))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    let page = format!("/admin/owners/{}", ada.id);
    let delete = |target: &str| {
        format!(
            "/admin/children/{}/delete?return={}",
            child.id,
            form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>()
        )
    };
    let response = post_fields(&router, &delete(&page), &[("confirm", "1")]).await;
    assert_eq!(response.headers().get(LOCATION).unwrap(), page.as_str());

    for hostile in ["//evil.example", "https://evil.example", "/elsewhere"] {
        let other = toasty::create!(Child {
            owner_id: ada.id,
            body: "again".to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
        let uri = format!(
            "/admin/children/{}/delete?return={}",
            other.id,
            form_urlencoded::byte_serialize(hostile.as_bytes()).collect::<String>()
        );
        let response = post_fields(&router, &uri, &[("confirm", "1")]).await;
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "/admin/children",
            "{hostile} is not followed"
        );
    }
}

#[tokio::test]
async fn the_edit_page_shows_the_relation_too() {
    let (router, _db, ada, _bob) = fixture().await;
    let html = body_string(get(&router, &format!("/admin/owners/{}/edit", ada.id)).await).await;
    assert!(html.contains("ada-first"), "{html}");
    assert!(!html.contains("bob-first"), "{html}");
}

#[tokio::test]
async fn a_relation_to_an_unregistered_resource_does_not_build() {
    let db = memory_db(toasty::models!(Owner, Child)).await;
    let Err(error) = panel(db).resource::<OwnerResource>().build() else {
        panic!("a relation to an unregistered resource must not build");
    };
    assert!(
        error.to_string().contains("relates to `children`"),
        "got {error}"
    );
}
