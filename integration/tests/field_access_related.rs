//! A restricted field of a record stays hidden wherever the panel shows the record: on every
//! resource over its model, through another resource's relation column, in a record title, in a
//! key, in a count, in a computed column and in a relation an embedded value holds. The mount
//! refuses a declaration that would show it anyway.

mod resources;
mod sources;

use resources::*;
use tablo::{
    ComputedColumn, DeclarationErrorKind, Detail, FieldAccessFault, NoForm, ReadOnly,
    RelationColumn, Resource, ResourceDef, Site, Table, Tenancy, TenantId, lens, relation,
    testing::columns,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_bytes, body_string, mount, panel, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
pub(super) struct Member {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    salary: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
pub(super) struct Card {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    code: String,
    #[index]
    member_id: Uuid,
    #[belongs_to(key = member_id, references = id)]
    member: toasty::Deferred<Member>,
}

/// A relation to a member kept in an embedded value.
#[derive(Debug, Clone, toasty::Embed)]
struct Assignment {
    member_id: Uuid,
    #[belongs_to(key = member_id, references = id)]
    member: toasty::Deferred<Member>,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Dossier {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    code: String,
    assignment: Assignment,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Warehouse {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    #[has_many]
    pickers: toasty::Deferred<Vec<Picker>>,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Picker {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    salary: String,
    #[index]
    warehouse_id: Uuid,
    #[belongs_to(key = warehouse_id, references = id)]
    warehouse: toasty::Deferred<Warehouse>,
}

/// Who holds a ticket: a member a variant of an embedded enum holds, or no one.
#[derive(Debug, Clone, toasty::Embed)]
pub(super) enum Holder {
    Assigned {
        #[index]
        member_id: Uuid,
        #[belongs_to(key = member_id, references = id)]
        member: toasty::Deferred<Member>,
    },
    Unassigned,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
pub(super) struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    code: String,
    holder: Holder,
}

/// The tenant whose requests view the salary.
pub(super) const HR: TenantId = TenantId::new(Uuid::from_u128(1));
/// A tenant whose requests see no salary.
pub(super) const SALES: TenantId = TenantId::new(Uuid::from_u128(2));

pub(super) fn hr(cx: &Cx) -> bool {
    tablo::tenant_id(cx) == Some(HR)
}

async fn db() -> Db {
    test_db(toasty::models!(
        Member, Card, Dossier, Warehouse, Picker, Ticket
    ))
    .await
}

fn router(db: &Db) -> Router {
    mount(
        db.clone(),
        panel()
            .resource::<MemberResource>()
            .resource::<CardResource>(),
    )
    .expect("panel builds")
}

pub(super) async fn member(db: &Db, tenant: TenantId, salary: &str) -> Member {
    toasty::create!(Member {
        tenant_id: tenant,
        name: "Ada".to_string(),
        salary: salary.to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn card(db: &Db, tenant: TenantId) -> Card {
    let member = member(db, tenant, "S-900").await;
    toasty::create!(Card {
        tenant_id: tenant,
        code: "C-1".to_string(),
        member_id: member.id,
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

pub(super) async fn page(client: &TestClient<'_>, uri: &str) -> String {
    body_string(client.get(uri).await).await
}

/// The faults a refused mount names, with the site each sits in.
pub(super) fn faults(errors: &[tablo::DeclarationError]) -> Vec<(Site, FieldAccessFault)> {
    errors
        .iter()
        .filter_map(|error| match &error.kind {
            DeclarationErrorKind::FieldAccess { fault } => {
                Some((error.site.clone(), fault.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A user the member resource hides the salary from finds no column showing it on the cards'
/// list, detail page or export; the columns showing the member's name and title stay. The user
/// who may view it sees it on each.
#[tokio::test]
async fn a_related_hidden_field_leaves_another_resources_pages() {
    let db = db().await;
    let router = router(&db);
    let hr_card = card(&db, HR).await;
    let sales_card = card(&db, SALES).await;

    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        columns(&page(&hr, "/admin/cards").await),
        ["code", "holder", "pay", "member"]
    );
    let detail = page(&hr, &format!("/admin/cards/{}", hr_card.id)).await;
    assert!(detail.contains("S-900"), "{detail}");
    let csv = String::from_utf8(body_bytes(hr.get("/admin/cards/-/export").await).await).unwrap();
    assert!(csv.contains("S-900"), "{csv}");

    let sales = TestClient::new(&router).tenant(SALES);
    let list = page(&sales, "/admin/cards").await;
    assert_eq!(columns(&list), ["code", "holder", "member"]);
    assert!(list.contains("Ada") && !list.contains("S-900"), "{list}");
    let detail = page(&sales, &format!("/admin/cards/{}", sales_card.id)).await;
    assert!(
        detail.contains("C-1") && !detail.contains("S-900"),
        "{detail}"
    );
    let csv =
        String::from_utf8(body_bytes(sales.get("/admin/cards/-/export").await).await).unwrap();
    assert!(!csv.contains("S-900") && !csv.contains("Pay"), "{csv}");
}

/// A resource over the same model as the restricting one hides the salary too: the restriction is
/// the model's, so its own table, detail and export stop showing it to the user.
#[tokio::test]
async fn a_field_one_resource_hides_is_hidden_on_every_resource_over_its_model() {
    let db = db().await;
    member(&db, HR, "S-700").await;
    member(&db, SALES, "S-900").await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemberResource>()
            .resource::<OpenMember>(),
    )
    .expect("panel builds");

    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        columns(&page(&hr, "/admin/open-members").await),
        ["name", "salary"]
    );
    let sales = TestClient::new(&router).tenant(SALES);
    let list = page(&sales, "/admin/open-members").await;
    assert_eq!(columns(&list), ["name"]);
    assert!(!list.contains("S-900"), "{list}");
}

/// A relation column naming a resource that restricts nothing still hides its member's salary from
/// the user the restricting resource hides it from: the restriction follows the record.
#[tokio::test]
async fn a_relation_column_naming_another_resource_masks_the_restricted_field() {
    let db = db().await;
    card(&db, SALES).await;
    card(&db, HR).await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemberResource>()
            .resource::<OpenMember>()
            .resource::<OpenCard>(),
    )
    .expect("panel builds");

    let sales = TestClient::new(&router).tenant(SALES);
    let list = page(&sales, "/admin/open-cards").await;
    assert_eq!(columns(&list), ["code"]);
    assert!(!list.contains("S-900"), "{list}");
    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        columns(&page(&hr, "/admin/open-cards").await),
        ["code", "pay"]
    );
}

/// A column showing a member under no resource's field access, an unscoped relation column, a
/// computed column including the relation, or a label of a source that is not a resource, is
/// refused while a resource restricts a member's field.
#[tokio::test]
async fn an_unmasked_projection_of_a_restricted_record_is_refused() {
    struct Unmasked;

    impl Resource for Unmasked {
        type Model = Card;
        type Form = NoForm<Card>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(ReadOnly)
                .tenancy(Tenancy::column(Card::fields().tenant_id()))
                .table(Table::new((
                    RelationColumn::unscoped(relation!(Card.member), |m: &Member| m.salary.clone())
                        .label("Raw"),
                    ComputedColumn::new("Computed", |c: &Card| c.member.get().salary.clone())
                        .include_unscoped(Card::fields().member()),
                    RelationColumn::new::<BareSource>(
                        relation!(Card.member),
                        lens!(Member.salary),
                        |m: &Member| m.salary.clone(),
                    )
                    .label("By source"),
                )))
                .detail(Detail::empty())
        }
    }

    let errors = refusal(mount(
        db().await,
        panel().resource::<MemberResource>().resource::<Unmasked>(),
    ));
    let faults = faults(&errors);
    for column in ["raw", "computed", "by source"] {
        let fault = FieldAccessFault::UnmaskedRelation {
            name: column.to_string(),
            model: "Member".to_string(),
        };
        assert!(faults.contains(&(Site::Table, fault)), "{errors:?}");
    }

    mount(db().await, panel().resource::<Unmasked>())
        .expect("no mounted resource restricts a member's field");
}

/// A record title reading a field another resource over the model restricts is refused, since
/// every relation column and choice names the record by its title.
#[tokio::test]
async fn a_title_reading_a_field_another_resource_restricts_is_refused() {
    let errors = refusal(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<TitledMember>(),
    ));
    assert!(
        faults(&errors).contains(&(Site::RecordTitle, FieldAccessFault::RecordTitle)),
        "{errors:?}"
    );
}

/// A key a resource restricts is refused: the key is every record's URL id and its default label.
#[tokio::test]
async fn a_restricted_key_is_refused() {
    let errors = refusal(mount(db().await, panel().resource::<KeyMember>()));
    assert!(
        faults(&errors).contains(&(
            Site::FieldAccess("id".to_string()),
            FieldAccessFault::RecordKey
        )),
        "{errors:?}"
    );
}

/// A computed column including a member held by an embedded value is refused as one including a
/// member held directly.
#[tokio::test]
async fn an_embedded_relation_include_is_refused_like_a_direct_one() {
    let errors = refusal(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<DossierResource>(),
    ));
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "leak".to_string(),
        model: "Member".to_string(),
    };
    assert!(
        faults(&errors).contains(&(Site::Table, fault)),
        "{errors:?}"
    );
}

/// A computed column reading a member a sibling column loads is refused: its closure can read the
/// member through any relation the page loads. The table without it mounts.
#[tokio::test]
async fn a_computed_column_beside_a_restricted_load_is_refused() {
    let errors = refusal(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<SiblingCard>(),
    ));
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "leak".to_string(),
        model: "Member".to_string(),
    };
    assert!(
        faults(&errors).contains(&(Site::Table, fault)),
        "{errors:?}"
    );
    mount(
        db().await,
        panel().resource::<MemberResource>().resource::<PlainCard>(),
    )
    .expect("the table without the computed column mounts");
}

/// A count shows no field of the records it counts, so it mounts over a source that is not a
/// resource and over a resource alike.
#[tokio::test]
async fn a_count_shows_no_field_so_it_mounts_over_any_source() {
    mount(
        db().await,
        panel()
            .resource::<PickerResource>()
            .resource::<CountOfBare>(),
    )
    .expect("a count over a source that is not a resource mounts");
    mount(
        db().await,
        panel()
            .resource::<PickerResource>()
            .resource::<CountOfResource>(),
    )
    .expect("a count over a mounted resource mounts");
}

/// A computed column including a member an enum variant holds is refused as one including a member
/// held directly.
#[tokio::test]
async fn a_relation_an_enum_variant_holds_is_refused_like_a_direct_one() {
    let errors = refusal(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<TicketResource>(),
    ));
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "leak".to_string(),
        model: "Member".to_string(),
    };
    assert!(
        faults(&errors).contains(&(Site::Table, fault)),
        "{errors:?}"
    );
}
