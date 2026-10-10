//! Relation tables, end to end: an owner's detail and edit pages render the
//! child resource's list table narrowed to the owner, its URL state keyed by
//! the relation, and the writes it starts return to the owner's page.

use http::header::LOCATION;
use tablo::{
    Ability, DeclarationErrorKind, Relation, Resource, ResourceDef, Schema, Site, Table,
    TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::framework::common::{
    body_string, get, memory_db, mount, panel, post_fields, refusal, rows,
};

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

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(|_cx: &Cx, ability: Ability<'_, Owner>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Update(_)
                )
            })
            .table(Table::new(TextColumn::new(lens!(Owner.name))))
            .detail(tablo::Detail::new(TextColumn::new(lens!(Owner.name))))
            // The second owner's name is empty, so its title falls back to the resource's label
            // and its key.
            .record_title(lens!(Owner.name))
            .relation(Relation::has_many::<ChildResource>(
                Child::fields().owner_id(),
            ))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Owner)]
struct OwnerForm {
    name: String,
}

struct ChildResource;

impl Resource for ChildResource {
    type Model = Child;
    type Form = ChildForm;

    fn declare() -> ResourceDef<Self> {
        let c = ChildForm::controls();
        ResourceDef::new()
            .policy(|cx: &Cx, ability: Ability<'_, Child>| match ability {
                Ability::ViewAny => !has_header(cx, "x-deny-children"),
                Ability::View(_record) => true,
                Ability::Create => !has_header(cx, "x-no-create"),
                Ability::Update(_record) => true,
                Ability::DeleteAny => true,
                Ability::Delete(_) => true,
                Ability::RunAny { .. } | Ability::Run { .. } | Ability::RunHeader { .. } => true,
            })
            .table(Table::new(
                TextColumn::new(lens!(Child.body)).searchable().sortable(),
            ))
            .form(Schema::new((c.body, c.owner_id.label("Owner"))))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Child)]
struct ChildForm {
    body: String,
    #[form(relationship = OwnerResource)]
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
    let bob = toasty::create!(Owner { name: "" })
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
    let router = mount(
        db.clone(),
        panel()
            .resource::<OwnerResource>()
            .resource::<ChildResource>(),
    )
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

/// The detail page carries the relation's writes, each returning to it; the edit page renders no
/// relation.
#[tokio::test]
async fn the_detail_page_carries_the_writes() {
    let (router, _db, ada, _bob) = fixture().await;
    let edit = format!("/admin/owners/{}/edit", ada.id);
    let html = body_string(get(&router, &edit).await).await;
    assert!(!html.contains("data-relation="), "{html}");

    let detail = format!("/admin/owners/{}", ada.id);
    let html = body_string(get(&router, &detail).await).await;
    assert!(html.contains("data-relation=\"children\""), "{html}");
    let return_to = format!("return=%2Fadmin%2Fowners%2F{}", ada.id);
    let create = format!("/admin/children/create?owner_id={}&amp;{return_to}", ada.id);
    assert!(html.contains(&create), "create link {create}: {html}");
    let found = rows(&html);
    assert_eq!(found.len(), 2, "the relation lists its two rows: {html}");
    for body in ["ada-first", "ada-second"] {
        assert!(
            found
                .iter()
                .any(|row| row.cells.iter().any(|cell| cell == body)),
            "the relation lists {body}: {html}"
        );
    }
    for row in &found {
        assert!(
            row.actions
                .edit
                .as_deref()
                .is_some_and(|edit| edit.contains(&return_to)),
            "row edit returns: {html}"
        );
    }
    assert!(
        html.contains(&format!("/bulk-delete?{return_to}")),
        "bulk returns: {html}"
    );
}

/// The child's own policies decide per request: no `ViewAny`, no
/// section; no `Create`, no create link.
#[tokio::test]
async fn the_child_policies_gate_the_section_and_its_create_link() {
    let (router, _db, ada, _bob) = fixture().await;
    let detail = format!("/admin/owners/{}", ada.id);
    let denied = get_with_header(&router, &detail, "x-deny-children").await;
    assert!(!denied.contains("data-relation="), "{denied}");
    let no_create = get_with_header(&router, &detail, "x-no-create").await;
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

/// The owner choice offers each owner by the title its detail page shows.
#[tokio::test]
async fn the_owner_choice_offers_each_owner_by_its_record_title() {
    let (router, _db, ada, bob) = fixture().await;
    let form = body_string(get(&router, "/admin/children/create").await).await;
    assert!(
        form.contains(&format!("<option value=\"{}\">Ada</option>", ada.id)),
        "a titled owner: {form}"
    );
    assert!(
        form.contains(&format!(
            "<option value=\"{}\">Owner {}</option>",
            bob.id, bob.id
        )),
        "an owner with an empty title: {form}"
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
    let errors = refusal(mount(db, panel().resource::<OwnerResource>()));
    assert_eq!(errors.len(), 1, "{errors:?}");
    // An unregistered child has no slug on the panel, so the site names its type.
    assert_eq!(
        errors[0].site,
        Site::Relation(std::any::type_name::<ChildResource>().to_string())
    );
    assert_eq!(errors[0].kind, DeclarationErrorKind::UnregisteredRelation);
}

/// Two relations of one resource to the same child would share one parameter
/// prefix.
#[tokio::test]
async fn two_relations_to_one_child_do_not_build() {
    struct TwiceResource;
    impl Resource for TwiceResource {
        type Model = Owner;
        type Form = tablo::NoForm<Owner>;

        fn declare() -> ResourceDef<Self> {
            let relation = || Relation::has_many::<ChildResource>(Child::fields().owner_id());
            ResourceDef::new()
                .slug("twice")
                .table(Table::new(TextColumn::new(lens!(Owner.name))))
                .relation(relation())
                .relation(relation())
        }
    }

    let db = memory_db(toasty::models!(Owner, Child)).await;
    let errors = refusal(mount(
        db,
        panel()
            .resource::<TwiceResource>()
            .resource::<ChildResource>()
            .resource::<OwnerResource>(),
    ));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].site, Site::Relation("children".to_string()));
    assert_eq!(errors[0].kind, DeclarationErrorKind::DuplicateRelation);
}
