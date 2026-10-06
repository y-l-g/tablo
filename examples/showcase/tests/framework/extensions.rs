//! The extension points, end to end: an app's own column, filter, field
//! control and action, written against the public traits only, served by a
//! real `Panel`.
//!
//! Each piece is declared the way an app outside this crate would declare
//! it, so what this suite pins is that the traits are enough: nothing here
//! reaches a framework internal.

use tablo_core::{
    Ability, Action, BooleanColumn, Column, Committed, Control, ControlInput, DeclarationErrorKind,
    Field, Filter, FilterInput, Mutation, Resource, ResourceDef, Schema, Site, Table, TextColumn,
    lens,
};
use toasty::{Db, stmt::Expr};
use topcoat::{context::Cx, view::*};
use uuid::Uuid;

use crate::framework::common::{
    body_string, filter_options, get, input_value, memory_db, mount, panel, panel_router,
    post_fields, refusal, response_cookies, rows,
};

/// The flash notification a response set, decoded: the text the list shows
/// after the redirect.
fn flash(response: &http::Response<topcoat::router::Body>) -> String {
    response_cookies(response)
        .into_iter()
        .find(|(name, _)| name.ends_with("tablo_notification"))
        .map(|(_, value)| {
            percent_encoding::percent_decode_str(&value)
                .decode_utf8_lossy()
                .into_owned()
        })
        .unwrap_or_default()
}

#[derive(Debug, toasty::Model, Clone)]
struct Task {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    done: bool,
}

/// What `after_commit` was handed, one row per call.
#[derive(Debug, toasty::Model, Clone)]
struct Log {
    #[key]
    #[auto]
    id: Uuid,
    mutation: String,
    rows: i64,
}

/// A column that renders a view of its own: the title as a `<mark>`, and the
/// title's length in the export.
struct Highlighted;

impl Column<Task> for Highlighted {
    fn name(&self) -> &str {
        "highlighted"
    }

    fn label(&self) -> &str {
        "Highlighted"
    }

    fn text(&self, row: &Task) -> String {
        format!("{} chars", row.title.len())
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &Task) -> BoxView<'a> {
        let title = row.title.clone();
        view! { cx => <mark data-highlight="">(title)</mark> }.boxed()
    }
}

/// A filter over a predicate no built-in spells: titles that start with the
/// submitted letter.
struct Initial;

impl Filter<Task> for Initial {
    fn name(&self) -> &str {
        "initial"
    }

    fn label(&self) -> &str {
        "Initial"
    }

    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        let letter = value.trim();
        (letter.len() == 1).then(|| Task::fields().title().like(format!("{letter}%")))
    }

    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select(
            cx,
            vec![
                ("A".to_string(), "A…".to_string()),
                ("B".to_string(), "B…".to_string()),
            ],
        )
    }
}

/// A control rendering a text input with a marker an app would style.
struct Shouty;

impl Control for Shouty {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
        let attrs = input.attributes(cx);
        view! { cx => <input type="text" data-shouty="" (attrs)> }.boxed()
    }

    fn display<'a>(&self, cx: &'a Cx, value: &str) -> BoxView<'a> {
        let loud = value.to_uppercase();
        view! { cx => <strong>(loud)</strong> }.boxed()
    }
}

/// Mark tasks done: on a row, or on the selection. A task already done
/// refuses it.
struct Complete;

impl Action<TaskResource> for Complete {
    const NAME: &'static str = "complete";

    fn label(_cx: &Cx) -> String {
        "Complete".to_string()
    }

    fn can_run(_cx: &Cx, task: &Task) -> bool {
        !task.done
    }

    async fn run(_cx: &Cx, tasks: &[Task], ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        for task in tasks {
            Task::filter(Task::fields().id().eq(task.id))
                .update()
                .done(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

/// A bulk-only action that always fails: nothing it wrote may survive.
struct Explode;

impl Action<TaskResource> for Explode {
    const NAME: &'static str = "explode";
    const ROW: bool = false;

    fn label(_cx: &Cx) -> String {
        "Explode".to_string()
    }

    fn can_run(_cx: &Cx, _task: &Task) -> bool {
        true
    }

    async fn run(_cx: &Cx, tasks: &[Task], ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        for task in tasks {
            Task::filter(Task::fields().id().eq(task.id))
                .update()
                .title("exploded".to_string())
                .exec(&mut *ex)
                .await?;
        }
        Err(std::io::Error::other("the action refused itself").into())
    }
}

struct TaskResource;

impl Resource for TaskResource {
    type Model = Task;
    type Form = TaskForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tasks")
            .policy(|_cx: &Cx, ability: Ability<'_, Task>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            })
            .table(
                Table::new((
                    TextColumn::new(lens!(Task.title)).sortable(),
                    Highlighted,
                    BooleanColumn::new(lens!(Task.done)),
                ))
                .filters((Initial,)),
            )
            .form(Schema::new((
                Field::custom(Task::fields().title(), Shouty),
                Field::toggle(Task::fields().done()),
            )))
            .action::<Complete>()
            .action::<Explode>()
    }

    async fn after_commit(cx: &Cx, committed: Committed<Task>) -> topcoat::Result<()> {
        let mutation = match committed.mutation() {
            Mutation::Action(name) => format!("action:{name}"),
            other => format!("{other:?}"),
        };
        let mut db = tablo_core::db::db(cx);
        toasty::create!(Log {
            mutation,
            rows: committed.records().len() as i64,
        })
        .exec(&mut db)
        .await
        .map_err(|error| -> topcoat::Error { error.into() })?;
        Ok(())
    }
}

#[derive(tablo_core::RecordForm)]
#[form(model = Task)]
struct TaskForm {
    title: String,
    #[form(blank = false)]
    done: bool,
}

async fn db() -> Db {
    memory_db(toasty::models!(Task, Log)).await
}

async fn seed(db: &Db, title: &str, done: bool) -> Task {
    let mut db = db.clone();
    toasty::create!(Task {
        title: title.to_string(),
        done,
    })
    .exec(&mut db)
    .await
    .expect("seed task")
}

async fn task(db: &Db, id: Uuid) -> Task {
    let mut db = db.clone();
    Task::get_by_id(&mut db, &id)
        .await
        .expect("the task exists")
}

async fn logs(db: &Db) -> Vec<Log> {
    let mut db = db.clone();
    Log::all().exec(&mut db).await.expect("query logs")
}

#[tokio::test]
async fn an_app_column_renders_its_view_and_exports_its_text() {
    let db = db().await;
    seed(&db, "Alpha", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let html = body_string(get(&router, "/admin/tasks").await).await;
    assert!(
        html.contains("<mark data-highlight=\"\">Alpha</mark>"),
        "the cell is the column's own view: {html}"
    );
    assert!(
        html.contains(">Highlighted<"),
        "the header is its label: {html}"
    );

    let csv = body_string(get(&router, "/admin/tasks/export").await).await;
    assert!(
        csv.starts_with("Title,Highlighted,Done\n"),
        "the export writes every label: {csv}"
    );
    assert!(
        csv.contains("Alpha,5 chars,No\n"),
        "the export writes each column's text: {csv}"
    );
}

#[tokio::test]
async fn a_boolean_column_renders_an_icon_with_its_label() {
    let db = db().await;
    seed(&db, "Alpha", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let html = body_string(get(&router, "/admin/tasks").await).await;
    assert!(
        html.contains("data-boolean=\"true\""),
        "the cell carries the value: {html}"
    );
    assert!(
        html.contains("<span class=\"sr-only\">Yes</span>"),
        "the icon is labelled for assistive tech: {html}"
    );

    seed(&db, "Bravo", false).await;
    let html = body_string(get(&router, "/admin/tasks").await).await;
    assert!(html.contains("data-boolean=\"false\""), "{html}");
    assert!(
        html.contains("<span class=\"sr-only\">No</span>"),
        "a false value reads No: {html}"
    );
}

#[tokio::test]
async fn an_app_filter_renders_its_control_and_applies_its_predicate() {
    let db = db().await;
    seed(&db, "Alpha", false).await;
    seed(&db, "Bravo", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let html = body_string(get(&router, "/admin/tasks").await).await;
    let options = filter_options(&html, "initial")
        .unwrap_or_else(|| panic!("the initial filter renders: {html}"));
    assert_eq!(
        options
            .iter()
            .map(|option| (
                option.value.as_str(),
                option.label.as_str(),
                option.selected
            ))
            .collect::<Vec<_>>(),
        [("", "All", true), ("A", "A…", false), ("B", "B…", false)],
        "the control offers the filter's options: {html}"
    );

    let html = body_string(get(&router, "/admin/tasks?f.initial=B").await).await;
    assert!(html.contains("Bravo"), "the matching row stays: {html}");
    assert!(
        !html.contains("Alpha"),
        "the other row is filtered out: {html}"
    );

    let html = body_string(get(&router, "/admin/tasks?f.initial=nope").await).await;
    assert!(
        html.contains("initial:nope (invalid value)"),
        "a value the filter refuses is reported: {html}"
    );
}

#[tokio::test]
async fn an_app_control_renders_inside_the_field_chrome() {
    let db = db().await;
    let task = seed(&db, "Alpha", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let html = body_string(get(&router, &format!("/admin/tasks/{}/edit", task.id)).await).await;
    assert!(
        html.contains("data-shouty=\"\""),
        "the control's input: {html}"
    );
    assert_eq!(
        input_value(&html, "title").as_deref(),
        Some("Alpha"),
        "with the stored value: {html}"
    );
    assert!(
        html.contains("for=\"title\""),
        "the field's label points at the control: {html}"
    );

    let html = body_string(get(&router, &format!("/admin/tasks/{}", task.id)).await).await;
    assert!(
        html.contains("<strong>ALPHA</strong>"),
        "the detail page shows the control's display: {html}"
    );
}

#[tokio::test]
async fn a_toggle_submits_false_when_unchecked_and_true_when_checked() {
    let db = db().await;
    let task = seed(&db, "Alpha", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    // The hidden `false` comes first, so a checked box's `true` posts after
    // it and wins. The vendored checkbox wraps its input, so the assertions
    // read the input tag, whose attributes render in no guaranteed order
    // (topcoat#122). A checked box carries `checked=""`.
    let hidden = "<input type=\"hidden\" name=\"done\" value=\"false\">";
    let html = body_string(get(&router, &format!("/admin/tasks/{}/edit", task.id)).await).await;
    let checkbox = checkbox_after(&html, hidden);
    for attribute in [
        "type=\"checkbox\"",
        "id=\"done\"",
        "name=\"done\"",
        "value=\"true\"",
        "checked=\"\"",
    ] {
        assert!(
            checkbox.contains(attribute),
            "the checkbox follows the hidden input and carries {attribute}: {checkbox}"
        );
    }

    let open = seed(&db, "Bravo", false).await;
    let html = body_string(get(&router, &format!("/admin/tasks/{}/edit", open.id)).await).await;
    let checkbox = checkbox_after(&html, hidden);
    assert!(
        !checkbox.contains("checked=\"\""),
        "a false value renders unchecked: {checkbox}"
    );

    // The browser sends the hidden `false` alone for an unchecked box.
    let edit = format!("/admin/tasks/{}/edit", task.id);
    let response = post_fields(&router, &edit, &[("title", "Alpha"), ("done", "false")]).await;
    assert_eq!(response.status(), 303);
    assert!(!self::task(&db, task.id).await.done);

    // And `false` then `true` for a checked one: the last value wins.
    let response = post_fields(
        &router,
        &edit,
        &[("title", "Alpha"), ("done", "false"), ("done", "true")],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert!(self::task(&db, task.id).await.done);
}

#[tokio::test]
async fn a_row_renders_only_the_actions_its_record_allows() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let done = seed(&db, "Done", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let html = body_string(get(&router, "/admin/tasks").await).await;
    assert!(
        html.contains(&format!(
            "action=\"/admin/tasks/{}/-/actions/complete\"",
            open.id
        )),
        "an open task offers Complete: {html}"
    );
    assert!(
        !html.contains(&format!("/admin/tasks/{}/-/actions/complete", done.id)),
        "a done task does not: {html}"
    );
    assert!(
        !html.contains(&format!("/admin/tasks/{}/-/actions/explode", open.id)),
        "a bulk-only action renders no row button: {html}"
    );
    assert!(
        html.contains("formaction=\"/admin/tasks/-/actions/complete\""),
        "the bulk bar offers Complete for the selection: {html}"
    );
    assert!(
        html.contains("formaction=\"/admin/tasks/-/actions/explode\""),
        "and the bulk-only action: {html}"
    );
    // The resource allows no delete, so the bulk bar carries the actions
    // alone: no delete trigger or dialog, and the form posts to the first
    // action's route.
    assert!(!html.contains("data-bulk-confirm-trigger"), "{html}");
    assert!(!html.contains("data-bulk-confirm-dialog"), "{html}");
    assert!(
        html.contains("action=\"/admin/tasks/-/actions/complete\""),
        "the bulk form posts to the first action: {html}"
    );
    // Both rows take a bulk action (`explode` runs on any task), so both
    // render a checkbox.
    let found = rows(&html);
    assert_eq!(found.len(), 2, "both rows render: {html}");
    for title in ["Open", "Done"] {
        assert!(
            found
                .iter()
                .any(|row| row.cells.iter().any(|cell| cell == title)),
            "the list shows {title}: {html}"
        );
    }
    assert!(
        found.iter().all(|row| row.select_value.is_some()),
        "both rows take a bulk action, so both render a checkbox: {html}"
    );
}

#[tokio::test]
async fn a_row_action_runs_in_the_transaction_and_reaches_after_commit() {
    let db = db().await;
    let task = seed(&db, "Alpha", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let response = post_fields(
        &router,
        &format!("/admin/tasks/{}/-/actions/complete", task.id),
        &[],
    )
    .await;
    assert_eq!(response.status(), 303, "a committed action redirects");
    assert!(
        flash(&response).contains("Complete: 1 record"),
        "with the default success text: {}",
        flash(&response)
    );
    assert!(self::task(&db, task.id).await.done);

    let logs = logs(&db).await;
    assert_eq!(logs.len(), 1, "one commit, one hook call");
    assert_eq!(logs[0].mutation, "action:complete");
    assert_eq!(logs[0].rows, 1);
}

#[tokio::test]
async fn a_row_the_action_refuses_is_a_403_and_writes_nothing() {
    let db = db().await;
    let task = seed(&db, "Alpha", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let response = post_fields(
        &router,
        &format!("/admin/tasks/{}/-/actions/complete", task.id),
        &[],
    )
    .await;
    assert_eq!(response.status(), 403);
    assert!(logs(&db).await.is_empty());
}

#[tokio::test]
async fn an_unknown_or_misplaced_action_is_a_404() {
    let db = db().await;
    let task = seed(&db, "Alpha", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let response = post_fields(
        &router,
        &format!("/admin/tasks/{}/-/actions/nope", task.id),
        &[],
    )
    .await;
    assert_eq!(response.status(), 404);

    // `explode` is bulk-only, so its row route does not exist.
    let response = post_fields(
        &router,
        &format!("/admin/tasks/{}/-/actions/explode", task.id),
        &[],
    )
    .await;
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn a_bulk_action_runs_once_for_the_whole_selection() {
    let db = db().await;
    let first = seed(&db, "Alpha", false).await;
    let second = seed(&db, "Bravo", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let ids = format!(",{},{},", first.id, second.id);
    let response = post_fields(&router, "/admin/tasks/-/actions/complete", &[("ids", &ids)]).await;
    assert_eq!(response.status(), 303);
    assert!(self::task(&db, first.id).await.done);
    assert!(self::task(&db, second.id).await.done);

    let logs = logs(&db).await;
    assert_eq!(logs.len(), 1, "one bulk write is one hook call");
    assert_eq!(logs[0].rows, 2);
}

#[tokio::test]
async fn a_selection_runs_the_records_the_action_allows() {
    let db = db().await;
    let open = seed(&db, "Open", false).await;
    let done = seed(&db, "Done", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let ids = format!("{},{}", open.id, done.id);
    let response = post_fields(&router, "/admin/tasks/-/actions/complete", &[("ids", &ids)]).await;
    assert_eq!(
        response.status(),
        303,
        "the list answers with a notification"
    );
    assert!(
        flash(&response).contains("Complete: 1 record (1 of 2 skipped)"),
        "the notification reports the skipped record: {}",
        flash(&response)
    );
    assert!(
        self::task(&db, open.id).await.done,
        "the allowed record is written"
    );

    let logs = logs(&db).await;
    assert_eq!(logs.len(), 1, "one bulk write is one hook call");
    assert_eq!(
        logs[0].rows, 1,
        "the hook sees only the record the action ran on"
    );
}

#[tokio::test]
async fn a_selection_the_action_refuses_entirely_writes_nothing() {
    let db = db().await;
    let first = seed(&db, "Alpha", true).await;
    let second = seed(&db, "Bravo", true).await;
    let router = panel_router::<TaskResource>(db.clone());

    let ids = format!("{},{}", first.id, second.id);
    let response = post_fields(&router, "/admin/tasks/-/actions/complete", &[("ids", &ids)]).await;
    assert_eq!(
        response.status(),
        303,
        "the list answers with an error notification"
    );
    assert!(
        flash(&response).contains("2 selected records cannot take this action"),
        "the notification names the refusal: {}",
        flash(&response)
    );
    assert!(logs(&db).await.is_empty());
}

#[tokio::test]
async fn a_failing_action_rolls_back_what_it_wrote() {
    let db = db().await;
    let task = seed(&db, "Alpha", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let ids = task.id.to_string();
    let response = post_fields(&router, "/admin/tasks/-/actions/explode", &[("ids", &ids)]).await;
    assert!(
        response.status().is_server_error(),
        "a failed action is an error, got {}",
        response.status()
    );
    assert_eq!(
        self::task(&db, task.id).await.title,
        "Alpha",
        "the write inside the failed action rolled back"
    );
    assert!(logs(&db).await.is_empty(), "nothing committed, no hook");
}

#[tokio::test]
async fn a_bulk_action_without_a_selection_writes_nothing() {
    let db = db().await;
    seed(&db, "Alpha", false).await;
    let router = panel_router::<TaskResource>(db.clone());

    let response = post_fields(&router, "/admin/tasks/-/actions/complete", &[("ids", "")]).await;
    assert_eq!(response.status(), 303);
    assert!(
        flash(&response).contains("Select at least one row first"),
        "{}",
        flash(&response)
    );
    assert!(logs(&db).await.is_empty());
}

/// Two actions sharing a name.
struct Twice;

impl Action<TwiceResource> for Twice {
    const NAME: &'static str = "twice";

    fn label(_cx: &Cx) -> String {
        "Twice".to_string()
    }

    fn can_run(_cx: &Cx, _task: &Task) -> bool {
        true
    }

    async fn run(_cx: &Cx, _: &[Task], _: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        Ok(())
    }
}

struct TwiceResource;

impl Resource for TwiceResource {
    type Model = Task;
    type Form = tablo_core::NoForm<Task>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .table(Table::new(TextColumn::new(lens!(Task.title))))
            .action::<Twice>()
            .action::<Twice>()
    }
}

#[tokio::test]
async fn two_actions_sharing_a_name_fail_the_build() {
    let errors = refusal(mount(db().await, panel().resource::<TwiceResource>()));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].site, Site::Registration);
    assert_eq!(
        errors[0].kind,
        DeclarationErrorKind::DuplicateAction { name: "twice" }
    );
}

/// A destructive action behind the confirmation marker.
struct Archive;

impl Action<ConfirmResource> for Archive {
    const NAME: &'static str = "archive";
    const CONFIRM: bool = true;

    fn label(_cx: &Cx) -> String {
        "Archive".to_string()
    }

    async fn run(_cx: &Cx, tasks: &[Task], ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        for task in tasks {
            Task::filter(Task::fields().id().eq(task.id))
                .update()
                .done(true)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

struct ConfirmResource;

impl Resource for ConfirmResource {
    type Model = Task;
    type Form = tablo_core::NoForm<Task>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("confirmed")
            .policy(|_cx: &Cx, ability: Ability<'_, Task>| {
                matches!(ability, Ability::ViewAny | Ability::View(_))
            })
            .table(Table::new(TextColumn::new(lens!(Task.title))))
            .action::<Archive>()
    }
}

#[tokio::test]
async fn a_confirmatory_row_action_refuses_an_unmarked_post() {
    let db = db().await;
    let task = seed(&db, "Alpha", false).await;
    let router = panel_router::<ConfirmResource>(db.clone());
    let url = format!("/admin/confirmed/{}/-/actions/archive", task.id);

    let response = post_fields(&router, &url, &[]).await;
    assert_eq!(response.status(), 400);
    assert!(!self::task(&db, task.id).await.done);

    let response = post_fields(&router, &url, &[("confirm", "1")]).await;
    assert_eq!(response.status(), 303);
    assert!(self::task(&db, task.id).await.done);
}

#[tokio::test]
async fn a_confirmatory_bulk_action_refuses_an_unmarked_post() {
    let db = db().await;
    let first = seed(&db, "Alpha", false).await;
    let second = seed(&db, "Bravo", false).await;
    let router = panel_router::<ConfirmResource>(db.clone());
    let ids = format!("{},{}", first.id, second.id);

    let response = post_fields(
        &router,
        "/admin/confirmed/-/actions/archive",
        &[("ids", &ids)],
    )
    .await;
    assert_eq!(response.status(), 400);
    assert!(!self::task(&db, first.id).await.done);
    assert!(!self::task(&db, second.id).await.done);

    let response = post_fields(
        &router,
        "/admin/confirmed/-/actions/archive",
        &[("ids", &ids), ("confirm", "1")],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert!(self::task(&db, first.id).await.done);
    assert!(self::task(&db, second.id).await.done);
}

#[tokio::test]
async fn a_confirmatory_action_renders_triggers_and_dialogs() {
    let db = db().await;
    let task = seed(&db, "Alpha", false).await;
    let router = panel_router::<ConfirmResource>(db.clone());

    let html = body_string(get(&router, "/admin/confirmed").await).await;
    assert!(
        html.contains("data-row-delete-trigger")
            && html.contains(&format!(
                "data-row-delete-action=\"/admin/confirmed/{}/-/actions/archive\"",
                task.id
            )),
        "the row button opens the shared dialog with its POST target: {html}"
    );
    assert!(
        html.contains("id=\"-admin-confirmed-action-confirm\""),
        "the shared row dialog renders: {html}"
    );
    assert!(
        html.contains("data-bulk-action-confirm-trigger")
            && html
                .contains("data-bulk-action-confirm-action=\"/admin/confirmed/-/actions/archive\"")
            && html.contains("data-bulk-action-confirm-dialog"),
        "the bulk bar carries its own confirmation: {html}"
    );

    let plain =
        body_string(get(&panel_router::<TaskResource>(db.clone()), "/admin/tasks").await).await;
    assert!(
        plain.contains("/-/actions/complete\"")
            && !plain.contains("data-row-delete-action=\"/admin/tasks/")
            && !plain.contains("data-bulk-action-confirm-trigger"),
        "an immediate action submits directly: {plain}"
    );
}

/// The opening tag right after `marker` in `html`, without its `>`.
fn tag_after<'h>(html: &'h str, marker: &str) -> &'h str {
    let at = html
        .find(marker)
        .unwrap_or_else(|| panic!("no {marker} in {html}"));
    let rest = &html[at + marker.len()..];
    &rest[..rest.find('>').expect("the tag closes")]
}

/// The checkbox input tag following the hidden `false` input: the vendored
/// checkbox wraps its input, so the tag right after the hidden one is the wrapper.
fn checkbox_after<'h>(html: &'h str, hidden: &str) -> &'h str {
    let at = html
        .find(hidden)
        .unwrap_or_else(|| panic!("the hidden input renders: {html}"));
    tag_after(&html[at + hidden.len()..], "<input")
}
