//! Columns editing their field in a list's cells: each cell posts its field alone, with the
//! record's version, to the record's edit route, which saves it as the edit page would and lands
//! back on the list. A cell renders its value where the user may not change the field or edit the
//! record, and the mount refuses a column whose form control would not save what it posts.

use tablo::{
    Ability, DeclarationErrorKind, EditableColumnFault, FieldAccess, NoForm, Page, PageDef,
    RecordTable, Resource, ResourceDef, Schema, SelectColumn, Site, Table, Tenancy, TenantId,
    TextInputColumn, ToggleColumn, Widget, lens,
    testing::{CellEdit, Row, rows},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{
    TestClient, body_string, field_error, mount, panel, refusal, set_cookie_header, test_db,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum Stage {
    Open,
    Done,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Task {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    title: String,
    stage: Stage,
    urgent: bool,
    grade: String,
    deleted_at: Option<jiff::Timestamp>,
    #[version]
    version: u64,
}

#[derive(tablo::RecordForm)]
#[form(model = Task)]
struct TaskForm {
    title: String,
    #[form(options)]
    stage: Stage,
    urgent: bool,
    grade: String,
}

/// The tenant whose requests change the grade.
const LEADS: TenantId = TenantId::new(Uuid::from_u128(1));
/// A tenant whose requests see a read-only grade.
const STAFF: TenantId = TenantId::new(Uuid::from_u128(2));

fn leads(cx: &Cx) -> bool {
    tablo::tenant_id(cx) == Some(LEADS)
}

/// The task no one may edit.
const FROZEN: &str = "frozen";

struct TaskResource;

impl Resource for TaskResource {
    type Model = Task;
    type Form = TaskForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tasks")
            .policy_fn(|_cx, ability: Ability<'_, Task>| match ability {
                Ability::Update(task) => task.title != FROZEN,
                _ => true,
            })
            .tenancy(Tenancy::column(Task::fields().tenant_id()))
            .soft_delete(lens!(Task.deleted_at))
            .field_access(lens!(Task.grade), FieldAccess::new().edit(leads))
            .table(Table::new((
                TextInputColumn::new(lens!(Task.title)),
                SelectColumn::new(lens!(Task.stage)),
                ToggleColumn::new(lens!(Task.urgent)),
                TextInputColumn::new(lens!(Task.grade)),
            )))
    }
}

async fn task(db: &Db, tenant: TenantId, title: &str) -> Task {
    toasty::create!(Task {
        tenant_id: tenant,
        title: title.to_string(),
        stage: Stage::Open,
        urgent: false,
        grade: "B".to_string(),
        deleted_at: None,
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn stored(db: &Db, id: Uuid) -> Task {
    Task::get_by_id(&mut db.clone(), id).await.unwrap()
}

async fn list_rows(client: &TestClient<'_>) -> Vec<Row> {
    rows(&body_string(client.get("/admin/tasks").await).await)
}

fn edit<'r>(row: &'r Row, column: &str) -> Option<&'r CellEdit> {
    row.edits.iter().find(|edit| edit.column == column)
}

fn columns(row: &Row) -> Vec<&str> {
    row.edits.iter().map(|edit| edit.column.as_str()).collect()
}

#[tokio::test]
async fn each_cell_renders_its_control_with_the_stored_value_and_version() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let record = task(&db, LEADS, "write the docs").await;
    let rows = list_rows(&TestClient::new(&router).tenant(LEADS)).await;
    let row = rows.first().expect("the task lists");
    let version = Some(record.version.to_string());
    let action = format!("/admin/tasks/{}/edit", record.id);
    let expected = [
        ("title", "write the docs"),
        ("stage", "open"),
        ("urgent", "false"),
        ("grade", "B"),
    ];
    assert_eq!(row.edits.len(), expected.len(), "{row:?}");
    for (column, value) in expected {
        let edit = edit(row, column).unwrap_or_else(|| panic!("{column} edits: {row:?}"));
        assert_eq!(edit.value.as_deref(), Some(value), "{column}");
        assert_eq!(edit.version, version, "{column}");
        assert!(
            edit.action.starts_with(&action),
            "{column}: {}",
            edit.action
        );
    }
}

#[tokio::test]
async fn a_cell_saves_its_field_alone_and_lands_back_on_the_list() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let record = task(&db, LEADS, "write the docs").await;
    let client = TestClient::new(&router).tenant(LEADS);
    let rows = list_rows(&client).await;
    let urgent = edit(&rows[0], "urgent").expect("the toggle edits").clone();

    let version = urgent.version.unwrap();
    let response = client
        .submit(
            &urgent.action,
            &format!("urgent=false&urgent=true&record_version={version}"),
        )
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let location = response.headers()["location"].to_str().unwrap();
    assert!(location.starts_with("/admin/tasks"), "{location}");
    let saved = stored(&db, record.id).await;
    assert!(saved.urgent);
    assert_eq!(
        (saved.title, saved.stage, saved.grade),
        (record.title, record.stage, record.grade),
        "the save keeps every field the cell does not post"
    );

    let stage = edit(&list_rows(&client).await[0], "stage").unwrap().clone();
    let version = stage.version.unwrap();
    let response = client
        .submit(
            &stage.action,
            &format!("stage=done&record_version={version}"),
        )
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let saved = stored(&db, record.id).await;
    assert_eq!((saved.stage, saved.urgent), (Stage::Done, true));
}

#[tokio::test]
async fn a_cell_another_save_overtook_saves_nothing() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let record = task(&db, LEADS, "write the docs").await;
    let client = TestClient::new(&router).tenant(LEADS);
    let title = edit(&list_rows(&client).await[0], "title").unwrap().clone();

    record
        .clone()
        .update()
        .grade("A")
        .exec(&mut db.clone())
        .await
        .unwrap();
    let version = title.version.unwrap();
    let response = client
        .submit(
            &title.action,
            &format!("title=mine&record_version={version}"),
        )
        .await;
    assert_eq!(response.status(), 200, "the edit page shows the refusal");
    let saved = stored(&db, record.id).await;
    assert_eq!(
        (saved.title.as_str(), saved.grade.as_str()),
        ("write the docs", "A")
    );
}

#[tokio::test]
async fn a_field_the_user_may_not_change_renders_its_value() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    task(&db, LEADS, "lead's task").await;
    task(&db, STAFF, "staff task").await;

    let leads = list_rows(&TestClient::new(&router).tenant(LEADS)).await;
    assert_eq!(columns(&leads[0]), ["title", "stage", "urgent", "grade"]);
    let staff = list_rows(&TestClient::new(&router).tenant(STAFF)).await;
    assert_eq!(columns(&staff[0]), ["title", "stage", "urgent"]);
    assert!(
        staff[0].cells.iter().any(|cell| cell == "B"),
        "{:?}",
        staff[0]
    );
}

#[tokio::test]
async fn a_record_the_user_may_not_edit_renders_its_values() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    task(&db, LEADS, FROZEN).await;
    let rows = list_rows(&TestClient::new(&router).tenant(LEADS)).await;
    assert!(rows[0].edits.is_empty(), "{:?}", rows[0]);
    assert!(
        rows[0].cells.iter().any(|cell| cell == FROZEN),
        "{:?}",
        rows[0]
    );
}

#[tokio::test]
async fn a_trashed_record_renders_its_values() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let mut record = task(&db, LEADS, "old task").await;
    record
        .update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db.clone())
        .await
        .unwrap();
    let client = TestClient::new(&router).tenant(LEADS);
    let html = body_string(client.get("/admin/tasks?f.trashed=only").await).await;
    let rows = rows(&html);
    assert_eq!(rows.len(), 1, "{html}");
    assert!(rows[0].edits.is_empty(), "{:?}", rows[0]);
}

#[tokio::test]
async fn a_cell_posting_a_field_the_user_may_not_change_is_refused() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let record = task(&db, STAFF, "staff task").await;
    let client = TestClient::new(&router).tenant(STAFF);
    let title = edit(&list_rows(&client).await[0], "title").unwrap().clone();
    let version = title.version.unwrap();
    let response = client
        .submit(&title.action, &format!("grade=A&record_version={version}"))
        .await;
    assert_eq!(response.status(), 400);
    assert_eq!(stored(&db, record.id).await.grade, "B");
}

#[tokio::test]
async fn a_refused_cell_renders_the_edit_page_with_its_error() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(db.clone(), panel().resource::<TaskResource>()).expect("panel builds");
    let record = task(&db, LEADS, "write the docs").await;
    let client = TestClient::new(&router).tenant(LEADS);
    let stage = edit(&list_rows(&client).await[0], "stage").unwrap().clone();
    let version = stage.version.unwrap();
    let response = client
        .submit(
            &stage.action,
            &format!("stage=lost&record_version={version}"),
        )
        .await;
    assert_eq!(response.status(), 200);
    let html = body_string(response).await;
    assert!(field_error(&html, "stage").is_some(), "{html}");
    assert_eq!(stored(&db, record.id).await.stage, Stage::Open);
}

/// The tasks on a dashboard.
struct Tasks;

impl Widget for Tasks {
    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        tablo::record_table::<TaskResource>(cx, RecordTable::new()).await
    }
}

struct Dashboard;

impl Page for Dashboard {
    fn declare() -> PageDef {
        PageDef::new().widget::<Tasks>()
    }

    async fn render(cx: &Cx) -> topcoat::Result<impl View> {
        Ok(view! { cx => <main>(tablo::widgets::<Self>(cx))</main> })
    }
}

/// A widget's table renders its cells' forms on a page nothing else sets the CSRF token on, so
/// the table sets it.
#[tokio::test]
async fn a_widgets_cells_carry_a_csrf_token() {
    let db = test_db(toasty::models!(Task)).await;
    let router = mount(
        db.clone(),
        panel().resource::<TaskResource>().page::<Dashboard>(),
    )
    .expect("panel builds");
    task(&db, LEADS, "write the docs").await;
    let response = TestClient::new(&router)
        .tenant(LEADS)
        .get("/admin/dashboard")
        .await;
    assert!(
        set_cookie_header(&response, tablo::csrf::COOKIE_NAME).is_some(),
        "the page sets the token its cells post"
    );
    let rows = rows(&body_string(response).await);
    assert_eq!(rows[0].edits.len(), 4, "{:?}", rows[0]);
}

fn faults(errors: Vec<tablo::DeclarationError>) -> Vec<(String, EditableColumnFault)> {
    errors
        .into_iter()
        .filter(|error| error.site == Site::Table)
        .filter_map(|error| match error.kind {
            DeclarationErrorKind::EditableColumn { column, fault } => Some((column, fault)),
            _ => None,
        })
        .collect()
}

/// A form binding every field but the flag.
#[derive(tablo::RecordForm)]
#[form(model = Task)]
struct SlimForm {
    title: String,
    #[form(options)]
    stage: Stage,
    grade: String,
}

#[tokio::test]
async fn the_mount_refuses_a_column_its_form_would_not_save() {
    struct Misdeclared;

    impl Resource for Misdeclared {
        type Model = Task;
        type Form = SlimForm;

        fn declare() -> ResourceDef<Self> {
            let c = SlimForm::controls();
            let grade = c.grade.visible_when(&c.stage, ["done"]);
            ResourceDef::new()
                .slug("tasks")
                .policy_fn(|_cx, _ability: Ability<'_, Task>| true)
                .tenancy(Tenancy::column(Task::fields().tenant_id()))
                .table(Table::new((
                    // The form binds no flag.
                    ToggleColumn::new(lens!(Task.urgent)),
                    // The form edits the stage with a choice.
                    TextInputColumn::new(lens!(Task.stage)),
                    TextInputColumn::new(lens!(Task.grade)),
                    TextInputColumn::new(lens!(Task.title)),
                )))
                .form(Schema::new((c.title.disabled_on_edit(), c.stage, grade)))
        }
    }

    let db = test_db(toasty::models!(Task)).await;
    let errors = refusal(mount(db, panel().resource::<Misdeclared>()));
    let faults = faults(errors);
    for expected in [
        ("urgent".to_string(), EditableColumnFault::Unplaced),
        (
            "stage".to_string(),
            EditableColumnFault::OtherControl("a one-line text field"),
        ),
        ("grade".to_string(), EditableColumnFault::Conditional),
        ("title".to_string(), EditableColumnFault::Disabled),
    ] {
        assert!(faults.contains(&expected), "{expected:?} in {faults:?}");
    }
}

#[tokio::test]
async fn the_mount_refuses_a_select_offering_other_options_than_its_form() {
    struct Narrowed;

    impl Resource for Narrowed {
        type Model = Task;
        type Form = TaskForm;

        fn declare() -> ResourceDef<Self> {
            let c = TaskForm::controls();
            ResourceDef::new()
                .slug("tasks")
                .policy_fn(|_cx, _ability: Ability<'_, Task>| true)
                .tenancy(Tenancy::column(Task::fields().tenant_id()))
                .table(Table::new(SelectColumn::new(lens!(Task.stage))))
                .form(Schema::new((
                    c.title,
                    c.stage.options(["open"]),
                    c.urgent,
                    c.grade,
                )))
        }
    }

    let db = test_db(toasty::models!(Task)).await;
    let errors = refusal(mount(db, panel().resource::<Narrowed>()));
    assert_eq!(
        faults(errors),
        [("stage".to_string(), EditableColumnFault::OtherOptions)]
    );
}

#[tokio::test]
async fn the_mount_refuses_an_editable_column_without_a_form() {
    struct Formless;

    impl Resource for Formless {
        type Model = Task;
        type Form = NoForm<Task>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("tasks")
                .policy_fn(|_cx, _ability: Ability<'_, Task>| true)
                .tenancy(Tenancy::column(Task::fields().tenant_id()))
                .table(Table::new(ToggleColumn::new(lens!(Task.urgent))))
        }
    }

    let db = test_db(toasty::models!(Task)).await;
    let errors = refusal(mount(db, panel().resource::<Formless>()));
    assert_eq!(
        faults(errors),
        [("urgent".to_string(), EditableColumnFault::NoForm)]
    );
}
