//! A CSV import under field access: a request imports only the fields it may change, a field it
//! may not takes its default, and the mount refuses an import on a resource that creates
//! nothing.

use tablo::{
    Ability, DeclarationErrorKind, FieldAccess, FieldErrors, NoForm, Resource, ResourceDef, Schema,
    Site, Tenancy, TenantId,
    testing::{ImportError, ImportRefusal, import_columns, import_errors, import_refusal},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_string, mount, panel, refusal, test_db};

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
        let c = EmployeeForm::controls();
        ResourceDef::new()
            .slug("employees")
            .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
            .tenancy(Tenancy::column(Employee::fields().tenant_id()))
            .field_access(tablo::lens!(Employee.salary), FieldAccess::new().view(hr))
            .field_access(tablo::lens!(Employee.grade), FieldAccess::new().edit(hr))
            .form(Schema::new((
                c.name,
                c.salary.default("S-100"),
                c.grade.default("junior"),
            )))
            .importable()
    }

    /// Refuses a row named `Hidden` twice under the salary, which a sales request cannot see.
    fn validate_record(_cx: &Cx, form: &EmployeeForm) -> FieldErrors<EmployeeFormField> {
        let mut errors = FieldErrors::new();
        if form.name == "Hidden" {
            errors.add(EmployeeFormField::Salary, "Salary is out of range");
            errors.add(EmployeeFormField::Salary, "Salary is not on the grid");
        }
        errors
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Employee)).await
}

fn router(db: &Db) -> Router {
    mount(db.clone(), panel().resource::<EmployeeResource>()).expect("panel builds")
}

async fn import(client: &TestClient<'_>, csv: &str) -> http::Response<topcoat::router::Body> {
    client
        .submit_multipart("/admin/employees/-/import", &[("file", Some("e.csv"), csv)])
        .await
}

async fn employees(db: &Db) -> Vec<Employee> {
    Employee::all().exec(&mut db.clone()).await.unwrap()
}

#[tokio::test]
async fn an_import_takes_only_the_fields_the_request_may_change() {
    let db = db().await;
    let router = router(&db);
    let hr = TestClient::new(&router).tenant(HR);
    let sales = TestClient::new(&router).tenant(SALES);

    let hr_page = body_string(hr.get("/admin/employees/-/import").await).await;
    assert_eq!(import_columns(&hr_page), ["name", "salary", "grade"]);
    let sales_page = body_string(sales.get("/admin/employees/-/import").await).await;
    assert_eq!(import_columns(&sales_page), ["name"]);
    let template = body_string(sales.get("/admin/employees/-/import/template").await).await;
    assert_eq!(template, "name\n");

    for csv in ["name,salary\nAda,S-900\n", "name,grade\nAda,lead\n"] {
        let html = body_string(import(&sales, csv).await).await;
        assert_eq!(
            import_refusal(&html),
            Some(ImportRefusal::UnknownColumn),
            "{csv}"
        );
    }
    assert!(employees(&db).await.is_empty());

    let resp = import(&sales, "name\nAda\n").await;
    assert_eq!(resp.status(), 303);
    let imported = employees(&db).await;
    assert_eq!(imported.len(), 1);
    assert_eq!(
        (
            imported[0].tenant_id,
            imported[0].salary.as_str(),
            imported[0].grade.as_str()
        ),
        (TenantId::from(SALES), "S-100", "junior"),
        "a field the request may not change takes its default, in the request's tenant"
    );
}

#[tokio::test]
async fn the_mount_refuses_an_import_on_a_resource_that_creates_nothing() {
    struct EditOnly;

    impl Resource for EditOnly {
        type Model = Employee;
        type Form = EmployeeForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .edit_only()
                .importable()
        }
    }

    struct ListOnly;

    impl Resource for ListOnly {
        type Model = Employee;
        type Form = NoForm<Employee>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("listed")
                .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .importable()
        }
    }

    let refused: Vec<_> = refusal(mount(
        db().await,
        panel().resource::<EditOnly>().resource::<ListOnly>(),
    ))
    .into_iter()
    .map(|error| (error.resource, error.site, error.kind))
    .collect();
    assert_eq!(
        refused,
        [
            (
                Some(std::any::type_name::<EditOnly>()),
                Site::Registration,
                DeclarationErrorKind::ImportWithoutCreate
            ),
            (
                Some(std::any::type_name::<ListOnly>()),
                Site::Registration,
                DeclarationErrorKind::ImportWithoutCreate
            ),
        ]
    );
}

/// An error under a field the request cannot see names no key, once per row, as the create form
/// says only that a hidden field refuses it.
#[tokio::test]
async fn an_error_under_a_hidden_field_names_no_key() {
    let db = db().await;
    let router = router(&db);
    let hr = TestClient::new(&router).tenant(HR);
    let sales = TestClient::new(&router).tenant(SALES);

    let html = body_string(import(&sales, "name\nHidden\n").await).await;
    assert_eq!(import_errors(&html), [ImportError { row: 2, key: None }]);
    assert!(
        !html.contains("salary") && !html.contains("Salary"),
        "the hidden field's key and label stay off the page"
    );

    let html = body_string(import(&hr, "name\nHidden\n").await).await;
    let salary = ImportError {
        row: 2,
        key: Some("salary".to_string()),
    };
    assert_eq!(import_errors(&html), [salary.clone(), salary]);
    assert!(employees(&db).await.is_empty());
}

/// A control disabled on the create page posts nothing there, so no column takes it.
#[tokio::test]
async fn a_header_naming_a_disabled_control_refuses_the_file() {
    struct Ledger;

    impl Resource for Ledger {
        type Model = Employee;
        type Form = EmployeeForm;

        fn declare() -> ResourceDef<Self> {
            let c = EmployeeForm::controls();
            ResourceDef::new()
                .slug("ledger")
                .policy_fn(|_cx, _ability: Ability<'_, Employee>| true)
                .tenancy(Tenancy::column(Employee::fields().tenant_id()))
                .form(Schema::new((
                    c.name,
                    c.salary.default("S-100"),
                    c.grade.default("junior").disabled(),
                )))
                .importable()
        }
    }

    let db = db().await;
    let router = mount(db.clone(), panel().resource::<Ledger>()).expect("panel builds");
    let client = TestClient::new(&router).tenant(HR);

    let html = body_string(client.get("/admin/ledger/-/import").await).await;
    assert_eq!(import_columns(&html), ["name", "salary"]);
    let html = body_string(
        client
            .submit_multipart(
                "/admin/ledger/-/import",
                &[("file", Some("e.csv"), "name,grade\nAda,lead\n")],
            )
            .await,
    )
    .await;
    assert_eq!(import_refusal(&html), Some(ImportRefusal::UnknownColumn));
    assert!(employees(&db).await.is_empty());
}
