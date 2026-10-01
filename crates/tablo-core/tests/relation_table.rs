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

    fn form(_dx: &tablo_core::DeclCx) -> Schema {
        Schema::new(Field::text(Owner::fields().name()))
    }

    fn view(_dx: &tablo_core::DeclCx) -> Schema {
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

    fn table() -> Table<Owner> {
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

    fn form(_dx: &tablo_core::DeclCx) -> Schema {
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

    fn can_view_any(cx: &Cx) -> bool {
        !has_header(cx, "x-deny-children")
    }

    fn can_view(_cx: &Cx, _record: &Child) -> bool {
        true
    }

    fn can_create(cx: &Cx) -> bool {
        !has_header(cx, "x-no-create")
    }

    fn can_update(_cx: &Cx, _record: &Child) -> bool {
        true
    }

    fn can_delete_any(_cx: &Cx) -> bool {
        true
    }

    fn table() -> Table<Child> {
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

/// Whether the request carries `name`: the per-request switch the child's
/// policies read, so one router serves both answers. A build-time check runs
/// without a request and reads `false`.
fn has_header(cx: &Cx, name: &str) -> bool {
    topcoat::context::try_request_context::<http::request::Parts>(cx)
        .is_some_and(|parts| parts.headers.contains_key(name))
}

/// A GET carrying the header `name`.
async fn get_with_header(router: &Router, uri: &str, name: &str) -> String {
    let request = http::Request::builder()
        .uri(uri)
        .header(name, "1")
        .body(topcoat::router::Body::empty())
        .unwrap();
    body_string(router.handle(request).await).await
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
async fn the_relation_reads_and_writes_only_its_prefixed_state() {
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

/// The detail page shows the rows read-only; the edit page carries the
/// writes, each returning to it.
#[tokio::test]
async fn the_detail_page_is_read_only_and_the_edit_page_carries_the_writes() {
    let (router, _db, ada, _bob) = fixture().await;
    let detail = body_string(get(&router, &format!("/admin/owners/{}", ada.id)).await).await;
    let relation = &detail[detail.find("data-relation=").expect("the relation renders")..];
    for write in ["/create", "/edit", "/delete", "data-bulk-form"] {
        assert!(
            !relation.contains(write),
            "no {write} on the detail page: {relation}"
        );
    }

    let edit = format!("/admin/owners/{}/edit", ada.id);
    let html = body_string(get(&router, &edit).await).await;
    assert!(html.contains("data-relation=\"children\""), "{html}");
    let return_to = format!("return=%2Fadmin%2Fowners%2F{}%2Fedit", ada.id);
    let create = format!("/admin/children/create?owner_id={}&amp;{return_to}", ada.id);
    assert!(html.contains(&create), "create link {create}: {html}");
    assert!(
        html.contains(&format!("/edit?{return_to}")),
        "row edit returns: {html}"
    );
    assert!(
        html.contains(&format!("/bulk-delete?{return_to}")),
        "bulk returns: {html}"
    );
}

/// The child's own policies decide per request: no `can_view_any`, no
/// section; no `can_create`, no create link.
#[tokio::test]
async fn the_child_policies_gate_the_section_and_its_create_link() {
    let (router, _db, ada, _bob) = fixture().await;
    let edit = format!("/admin/owners/{}/edit", ada.id);
    let denied = get_with_header(&router, &edit, "x-deny-children").await;
    assert!(!denied.contains("data-relation="), "{denied}");
    let no_create = get_with_header(&router, &edit, "x-no-create").await;
    assert!(no_create.contains("ada-first"), "{no_create}");
    assert!(!no_create.contains("/admin/children/create"), "{no_create}");
}

#[tokio::test]
async fn the_create_page_seeds_the_owner_and_keeps_the_return() {
    let (router, _db, ada, _bob) = fixture().await;

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

/// Two relations of one resource to the same child would share one parameter
/// prefix.
#[tokio::test]
async fn two_relations_to_one_child_do_not_build() {
    struct TwiceResource;
    impl Resource for TwiceResource {
        type Model = Owner;
        type Form = tablo_core::NoForm<Owner>;

        fn slug() -> String {
            "twice".to_string()
        }

        fn table() -> Table<Owner> {
            OwnerResource::table()
        }

        fn relations() -> Vec<Relation<Owner>> {
            let relation = || {
                Relation::has_many::<ChildResource, _>(
                    Child::fields().owner_id(),
                    |owner: &Owner| owner.id,
                )
            };
            vec![relation(), relation()]
        }
    }

    let db = memory_db(toasty::models!(Owner, Child)).await;
    let Err(error) = panel(db)
        .resource::<TwiceResource>()
        .resource::<ChildResource>()
        .build()
    else {
        panic!("two relations to one child must not build");
    };
    assert!(
        error.to_string().contains("two relations to `children`"),
        "got {error}"
    );
}
