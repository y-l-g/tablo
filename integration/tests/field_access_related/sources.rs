//! The choices, filters and writes a panel offers through another resource: a choice or a filter
//! whose records a restricted model holds under a label the panel cannot mask, a dependent choice
//! narrowed by a restricted field, a query filter reaching a restricted model through a relation,
//! and a write of a field another resource's form hides.

use tablo::{
    Action, Allow, Field, FieldAccessFault, FieldError, FormInput, FormValues, Outcome,
    QueryFilter, Reads, Resource, ResourceDef, Schema, SelectFilter, Site, Table, Tenancy,
    TextColumn, lens,
};
use toasty::Db;
use topcoat::context::Cx;
use uuid::Uuid;

use super::{Card, HR, Member, MemberResource, SALES, faults, member, page};
use crate::common::{TestClient, mount, panel, refusal, test_db};

/// A label source over members that no panel mounts: it labels each by its salary.
struct SalaryLabel;

impl tablo::extend::OptionSource for SalaryLabel {
    type Model = Member;

    fn scoped_query(_cx: &Cx) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Member>>> {
        Ok(toasty::stmt::Query::all())
    }

    fn label(_cx: &Cx, member: &Member) -> String {
        member.salary.clone()
    }

    fn lists(_cx: &Cx) -> bool {
        true
    }
}

/// A card form whose member is a choice of `MemberResource`'s titles.
#[derive(tablo::RecordForm)]
#[form(model = Card)]
struct CardForm {
    code: String,
    #[form(relationship = MemberResource)]
    member_id: Uuid,
}

/// The choice labelled by the salary a source no panel mounts gives each member.
struct LabelledChoice;

impl Resource for LabelledChoice {
    type Model = Card;
    type Form = CardForm;

    fn declare() -> ResourceDef<Self> {
        let c = CardForm::controls();
        ResourceDef::new()
            .slug("labelled-choices")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .form(Schema::new((
                c.code,
                c.member_id
                    .relationship_labelled::<SalaryLabel>(|m: &Member| m.salary.clone()),
            )))
    }
}

/// The same choice, titled by the member's name as `MemberResource` titles it.
struct TitledChoice;

impl Resource for TitledChoice {
    type Model = Card;
    type Form = CardForm;

    fn declare() -> ResourceDef<Self> {
        let c = CardForm::controls();
        ResourceDef::new()
            .slug("titled-choices")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .form(Schema::new((c.code, c.member_id)))
    }
}

/// A dependent member choice narrowed by the salary, which `MemberResource` restricts.
struct NarrowedChoice;

impl Resource for NarrowedChoice {
    type Model = Card;
    type Form = CardForm;

    fn declare() -> ResourceDef<Self> {
        let c = CardForm::controls();
        let member = c.member_id.depends_on(&c.code, Member::fields().salary());
        ResourceDef::new()
            .slug("narrowed-choices")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .form(Schema::new((c.code, member)))
    }
}

/// The same dependent choice narrowed by the member's name, which no resource restricts.
struct NameNarrowedChoice;

impl Resource for NameNarrowedChoice {
    type Model = Card;
    type Form = CardForm;

    fn declare() -> ResourceDef<Self> {
        let c = CardForm::controls();
        let member = c.member_id.depends_on(&c.code, Member::fields().name());
        ResourceDef::new()
            .slug("name-narrowed-choices")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .form(Schema::new((c.code, member)))
    }
}

/// A card table filtered by a relationship over a source no panel mounts.
struct BareFilterCards;

impl Resource for BareFilterCards {
    type Model = Card;
    type Form = tablo::NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("bare-filter-cards")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Card.code))).filters(
                SelectFilter::relationship::<SalaryLabel>(lens!(Card.member_id)),
            ))
    }
}

/// A card table filtered by a relationship over the mounted member resource.
struct TitledFilterCards;

impl Resource for TitledFilterCards {
    type Model = Card;
    type Form = tablo::NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("titled-filter-cards")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Card.code))).filters(
                SelectFilter::relationship::<MemberResource>(lens!(Card.member_id)),
            ))
    }
}

/// A card table whose query filter reads the member a relation holds.
struct ThroughCards;

impl Resource for ThroughCards {
    type Model = Card;
    type Form = tablo::NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("through-cards")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(
                Table::new(TextColumn::new(lens!(Card.code))).filters(
                    QueryFilter::new("through", "Through")
                        .option("Ada", Card::fields().member().name().eq("Ada"))
                        .reading(Reads::of(Card::fields().member())),
                ),
            )
    }
}

/// A card table whose query filter reads the card alone.
struct LocalCards;

impl Resource for LocalCards {
    type Model = Card;
    type Form = tablo::NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("local-cards")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(
                Table::new(TextColumn::new(lens!(Card.code))).filters(
                    QueryFilter::new("local", "Local")
                        .option("C-1", Card::fields().code().eq("C-1"))
                        .reading(Reads::of(lens!(Card.code))),
                ),
            )
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Member)]
struct MemberForm {
    name: String,
    salary: String,
}

/// The members' edit form, which any user may submit, and which edits every field it shows.
struct MemberEdit;

impl Resource for MemberEdit {
    type Model = Member;
    type Form = MemberForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("member-edits")
            .policy(Allow)
            .tenancy(Tenancy::column(Member::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Member.name))))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Member, Card)).await
}

/// The faults a refused mount names, with the site each sits in.
fn refused<T>(mounted: topcoat::Result<T>) -> Vec<(Site, FieldAccessFault)> {
    faults(&refusal(mounted))
}

/// A choice labelled by a source no panel mounts is refused over a model a resource restricts; the
/// titled choice of the mounted resource mounts.
#[tokio::test]
async fn a_choice_labelled_by_a_source_no_panel_mounts_is_refused() {
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "member_id".to_string(),
        model: "Member".to_string(),
    };
    let found = refused(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<LabelledChoice>(),
    ));
    assert!(found.contains(&(Site::Form, fault)), "{found:?}");
    mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<TitledChoice>(),
    )
    .expect("a choice titled by a mounted resource mounts");
}

/// A dependent choice narrowed by a field a resource restricts would show which records hold each
/// value of it: it is refused. Narrowed by a field every user may view, it mounts.
#[tokio::test]
async fn a_dependent_choice_narrowed_by_a_restricted_field_is_refused() {
    let found = refused(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<NarrowedChoice>(),
    ));
    let fault = FieldAccessFault::NarrowedBy {
        field: "salary".to_string(),
    };
    assert!(found.contains(&(Site::Form, fault)), "{found:?}");
    mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<NameNarrowedChoice>(),
    )
    .expect("a choice narrowed by a field every user may view mounts");
}

/// A relationship filter labelled by a source no panel mounts is refused over a model a resource
/// restricts; the same filter over the mounted resource mounts.
#[tokio::test]
async fn a_filter_over_a_source_no_panel_mounts_is_refused() {
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "member_id".to_string(),
        model: "Member".to_string(),
    };
    let found = refused(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<BareFilterCards>(),
    ));
    assert!(found.contains(&(Site::Table, fault)), "{found:?}");
    mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<TitledFilterCards>(),
    )
    .expect("a filter over a mounted resource mounts");
}

/// A query filter whose reads reach a restricted model through a relation is refused; one reading
/// the card alone mounts.
#[tokio::test]
async fn a_query_filter_through_a_relation_to_a_restricted_model_is_refused() {
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "through".to_string(),
        model: "Member".to_string(),
    };
    let found = refused(mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<ThroughCards>(),
    ));
    assert!(found.contains(&(Site::Table, fault)), "{found:?}");
    mount(
        db().await,
        panel()
            .resource::<MemberResource>()
            .resource::<LocalCards>(),
    )
    .expect("a query filter reading the card alone mounts");
}

/// A member's stored salary, which only the write under test changes.
async fn stored(db: &Db, id: Uuid) -> Member {
    Member::filter_by_id(id)
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the member is stored")
}

/// A form of another resource over the model hides the salary from the user the restricting
/// resource hides it from: the form omits the control, and a submission posting the salary is
/// refused, so the write keeps the stored salary.
#[tokio::test]
async fn a_write_of_a_field_another_resource_hides_is_refused() {
    let db = db().await;
    let sales_member = member(&db, SALES, "S-900").await;
    let hr_member = member(&db, HR, "S-700").await;
    let router = mount(
        db.clone(),
        panel()
            .resource::<MemberResource>()
            .resource::<MemberEdit>(),
    )
    .expect("panel builds");

    let sales = TestClient::new(&router).tenant(SALES);
    let edit_url = format!("/admin/member-edits/{}/edit", sales_member.id);
    let edit = page(&sales, &edit_url).await;
    assert!(!edit.contains("name=\"salary\""), "{edit}");
    let response = sales.submit(&edit_url, "name=Ada&salary=S-1").await;
    assert_eq!(response.status(), 400);
    assert_eq!(stored(&db, sales_member.id).await.salary, "S-900");

    let hr = TestClient::new(&router).tenant(HR);
    let response = hr
        .submit(
            &format!("/admin/member-edits/{}/edit", hr_member.id),
            "name=Ada&salary=S-2",
        )
        .await;
    assert_ne!(response.status(), 400);
    assert_eq!(stored(&db, hr_member.id).await.salary, "S-2");
}

/// The member an action's input chooses, labelled by the salary a source no panel mounts gives.
struct Choosing;

impl FormInput for Choosing {
    type Field = std::convert::Infallible;

    fn key(field: Self::Field) -> &'static str {
        match field {}
    }

    fn schema() -> Schema {
        Schema::new(
            Field::choice_input("member")
                .relationship_labelled::<SalaryLabel>(|m: &Member| m.salary.clone()),
        )
    }

    fn parse(_cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>> {
        tablo::parse_scalar::<String>("member", values, None).map_err(|e| vec![e])?;
        Ok(Self)
    }

    fn write(&self, _out: &mut FormValues) {}
}

/// Picks a member for a card.
struct Pick;

impl Action<PickCards> for Pick {
    type Input = Choosing;
    const NAME: &'static str = "pick";

    fn label() -> tablo::Text {
        "Pick".into()
    }

    async fn run(
        _cx: &Cx,
        _cards: &[Card],
        _choosing: Choosing,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Outcome, tablo::WriteError> {
        Ok(Outcome::done())
    }
}

/// Cards whose action's input chooses a member under a label a source no panel mounts gives.
struct PickCards;

impl Resource for PickCards {
    type Model = Card;
    type Form = tablo::NoForm<Card>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("pick-cards")
            .policy(Allow)
            .tenancy(Tenancy::column(Card::fields().tenant_id()))
            .table(Table::new(TextColumn::new(lens!(Card.code))))
            .action::<Pick>()
    }
}

/// An action's input choosing a member is refused when the choice is labelled by a source no panel
/// mounts over a model a resource restricts a field of.
#[tokio::test]
async fn an_action_input_labelled_by_a_source_no_panel_mounts_is_refused() {
    let fault = FieldAccessFault::UnmaskedRelation {
        name: "member".to_string(),
        model: "Member".to_string(),
    };
    let found = refused(mount(
        db().await,
        panel().resource::<MemberResource>().resource::<PickCards>(),
    ));
    assert!(found.contains(&(Site::Registration, fault)), "{found:?}");
}
