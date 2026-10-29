//! A resource restricting its fields: a field the user may not view leaves the list, the detail
//! page, the export, the search, the sort, the filters, the tabs and the form; one the user may
//! view but not change renders disabled; a submission posting either is refused; an edit keeps what
//! the user could not change; and the mount refuses a declaration that would show the field anyway.
//! A widget's record table hides it as the list does.

use tablo::{
    Ability, Color, ComputedColumn, DeclarationErrorKind, Detail, FieldAccess, FieldAccessFault,
    FieldErrors, ListTab, Page, PageDef, QueryFilter, Reads, RecordTable, Resource, ResourceDef,
    Schema, Site, Tab, Table, Tabs, Tenancy, TenantId, TextColumn, Widget, lens,
    testing::{columns, ignored_filters, record_tables, row_keys, rows, tab_links, tabs},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::Router,
    view::{View, view},
};
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
const HR: TenantId = TenantId::new(Uuid::from_u128(1));
/// A tenant whose requests see no salary and a read-only grade.
const SALES: TenantId = TenantId::new(Uuid::from_u128(2));

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
                )
                .tab(ListTab::new("all", "All"))
                .tab(
                    ListTab::new("high", "High")
                        .query(Employee::fields().salary().eq("S-900"))
                        .reading(Reads::of(lens!(Employee.salary)))
                        .badge(),
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

/// The same records, the salary alone in a tab of the form and of the detail page.
struct TabbedResource;

impl Resource for TabbedResource {
    type Model = Employee;
    type Form = EmployeeForm;

    fn declare() -> ResourceDef<Self> {
        let c = EmployeeForm::controls();
        ResourceDef::new()
            .slug("tabbed")
            .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
            .tenancy(Tenancy::column(Employee::fields().tenant_id()))
            .field_access(lens!(Employee.salary), FieldAccess::new().view(hr))
            .form(Schema::new(Tabs::new([
                Tab::new("Profile").schema((c.name, c.grade)),
                Tab::new("Pay").schema(c.salary),
            ])))
            .detail(Detail::new(Tabs::new([
                Tab::new("Profile").columns((
                    TextColumn::new(lens!(Employee.name)),
                    TextColumn::new(lens!(Employee.grade)),
                )),
                Tab::new("Pay").columns(TextColumn::new(lens!(Employee.salary))),
            ])))
    }
}

/// The employees under the list's columns.
struct Staff;

impl Widget for Staff {
    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        tablo::record_table::<EmployeeResource>(cx, RecordTable::new()).await
    }
}

/// The employees under their name and salary alone.
struct Pay;

impl Widget for Pay {
    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        let table = RecordTable::new()
            .heading("Pay")
            .column(lens!(Employee.name))
            .column(lens!(Employee.salary));
        tablo::record_table::<EmployeeResource>(cx, table).await
    }
}

/// A dashboard of [`Staff`] and [`Pay`].
struct Dashboard;

impl Page for Dashboard {
    fn declare() -> PageDef {
        PageDef::new().widget::<Staff>().widget::<Pay>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => <main>(tablo::widgets::<Self>(cx))</main> })
    }
}

/// A widget naming a column the list does not declare.
struct Misnamed;

impl Widget for Misnamed {
    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        let table = RecordTable::new().column_named("wage");
        tablo::record_table::<EmployeeResource>(cx, table).await
    }
}

struct Broken;

impl Page for Broken {
    fn declare() -> PageDef {
        PageDef::new().widget::<Misnamed>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => <main>(tablo::widgets::<Self>(cx))</main> })
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
            .resource::<DefaultedResource>()
            .resource::<TabbedResource>()
            .page::<Dashboard>()
            .page::<Broken>(),
    )
    .expect("panel builds")
}

async fn employee(db: &Db, tenant: TenantId, name: &str, salary: &str) -> Employee {
    toasty::create!(Employee {
        tenant_id: tenant,
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

/// A widget's record table leaves out the salary for a user who may not view it, a column it
/// names included, and lists only the tenant's own employees; the user who may sees the salary.
#[tokio::test]
async fn a_record_table_leaves_out_a_hidden_field() {
    let db = db().await;
    let router = router(&db);
    let hr_ada = employee(&db, HR, "Ada", "S-900").await;
    let sales_ada = employee(&db, SALES, "Ada", "S-900").await;

    for (tenant, own, shown) in [
        (
            HR,
            &hr_ada,
            [vec!["name", "salary", "grade"], vec!["name", "salary"]],
        ),
        (SALES, &sales_ada, [vec!["name", "grade"], vec!["name"]]),
    ] {
        let client = TestClient::new(&router).tenant(tenant);
        let html = page(&client, "/admin/dashboard").await;
        let tables = record_tables(&html);
        assert_eq!(
            tables
                .iter()
                .map(|table| table.columns.clone())
                .collect::<Vec<_>>(),
            shown,
            "{tables:?}"
        );
        assert_eq!(html.contains("S-900"), tenant == HR, "{html}");
        for table in &tables {
            assert_eq!(table.list.as_deref(), Some("/admin/employees"));
            let views: Vec<_> = table
                .rows
                .iter()
                .map(|row| row.actions.view.clone())
                .collect();
            assert_eq!(
                views,
                [Some(format!("/admin/employees/{}", own.id))],
                "only the tenant's own employee: {table:?}"
            );
        }
    }
}

/// A record table naming a column the list does not declare fails its page rather than render
/// without it.
#[tokio::test]
async fn a_record_table_naming_an_undeclared_column_fails_its_page() {
    let db = db().await;
    let router = router(&db);
    let response = TestClient::new(&router)
        .tenant(HR)
        .get("/admin/broken")
        .await;
    assert_eq!(response.status(), 500);
}

/// A tab holding only a field the user may not view renders no trigger and no panel, on the form
/// and on the detail page.
#[tokio::test]
async fn a_tab_of_hidden_fields_leaves_the_page() {
    let db = db().await;
    let router = router(&db);
    let hr_ada = employee(&db, HR, "Ada", "S-900").await;
    let sales_ada = employee(&db, SALES, "Ada", "S-900").await;

    let hr = TestClient::new(&router).tenant(HR);
    let edit = tabs(&page(&hr, &format!("/admin/tabbed/{}/edit", hr_ada.id)).await);
    assert_eq!(
        edit.iter().map(|tab| tab.keys.clone()).collect::<Vec<_>>(),
        [vec!["name", "grade"], vec!["salary"]]
    );
    assert_eq!(
        tabs(&page(&hr, &format!("/admin/tabbed/{}", hr_ada.id)).await).len(),
        2
    );

    let sales = TestClient::new(&router).tenant(SALES);
    let edit = tabs(&page(&sales, &format!("/admin/tabbed/{}/edit", sales_ada.id)).await);
    assert_eq!(
        edit.iter().map(|tab| tab.keys.clone()).collect::<Vec<_>>(),
        [vec!["name", "grade"]]
    );
    let detail = page(&sales, &format!("/admin/tabbed/{}", sales_ada.id)).await;
    assert_eq!(tabs(&detail).len(), 1);
    assert!(!detail.contains("S-900"), "{detail}");
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

    // A tab over the salary counts and narrows for the user who may view it, and is unknown to
    // the one who may not.
    let tabbed = page(&hr, "/admin/employees?tab=high").await;
    assert_eq!(row_keys(&tabbed), [hr_high.id.to_string()]);
    assert_eq!(
        tab_links(&tabbed)
            .iter()
            .map(|tab| (tab.key.as_str(), tab.count))
            .collect::<Vec<_>>(),
        [("all", None), ("high", Some(1))]
    );
    let tabbed = page(&sales, "/admin/employees?tab=high").await;
    let keys: Vec<_> = tab_links(&tabbed).into_iter().map(|tab| tab.key).collect();
    assert_eq!(keys, ["all"]);
    let mut keys = row_keys(&tabbed);
    keys.sort();
    assert_eq!(keys, both);
    assert_eq!(ignored_filters(&tabbed).len(), 1, "{tabbed}");
    let export = sales.get("/admin/employees/-/export?tab=high").await;
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
                .table(
                    Table::new((
                        TextColumn::new(lens!(Employee.name))
                            .url(|e: &Employee| Some(format!("/pay/{}", e.salary))),
                        ComputedColumn::new("Initial", |e: &Employee| e.name[..1].to_string()),
                        TextColumn::new(lens!(Employee.grade))
                            .description(|e: &Employee| e.salary.clone()),
                    ))
                    .row_color(|e: &Employee| (e.salary == "S-900").then_some(Color::Danger))
                    .tab(
                        ListTab::new("high", "High").query(Employee::fields().salary().eq("S-900")),
                    ),
                )
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
    assert!(
        faults.contains(&(
            &Site::Table,
            &FieldAccessFault::UndeclaredReads {
                what: "column",
                name: "name".to_string()
            }
        )),
        "a link may read any field: {errors:?}"
    );
    assert!(
        faults.contains(&(
            &Site::Table,
            &FieldAccessFault::UndeclaredReads {
                what: "row color",
                name: "row_color".to_string()
            }
        )),
        "a row color may read any field: {errors:?}"
    );
    assert!(
        faults.contains(&(
            &Site::Table,
            &FieldAccessFault::UndeclaredReads {
                what: "tab",
                name: "high".to_string()
            }
        )),
        "{errors:?}"
    );
}

/// A row color reading the salary tints a row for the user who may view it, and leaves every row
/// plain for one who may not, whose tint would tell the salary.
#[tokio::test]
async fn a_row_color_reading_a_hidden_field_leaves_the_rows_plain() {
    struct Tinted;

    impl Resource for Tinted {
        type Model = Employee;
        type Form = EmployeeForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("tinted")
                .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .field_access(lens!(Employee.salary), FieldAccess::new().view(hr))
                .table(
                    Table::new(TextColumn::new(lens!(Employee.name)))
                        .row_color(|e: &Employee| (e.salary == "S-900").then_some(Color::Danger))
                        .row_color_reading(Reads::of(lens!(Employee.salary))),
                )
        }
    }

    let db = db().await;
    let router = mount(db.clone(), panel().resource::<Tinted>()).expect("the panel mounts");
    employee(&db, HR, "Ada", "S-900").await;
    employee(&db, SALES, "Ada", "S-900").await;

    let tints = |html: &str| {
        rows(html)
            .into_iter()
            .map(|row| row.color)
            .collect::<Vec<_>>()
    };
    let hr = TestClient::new(&router).tenant(HR);
    assert_eq!(
        tints(&page(&hr, "/admin/tinted").await),
        [Some("danger".to_string())]
    );
    let sales = TestClient::new(&router).tenant(SALES);
    assert_eq!(tints(&page(&sales, "/admin/tinted").await), [None]);
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
