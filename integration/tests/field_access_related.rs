//! Another resource showing a record whose resource restricts a field: a relation column naming
//! the field leaves the list, the detail page and the export of a user who may not view it, and
//! the mount refuses a column that shows the record under no resource's field access.

use tablo::{
    ComputedColumn, DeclarationErrorKind, Detail, FieldAccess, FieldAccessFault, NoForm, ReadOnly,
    RelationColumn, Resource, ResourceDef, Site, Table, Tenancy, TenantId, TextColumn, lens,
    relation, testing::columns,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_bytes, body_string, mount, panel, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Member {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    salary: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Card {
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

/// The tenant whose requests view the salary.
const HR: Uuid = Uuid::from_u128(1);
/// A tenant whose requests see no salary.
const SALES: Uuid = Uuid::from_u128(2);

fn hr(cx: &Cx) -> bool {
    tablo::tenant_id(cx) == Some(HR)
}

struct MemberResource;

impl Resource for MemberResource {
    type Model = Member;
    type Form = NoForm<Member>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("members")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .field_access(lens!(Member.salary), FieldAccess::new().view(hr))
            .record_title(lens!(Member.name))
            .table(Table::new(TextColumn::new(lens!(Member.name))))
    }
}

/// The member's salary, which `MemberResource` restricts.
fn pay() -> RelationColumn<Card> {
    RelationColumn::new::<MemberResource>(relation!(Card.member), lens!(Member.salary), |m| {
        m.salary.clone()
    })
    .label("Pay")
}

/// The member's name, which every user views.
fn holder() -> RelationColumn<Card> {
    RelationColumn::new::<MemberResource>(relation!(Card.member), lens!(Member.name), |m| {
        m.name.clone()
    })
    .label("Holder")
}

struct CardResource;

impl Resource for CardResource {
    type Model = Card;
    type Form = NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("cards")
            .policy(ReadOnly)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new((
                TextColumn::new(lens!(Card.code)),
                holder(),
                pay(),
                RelationColumn::of::<MemberResource>(relation!(Card.member)),
            )))
            .detail(Detail::new((TextColumn::new(lens!(Card.code)), pay())))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Member, Card)).await
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

async fn card(db: &Db, tenant: Uuid) -> Card {
    let member = toasty::create!(Member {
        tenant_id: TenantId::from(tenant),
        name: "Ada".to_string(),
        salary: "S-900".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap();
    toasty::create!(Card {
        tenant_id: TenantId::from(tenant),
        code: "C-1".to_string(),
        member_id: member.id,
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn page(client: &TestClient<'_>, uri: &str) -> String {
    body_string(client.get(uri).await).await
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

/// A column showing a member under no resource's field access, an unscoped relation column or a
/// computed column including the relation, is refused while a resource restricts a member's
/// field.
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
                )))
                .detail(Detail::empty())
        }
    }

    let errors = refusal(mount(
        db().await,
        panel().resource::<MemberResource>().resource::<Unmasked>(),
    ));
    let faults: Vec<(&Site, &FieldAccessFault)> = errors
        .iter()
        .filter_map(|error| match &error.kind {
            DeclarationErrorKind::FieldAccess { fault } => Some((&error.site, fault)),
            _ => None,
        })
        .collect();
    for column in ["raw", "computed"] {
        let fault = FieldAccessFault::UnmaskedRelation {
            column: column.to_string(),
            model: "Member".to_string(),
        };
        assert!(faults.contains(&(&Site::Table, &fault)), "{errors:?}");
    }

    mount(db().await, panel().resource::<Unmasked>())
        .expect("no mounted resource restricts a member's field");
}
