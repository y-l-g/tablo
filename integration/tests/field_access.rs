//! A resource restricting its fields: a field the user may not view leaves the list, the detail
//! page, the export, the search, the sort, the filters and the form; one the user may view but
//! not change renders disabled; a submission posting either is refused; an edit keeps what the
//! user could not change; and the mount refuses a declaration that would show the field anyway.

use tablo::{
    Ability, ComputedColumn, DeclarationErrorKind, FieldAccess, FieldAccessFault, FieldErrors,
    QueryFilter, Reads, Resource, ResourceDef, Schema, Site, Table, Tenancy, TenantId, TextColumn,
    lens,
    testing::{columns, ignored_filters, row_keys},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{
    TestClient, body_bytes, body_string, control, input_value, mount, panel, refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Employee {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    salary: String,
    grade: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Employee)]
struct EmployeeForm {
    name: String,
    salary: String,
    grade: String,
}

/// The tenant whose requests view the salary and change the grade.
const HR: Uuid = Uuid::from_u128(1);
/// A tenant whose requests see no salary and a read-only grade.
const SALES: Uuid = Uuid::from_u128(2);

fn hr(cx: &Cx) -> bool {
    tablo::tenant_id(cx) == Some(HR)
}

struct EmployeeResource;

impl Resource for EmployeeResource {
    type Model = Employee;
    type Form = EmployeeForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("employees")
            .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
            .tenancy(Tenancy::column(Employee::fields().tenant_id()))
            .field_access(lens!(Employee.salary), FieldAccess::new().view(hr))
            .field_access(lens!(Employee.grade), FieldAccess::new().edit(hr))
            .table(
                Table::new((
                    TextColumn::new(lens!(Employee.name)).searchable(),
                    TextColumn::new(lens!(Employee.salary))
                        .searchable()
                        .sortable(),
                    TextColumn::new(lens!(Employee.grade)),
                ))
                .filters(
                    QueryFilter::new("paid", "Paid")
                        .option("High", Employee::fields().salary().eq("S-900"))
                        .reading(Reads::of(lens!(Employee.salary))),
                ),
            )
    }
}

/// The same records, with a default for the salary, which a create the user may not choose it
/// on takes.
struct DefaultedResource;

impl Resource for DefaultedResource {
    type Model = Employee;
    type Form = EmployeeForm;

    fn declare() -> ResourceDef<Self> {
        let c = EmployeeForm::controls();
        ResourceDef::new()
            .slug("defaulted")
            .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
            .tenancy(Tenancy::column(Employee::fields().tenant_id()))
            .field_access(lens!(Employee.salary), FieldAccess::new().view(hr))
            .field_access(lens!(Employee.grade), FieldAccess::new().edit(hr))
            .form(Schema::new((
                c.name,
                c.salary.default("S-100"),
                c.grade.default("junior"),
            )))
    }

    fn validate_record(_cx: &Cx, form: &EmployeeForm) -> FieldErrors<EmployeeFormField> {
        let mut errors = FieldErrors::new();
        if form.salary == "S-bad" {
            errors.add(EmployeeFormField::Salary, "Salary S-bad is out of range");
        }
        errors
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Employee)).await
}

fn router(db: &Db) -> Router {
    mount(
        db.clone(),
        panel()
            .resource::<EmployeeResource>()
            .resource::<DefaultedResource>(),
    )
    .expect("panel builds")
}

async fn employee(db: &Db, tenant: Uuid, name: &str, salary: &str) -> Employee {
    toasty::create!(Employee {
        tenant_id: TenantId::from(tenant),
        name: name.to_string(),
        salary: salary.to_string(),
        grade: "senior".to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn stored(db: &Db, id: Uuid) -> Employee {
    Employee::filter_by_id(id)
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the employee is stored")
}

async fn page(client: &TestClient<'_>, uri: &str) -> String {
    body_string(client.get(uri).await).await
}

/// A user who may not view the salary sees it nowhere: no column, no detail entry, no export
/// column, no control and no value on the edit page. The user who may sees each.
#[tokio::test]
async fn a_hidden_field_leaves_every_page_and_the_export() {
    let db = db().await;
    let router = router(&db);
    let hr_ada = employee(&db, HR, "Ada", "S-900").await;
    let sales_ada = employee(&db, SALES, "Ada", "S-900").await;

    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        columns(&page(&hr, "/admin/employees").await),
        ["name", "salary", "grade"]
    );
    assert!(
        page(&hr, &format!("/admin/employees/{}", hr_ada.id))
            .await
            .contains("S-900")
    );
    let csv =
        String::from_utf8(body_bytes(hr.get("/admin/employees/-/export").await).await).unwrap();
    assert!(csv.contains("S-900"), "{csv}");

    let sales = TestClient::new(&router).tenant(SALES);
    assert_eq!(
        columns(&page(&sales, "/admin/employees").await),
        ["name", "grade"]
    );
    let detail = page(&sales, &format!("/admin/employees/{}", sales_ada.id)).await;
    assert!(
        detail.contains("Ada") && !detail.contains("S-900"),
        "{detail}"
    );
    let csv =
        String::from_utf8(body_bytes(sales.get("/admin/employees/-/export").await).await).unwrap();
    assert!(!csv.contains("S-900") && !csv.contains("Salary"), "{csv}");
    let edit = page(&sales, &format!("/admin/employees/{}/edit", sales_ada.id)).await;
    assert_eq!(input_value(&edit, "salary"), None);
    assert!(!edit.contains("S-900"), "{edit}");
}

/// A search, a sort or a filter over the hidden field tells the user nothing: the search does not
/// match it, and the sort and the filter read as ones the table does not declare.
#[tokio::test]
async fn a_hidden_field_answers_no_search_or_filter() {
    let db = db().await;
    let router = router(&db);
    let high = employee(&db, SALES, "Ada", "S-900").await;
    let low = employee(&db, SALES, "Bob", "S-100").await;
    let hr_high = employee(&db, HR, "Cy", "S-900").await;
    employee(&db, HR, "Di", "S-100").await;

    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        row_keys(&page(&hr, "/admin/employees?q=S-900").await),
        [hr_high.id.to_string()]
    );
    assert_eq!(
        row_keys(&page(&hr, "/admin/employees?f.paid=High").await),
        [hr_high.id.to_string()]
    );

    const ASCENDING: &str = "/admin/employees?sort=salary&dir=asc";
    const DESCENDING: &str = "/admin/employees?sort=salary&dir=desc";
    let ascending = row_keys(&page(&hr, ASCENDING).await);
    let mut descending = row_keys(&page(&hr, DESCENDING).await);
    descending.reverse();
    assert_eq!(ascending, descending);

    let sales = TestClient::new(&router).tenant(SALES);
    assert!(row_keys(&page(&sales, "/admin/employees?q=S-900").await).is_empty());
    // The sort reads as one on a column the table does not declare: both directions list alike.
    assert_eq!(
        row_keys(&page(&sales, ASCENDING).await),
        row_keys(&page(&sales, DESCENDING).await)
    );
    let filtered = page(&sales, "/admin/employees?f.paid=High").await;
    let mut keys = row_keys(&filtered);
    keys.sort();
    let mut both = vec![high.id.to_string(), low.id.to_string()];
    both.sort();
    assert_eq!(keys, both);
    assert_eq!(ignored_filters(&filtered).len(), 1, "{filtered}");
    let export = sales.get("/admin/employees/-/export?f.paid=High").await;
    assert_eq!(export.status(), 400);
}

/// The grade, which the user may view but not change, renders disabled; a submission posting it,
/// or the hidden salary, is refused.
#[tokio::test]
async fn a_posted_masked_field_is_refused() {
    let db = db().await;
    let router = router(&db);
    let ada = employee(&db, SALES, "Ada", "S-900").await;
    let sales = TestClient::new(&router).tenant(SALES);
    let edit_url = format!("/admin/employees/{}/edit", ada.id);

    let edit = page(&sales, &edit_url).await;
    assert!(control(&edit, "grade").contains("disabled=\"\""));
    assert!(!control(&edit, "name").contains("disabled=\"\""));

    for body in ["name=Ada&salary=S-1", "name=Ada&grade=lead"] {
        let response = sales.submit(&edit_url, body).await;
        assert_eq!(response.status(), 400, "{body}");
        let response = sales.submit("/admin/defaulted/-/create", body).await;
        assert_eq!(response.status(), 400, "{body}");
    }
    let kept = stored(&db, ada.id).await;
    assert_eq!(
        (kept.salary.as_str(), kept.grade.as_str()),
        ("S-900", "senior")
    );
}

/// An edit keeps what the user could not change, and a create takes the control's default.
#[tokio::test]
async fn a_write_keeps_or_defaults_the_masked_fields() {
    let db = db().await;
    let router = router(&db);
    let ada = employee(&db, SALES, "Ada", "S-900").await;
    let sales = TestClient::new(&router).tenant(SALES);

    let response = sales
        .submit(
            &format!("/admin/employees/{}/edit", ada.id),
            "name=Ada+Lovelace",
        )
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let saved = stored(&db, ada.id).await;
    assert_eq!(
        (
            saved.name.as_str(),
            saved.salary.as_str(),
            saved.grade.as_str()
        ),
        ("Ada Lovelace", "S-900", "senior")
    );

    let response = sales.submit("/admin/defaulted/-/create", "name=Bob").await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let bob = Employee::filter(Employee::fields().name().eq("Bob"))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the create stored Bob");
    assert_eq!(
        (bob.salary.as_str(), bob.grade.as_str()),
        ("S-100", "junior")
    );
}

/// A create needing a value the user may not choose, with no default to take, is refused, and the
/// list offers none.
#[tokio::test]
async fn a_create_the_user_cannot_complete_is_refused() {
    let db = db().await;
    let router = router(&db);
    let sales = TestClient::new(&router).tenant(SALES);

    assert_eq!(sales.get("/admin/employees/-/create").await.status(), 403);
    let response = sales.submit("/admin/employees/-/create", "name=Bob").await;
    assert_eq!(response.status(), 403);
    let list = page(&sales, "/admin/employees").await;
    assert!(!list.contains("/admin/employees/-/create"));

    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(hr.get("/admin/employees/-/create").await.status(), 200);
}

/// A box accepting the terms, which a create must check.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Pledge {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    name: String,
    agreed: bool,
}

#[derive(tablo::RecordForm)]
#[form(model = Pledge)]
struct PledgeForm {
    name: String,
    agreed: bool,
}

struct PledgeResource;

impl Resource for PledgeResource {
    type Model = Pledge;
    type Form = PledgeForm;

    fn declare() -> ResourceDef<Self> {
        let c = PledgeForm::controls();
        ResourceDef::new()
            .slug("pledges")
            .policy_fn(|_cx, _ability: Ability<'_, Pledge>| true)
            .tenancy(Tenancy::column(Pledge::fields().tenant_id()))
            .field_access(lens!(Pledge.agreed), FieldAccess::new().view(hr))
            .form(Schema::new((c.name, c.agreed.accepted())))
    }
}

/// A field whose rules refuse a blank answer, as an `accepted` box, keeps a user it is hidden
/// from from creating, as a required one does, rather than refusing every create they submit.
#[tokio::test]
async fn a_hidden_field_refusing_a_blank_answer_keeps_the_user_from_creating() {
    let db = test_db(toasty::models!(Pledge)).await;
    let router = mount(db.clone(), panel().resource::<PledgeResource>()).expect("panel builds");
    let sales = TestClient::new(&router).tenant(SALES);
    assert_eq!(sales.get("/admin/pledges/-/create").await.status(), 403);
    let response = sales.submit("/admin/pledges/-/create", "name=Bob").await;
    assert_eq!(response.status(), 403);

    let hr = TestClient::new(&router).tenant(HR);
    let response = hr
        .submit("/admin/pledges/-/create", "name=Ada&agreed=true")
        .await;
    assert_eq!(response.status(), 303);
}

/// An error on a field the user may not view renders as an alert without its message, which could
/// tell the value.
#[tokio::test]
async fn an_error_on_a_hidden_field_shows_an_alert_only() {
    let db = db().await;
    let router = router(&db);
    let ada = employee(&db, SALES, "Ada", "S-bad").await;
    let sales = TestClient::new(&router).tenant(SALES);
    let response = sales
        .submit(&format!("/admin/defaulted/{}/edit", ada.id), "name=Ada+L")
        .await;
    assert_eq!(response.status(), 200, "the form re-renders");
    let html = body_string(response).await;
    assert!(html.contains("data-unseen-error"), "{html}");
    assert!(!html.contains("S-bad"), "{html}");
    assert_eq!(stored(&db, ada.id).await.name, "Ada");
}

/// The mount refuses a record title over a restricted field, and a column that does not say which
/// fields it reads.
#[tokio::test]
async fn a_declaration_showing_a_restricted_field_is_refused() {
    struct Titled;

    impl Resource for Titled {
        type Model = Employee;
        type Form = EmployeeForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .field_access(lens!(Employee.salary), FieldAccess::new().view(hr))
                .record_title(lens!(Employee.salary))
                .table(Table::new((
                    TextColumn::new(lens!(Employee.name)),
                    ComputedColumn::new("Initial", |e: &Employee| e.name[..1].to_string()),
                    TextColumn::new(lens!(Employee.grade))
                        .description(|e: &Employee| e.salary.clone()),
                )))
        }
    }

    let errors = refusal(mount(db().await, panel().resource::<Titled>()));
    let faults: Vec<(&Site, &FieldAccessFault)> = errors
        .iter()
        .filter_map(|error| match &error.kind {
            DeclarationErrorKind::FieldAccess { fault } => Some((&error.site, fault)),
            _ => None,
        })
        .collect();
    assert!(
        faults.contains(&(&Site::RecordTitle, &FieldAccessFault::RecordTitle)),
        "{errors:?}"
    );
    assert!(
        faults.contains(&(
            &Site::Table,
            &FieldAccessFault::UndeclaredReads {
                what: "column",
                name: "initial".to_string()
            }
        )),
        "{errors:?}"
    );
    assert!(
        faults.contains(&(
            &Site::Table,
            &FieldAccessFault::UndeclaredReads {
                what: "column",
                name: "grade".to_string()
            }
        )),
        "a description may read any field: {errors:?}"
    );
}

/// A rule over edits alone leaves the field shown to every user, so a title over it mounts, and a
/// described column mounts once it declares what its description reads.
#[tokio::test]
async fn an_edit_rule_leaves_the_title_free() {
    struct Graded;

    impl Resource for Graded {
        type Model = Employee;
        type Form = EmployeeForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .field_access(lens!(Employee.grade), FieldAccess::new().edit(hr))
                .record_title(lens!(Employee.grade))
                .table(Table::new(
                    TextColumn::new(lens!(Employee.name))
                        .description(|e: &Employee| e.grade.clone())
                        .reading(Reads::of(lens!(Employee.grade))),
                ))
        }
    }

    mount(db().await, panel().resource::<Graded>()).expect("the panel mounts");
}
