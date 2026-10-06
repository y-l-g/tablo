use toasty::{Db, stmt::List};
use topcoat::context::{Cx, CxTestBuilder};

use super::*;
use crate::{
    Ability, ComputedColumn, lens,
    resource::{Resource, ResourceDef},
    table::{Column, SelectFilter, Sort, TablePage, TableState, TernaryFilter, TextColumn},
    test_support::User,
};

#[derive(Debug, Clone, toasty::Model)]
struct Task {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    status: String,
    featured: bool,
    created_at: jiff::Timestamp,
}

fn status_table() -> Table<Task> {
    Table::<Task>::new(TextColumn::new(lens!(Task.title))).filters(SelectFilter::new(
        Task::fields().status(),
        vec!["published".to_string(), "draft".to_string()],
    ))
}

fn filters_state(pairs: &[(&str, &str)]) -> TableState {
    TableState {
        filters: pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..TableState::default()
    }
}

#[tokio::test]
async fn table_search_filters_via_column() {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(User { name: "Ada" })
        .exec(&mut db)
        .await
        .unwrap();
    toasty::create!(User { name: "Bob" })
        .exec(&mut db)
        .await
        .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let col = TextColumn::new(lens!(User.name)).searchable();
    let expr = col.search_expr("Ada").unwrap();
    let mut db = crate::db::db(&cx);
    let rows = User::filter(expr).exec(&mut db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");
    assert!(col.search_expr("").is_none());
    assert!(col.search_expr("   ").is_none());
}

#[tokio::test]
async fn wired_table_carries_the_declared_action_chrome() {
    struct ChromeResource;
    impl Resource for ChromeResource {
        type Model = User;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(|_cx: &Cx, ability: Ability<'_, User>| {
                    matches!(ability, Ability::DeleteAny | Ability::Delete(_))
                })
                .table(Table::new(TextColumn::new(lens!(User.name))).paginate(25))
        }
    }

    let db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // The wiring derives the action URLs from the request path, so the Cx needs one.
    let parts = http::Request::builder()
        .uri("/admin/dummies")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    let cx = crate::test_support::panel_cx::<ChromeResource>(&db).with(parts);
    let declared = crate::resource::require_mounted::<ChromeResource>(&cx).unwrap();
    let wired = crate::panel::wired_table::<ChromeResource>(&cx).unwrap();
    assert!(
        wired.bulk_enabled(),
        "wired_table must attach the delete/bulk chrome the resource declares"
    );
    assert_eq!(
        wired.page_size(),
        declared.table.page_size(),
        "wired_table must keep the declared page size"
    );
}

#[test]
fn table_search_expr_ors_across_searchable_columns() {
    // distinct names — title + status, not one field twice.
    let tasks_table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)).searchable(),
        TextColumn::new(lens!(Task.status)).searchable(),
    ));
    assert!(tasks_table.search_expr("Ada").is_some());
    assert!(tasks_table.search_expr("").is_none());
    assert!(tasks_table.search_expr("   ").is_none());
    let table_none = Table::<User>::new(TextColumn::new(lens!(User.name)));
    assert!(table_none.search_expr("Ada").is_none());
}

#[test]
fn table_order_by_returns_first_sortable() {
    let tasks_table = Table::<Task>::new((
        TextColumn::new(lens!(Task.title)).sortable(),
        TextColumn::new(lens!(Task.status)),
    ));
    assert!(tasks_table.order_by(false).is_some());
    let table_none = Table::<User>::new(TextColumn::new(lens!(User.name)));
    assert!(table_none.order_by(false).is_none());
}

#[test]
fn paginate_records_a_zero_page_size() {
    let errors = Table::<User>::new(TextColumn::new(lens!(User.name)))
        .paginate(0)
        .declaration_errors();
    assert_eq!(errors, [DeclarationErrorKind::ZeroPageSize]);
}

#[test]
fn table_order_bys_single_sort_column() {
    let users_table = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable());
    let orders = users_table.order_bys_for(&TableState::default());
    assert_eq!(orders.len(), 1, "sortable column only, got {orders:?}");
    // No sortable column → the PK alone.
    let table_none = Table::<User>::new(TextColumn::new(lens!(User.name)));
    assert_eq!(
        table_none.order_bys_for(&TableState::default()).len(),
        1,
        "an unsorted table falls back to the PK"
    );
}

#[test]
fn order_bys_for_resolves_sort_param_with_fallbacks() {
    let sorted = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable()).paginate(25);

    // ?sort=name&dir=desc → name desc (toasty appends PK internally)
    let state = TableState {
        sort: Some(Sort {
            column: "name".to_string(),
            descending: true,
        }),
        ..TableState::default()
    };
    let orders = sorted.order_bys_for(&state);
    assert_eq!(orders.len(), 1, "sort column only, got {orders:?}");

    let state = TableState {
        sort: Some(Sort {
            column: "nope".to_string(),
            descending: false,
        }),
        ..TableState::default()
    };
    assert_eq!(sorted.order_bys_for(&state).len(), 1);

    assert_eq!(sorted.order_bys_for(&TableState::default()).len(), 1);

    // No sortable column → PK-only deterministic order
    let unsorted = Table::<User>::new(TextColumn::new(lens!(User.name))).paginate(25);
    let orders = unsorted.order_bys_for(&TableState::default());
    assert_eq!(
        orders.len(),
        1,
        "PK-only for paginated unsorted, got {orders:?}"
    );
}

#[tokio::test]
async fn table_page_round_trips_real_cursors() {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["Ada", "Bob", "Cara"] {
        toasty::create!(User { name }).exec(&mut db).await.unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let users_table = Table::<User>::new(TextColumn::new(lens!(User.name)).sortable());
    let mut db = crate::db::db(&cx);

    // Page 1 of 1-per-page.
    let page1 = users_table
        .order_bys_for(&TableState::default())
        .iter()
        .fold(User::all(), |q, ord| q.order_by(ord.clone()))
        .paginate(1)
        .exec(&mut db)
        .await
        .unwrap();
    let tp1 = TablePage::from_toasty_page(page1).unwrap();
    assert_eq!(tp1.rows.len(), 1);
    assert_eq!(tp1.rows[0].name, "Ada");
    let cursor = tp1.next_cursor.expect("full page has a next cursor");

    let tp1_decoded = crate::toasty_compat::cursor::decode(&cursor).unwrap();
    let page2 = User::all()
        .order_by(User::fields().name().asc())
        .paginate(1)
        .after(tp1_decoded)
        .exec(&mut db)
        .await
        .unwrap();
    let tp2 = TablePage::from_toasty_page(page2).unwrap();
    assert_eq!(tp2.rows[0].name, "Bob", "cursor must resume after Ada");
}

#[test]
fn unapplied_filters_flags_unknown_keys_and_rejected_values() {
    let tbl = status_table();
    assert!(tbl.unapplied_filters(&filters_state(&[])).is_empty());
    assert!(
        tbl.unapplied_filters(&filters_state(&[("status", "published")]))
            .is_empty(),
        "valid filter must apply"
    );
    assert_eq!(
        tbl.unapplied_filters(&filters_state(&[("stauts", "published")])),
        vec![("stauts:published".to_string(), "unknown filter".to_string())]
    );
    assert_eq!(
        tbl.unapplied_filters(&filters_state(&[("status", "Published")])),
        vec![("status:Published".to_string(), "invalid value".to_string())]
    );
}

#[test]
fn unapplied_filters_flags_dropped_filters() {
    let tbl = status_table();
    let too_many = std::iter::once("f.status=published".to_string())
        .chain((0..crate::table::state::MAX_FILTERS).map(|i| format!("f.k{i}=v")))
        .collect::<Vec<_>>()
        .join("&");
    let too_long = format!(
        "f.status={}",
        "a".repeat(crate::table::state::MAX_FILTER_LEN + 1)
    );
    for query in [too_many.as_str(), too_long.as_str(), "filters=status:draft"] {
        let state = TableState::from_query(query);
        assert!(
            tbl.unapplied_filters(&state)
                .iter()
                .any(|(pair, _)| pair == "dropped filters"),
            "the dropped filters of {query:.40} must be reported"
        );
    }
}

#[test]
fn ternary_all_is_a_neutral_noop_not_an_invalid_value() {
    let tbl = Table::<Task>::new(TextColumn::new(lens!(Task.title)))
        .filters(TernaryFilter::new(Task::fields().featured()));
    let state = filters_state(&[("featured", "all")]);
    assert!(
        tbl.filter_expr(&state).is_none(),
        "all must select no predicate"
    );
    assert!(
        tbl.unapplied_filters(&state).is_empty(),
        "all must not be flagged, got {:?}",
        tbl.unapplied_filters(&state)
    );
    // Genuine garbage still flags.
    assert_eq!(
        tbl.unapplied_filters(&filters_state(&[("featured", "maybe")])),
        vec![("featured:maybe".to_string(), "invalid value".to_string())]
    );
}

#[tokio::test]
async fn filter_banner_reports_unfiltered_when_nothing_applies() {
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let tbl = status_table();
    let render_banner = async |pairs: &[(&str, &str)]| {
        let page = crate::table::TablePage::<Task>::from(vec![]);
        tbl.render_with_state(&cx, page, &filters_state(pairs), "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx)
    };
    let html = render_banner(&[("status", "typo")]).await;
    assert!(
        html.contains("showing unfiltered results"),
        "invalid-only banner must admit unfiltered, got {html}"
    );
    let html = render_banner(&[("status", "published"), ("bogus", "x")]).await;
    assert!(
        html.contains("other filter(s) still apply"),
        "mixed banner keeps the tail, got {html}"
    );
}

struct NoColumns;

impl<M> IntoColumns<M> for NoColumns {
    fn into_columns(self) -> Vec<crate::table::column::BoxColumn<M>> {
        Vec::new()
    }
}

#[test]
fn empty_column_set_is_misdeclared() {
    let errors = Table::<User>::new(NoColumns).declaration_errors();
    assert_eq!(errors, [DeclarationErrorKind::NoColumns]);
}

#[test]
fn duplicate_column_name_is_misdeclared_on_field_computed_collision() {
    let errors = Table::<Task>::new((
        TextColumn::new(lens!(Task.status)).sortable(),
        ComputedColumn::new("Status", |t: &Task| t.status.clone()),
    ))
    .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::DuplicateColumn { .. })),
        "{errors:?}"
    );
}

#[test]
fn duplicate_column_name_is_misdeclared_on_case_only_computed_collision() {
    let errors = Table::<User>::new((
        ComputedColumn::new("Status", |u: &User| u.name.clone()),
        ComputedColumn::new("STATUS", |u: &User| u.name.clone()),
    ))
    .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::DuplicateColumn { .. })),
        "{errors:?}"
    );
}

#[test]
fn duplicate_column_name_is_misdeclared_on_duplicate_field() {
    let errors = Table::<User>::new((
        TextColumn::new(lens!(User.name)),
        TextColumn::new(lens!(User.name)),
    ))
    .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::DuplicateColumn { .. })),
        "{errors:?}"
    );
}

#[test]
fn duplicate_filter_name_is_misdeclared_on_duplicate_field() {
    let errors = Table::<Task>::new(TextColumn::new(lens!(Task.title)))
        .filters((
            SelectFilter::new(Task::fields().status(), vec!["published".to_string()]),
            SelectFilter::new(Task::fields().status(), vec!["draft".to_string()]),
        ))
        .declaration_errors();
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, DeclarationErrorKind::DuplicateFilter { .. })),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_misdeclared_table_fails_to_render() {
    let table = Table::<User>::new((
        TextColumn::new(lens!(User.name)),
        TextColumn::new(lens!(User.name)),
    ));
    let cx = CxTestBuilder::new().build();
    let page = crate::table::TablePage::<User>::from(vec![]);
    let Err(error) = table.render(&cx, page).await else {
        panic!("a misdeclared table must not render");
    };
    assert!(
        error.to_string().contains("two columns are named 'name'"),
        "a duplicate column must name the field, got {error}"
    );
}

async fn seeded_users(names: &[&str]) -> topcoat::context::Cx {
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in names {
        toasty::create!(User {
            name: name.to_string()
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    CxTestBuilder::new().app_context(db).build()
}

fn paged_users_table(per_page: usize) -> Table<User> {
    Table::<User>::new(TextColumn::new(lens!(User.name)).sortable()).paginate(per_page)
}

#[tokio::test]
async fn full_walk_reaches_every_row_exactly_once_without_phantoms() {
    let cx = seeded_users(&["u01", "u02", "u03", "u04", "u05"]).await;
    let tbl = paged_users_table(2);
    let query = || toasty::stmt::Query::<List<User>>::all();
    // Forward walk from the first page to exhaustion.
    let mut seen = Vec::new();
    let mut state = TableState::default();
    let mut last = TablePage::load(&cx, &tbl, query(), &state).await.unwrap();
    assert!(last.prev_cursor.is_none(), "first page has no prev");
    loop {
        seen.extend(last.rows.iter().map(|u| u.name.clone()));
        match last.next_cursor.clone() {
            Some(cursor) => {
                state = TableState {
                    cursor: Some(crate::table::Cursor::After(cursor)),
                    ..TableState::default()
                };
                last = TablePage::load(&cx, &tbl, query(), &state).await.unwrap();
            }
            None => break,
        }
    }
    assert_eq!(seen, vec!["u01", "u02", "u03", "u04", "u05"]);
    // Backward walk from the terminal page to the first.
    let mut back = vec![last.rows.iter().map(|u| u.name.clone()).collect::<Vec<_>>()];
    while let Some(cursor) = last.prev_cursor.clone() {
        state = TableState {
            cursor: Some(crate::table::Cursor::Before(cursor)),
            ..TableState::default()
        };
        last = TablePage::load(&cx, &tbl, query(), &state).await.unwrap();
        back.push(last.rows.iter().map(|u| u.name.clone()).collect::<Vec<_>>());
    }
    back.reverse();
    assert_eq!(
        back,
        vec![
            vec!["u01".to_string(), "u02".to_string()],
            vec!["u03".to_string(), "u04".to_string()],
            vec!["u05".to_string()],
        ]
    );
    assert!(last.prev_cursor.is_none(), "first page has no prev");
}

#[tokio::test]
async fn exact_boundary_pages_carry_exact_cursors() {
    let cx = seeded_users(&["u01", "u02", "u03", "u04"]).await;
    let tbl = paged_users_table(2);
    let query = || toasty::stmt::Query::<List<User>>::all();
    let first = TablePage::load(&cx, &tbl, query(), &TableState::default())
        .await
        .unwrap();
    assert_eq!(first.rows.len(), 2);
    let cursor = first.next_cursor.clone().expect("page 1 of 2 has a next");
    let state = TableState {
        cursor: Some(crate::table::Cursor::After(cursor)),
        ..TableState::default()
    };
    let second = TablePage::load(&cx, &tbl, query(), &state).await.unwrap();
    assert_eq!(
        second
            .rows
            .iter()
            .map(|u| u.name.clone())
            .collect::<Vec<_>>(),
        vec!["u03".to_string(), "u04".to_string()]
    );
    assert!(
        second.next_cursor.is_none(),
        "terminal full page must not offer a next page"
    );
    assert!(
        second.prev_cursor.is_some(),
        "second page must offer a prev page"
    );
}

/// Counts sqlite driver executions inside the `gh172-budget` marker span.
struct BudgetState {
    count: std::sync::atomic::AtomicUsize,
    next_span: std::sync::atomic::AtomicU64,
}

/// Marker span attributing driver execs to the budget measurement.
const BUDGET_SPAN: &str = "gh172-budget";

thread_local! {
    static BUDGET_MARKERS: std::cell::RefCell<std::collections::HashSet<u64>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
    static BUDGET_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

struct BudgetVisitor {
    driver: Option<String>,
    message: String,
}

impl tracing::field::Visit for BudgetVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "driver" {
            self.driver = Some(value.to_string());
        }
        self.record_debug(field, &format_args!("{value}"));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for BudgetState {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target().starts_with("toasty_driver_sqlite")
            || (metadata.is_span() && metadata.name() == BUDGET_SPAN)
    }

    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let id = self
            .next_span
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if span.metadata().name() == BUDGET_SPAN {
            BUDGET_MARKERS.with(|markers| {
                markers.borrow_mut().insert(id);
            });
        }
        tracing::span::Id::from_u64(id)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let in_scope = BUDGET_DEPTH.with(|depth| depth.get() > 0);
        if !in_scope {
            return;
        }
        let mut visitor = BudgetVisitor {
            driver: None,
            message: String::new(),
        };
        event.record(&mut visitor);
        if visitor.driver.as_deref() == Some("sqlite") && visitor.message.contains("driver exec") {
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn enter(&self, span: &tracing::span::Id) {
        let is_marker = BUDGET_MARKERS.with(|markers| markers.borrow().contains(&span.into_u64()));
        if is_marker {
            BUDGET_DEPTH.with(|depth| depth.set(depth.get() + 1));
        }
    }

    fn exit(&self, span: &tracing::span::Id) {
        let is_marker = BUDGET_MARKERS.with(|markers| markers.borrow().contains(&span.into_u64()));
        if is_marker {
            BUDGET_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
}

static BUDGET_INSTALL: std::sync::OnceLock<std::sync::Arc<BudgetState>> =
    std::sync::OnceLock::new();

/// Install the budget counter as the process-global default.
fn install_budget_counter() -> std::sync::Arc<BudgetState> {
    BUDGET_INSTALL
        .get_or_init(|| {
            let state = std::sync::Arc::new(BudgetState {
                count: std::sync::atomic::AtomicUsize::new(0),
                next_span: std::sync::atomic::AtomicU64::new(1),
            });
            tracing::subscriber::set_global_default(state.clone())
                .expect("budget counter installs once");
            tracing::callsite::rebuild_interest_cache();
            state
        })
        .clone()
}

#[tokio::test(flavor = "current_thread")]
async fn full_page_costs_main_plus_single_direction_probe() {
    use std::sync::atomic::Ordering;
    let cx = seeded_users(&["u01", "u02", "u03", "u04"]).await;
    let tbl = paged_users_table(2);
    let budget = install_budget_counter();
    let _scope = tracing::info_span!(BUDGET_SPAN).entered();
    let count_around = |reset: bool| {
        if reset {
            budget.count.store(0, Ordering::SeqCst);
        }
        budget.count.load(Ordering::SeqCst)
    };
    let ordered = || User::all().order_by(User::fields().name().asc());
    let mut db = crate::db::db(&cx);
    count_around(true);
    let bare_main = ordered().paginate(2).exec(&mut db).await.unwrap();
    let bare_main_cost = count_around(false);
    count_around(true);
    let probe_cursor = bare_main.next_cursor.clone().unwrap();
    ordered()
        .paginate(1)
        .after(probe_cursor)
        .exec(&mut db)
        .await
        .unwrap();
    let bare_probe_cost = count_around(false);
    assert!(
        bare_main_cost > 0 && bare_probe_cost > 0,
        "the counter must observe driver execs, got main={bare_main_cost} probe={bare_probe_cost}"
    );
    // Full first page: main + exactly one next probe.
    count_around(true);
    let first = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        count_around(false),
        bare_main_cost + bare_probe_cost,
        "full page must cost exactly main + one next probe"
    );
    assert_eq!(first.rows.len(), 2);
    // Short terminal page: main alone, no probe.
    let tbl3 = paged_users_table(3);
    count_around(true);
    let head = TablePage::load(
        &cx,
        &tbl3,
        toasty::stmt::Query::<List<User>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    assert_eq!(head.rows.len(), 3);
    let tail_state = TableState {
        cursor: head.next_cursor.clone().map(crate::table::Cursor::After),
        ..TableState::default()
    };
    count_around(true);
    let tail = TablePage::load(
        &cx,
        &tbl3,
        toasty::stmt::Query::<List<User>>::all(),
        &tail_state,
    )
    .await
    .unwrap();
    assert_eq!(tail.rows.len(), 1);
    assert!(tail.next_cursor.is_none());
    let short_cost = count_around(false);
    count_around(true);
    ordered().paginate(3).exec(&mut db).await.unwrap();
    let bare_short_cost = count_around(false);
    assert_eq!(
        short_cost, bare_short_cost,
        "short page must cost exactly one bare fetch (no probe)"
    );
    let p1 = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    let p2_state = TableState {
        cursor: p1.next_cursor.clone().map(crate::table::Cursor::After),
        ..TableState::default()
    };
    let p2 = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &p2_state,
    )
    .await
    .unwrap();
    assert_eq!(p2.rows.len(), 2);
    let back_to_first = TableState {
        cursor: p2.prev_cursor.clone().map(crate::table::Cursor::Before),
        ..TableState::default()
    };
    count_around(true);
    let first_again = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &back_to_first,
    )
    .await
    .unwrap();
    assert_eq!(
        first_again
            .rows
            .iter()
            .map(|u| u.name.clone())
            .collect::<Vec<_>>(),
        vec!["u01".to_string(), "u02".to_string()]
    );
    assert!(
        first_again.prev_cursor.is_none(),
        "backward landing on the first page must hide the phantom prev"
    );
    assert_eq!(
        count_around(false),
        bare_main_cost + bare_probe_cost,
        "backward landing must cost exactly main + one prev probe"
    );
}

#[tokio::test]
async fn stale_cursor_is_marked_for_retry() {
    use toasty::stmt::Value;
    use toasty_core::stmt::ValueRecord;

    let mut db = Db::builder()
        .models(toasty::models!(Task))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for title in ["Alpha", "Bravo", "Charlie", "Delta"] {
        toasty::create!(Task {
            title: title.to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: "2024-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = topcoat::context::CxTestBuilder::new()
        .app_context(db)
        .build();
    let table = || Table::<Task>::new(TextColumn::new(lens!(Task.title)).sortable()).paginate(2);
    // The query orders by `title` then the PK to break ties, so a cursor
    // with three fields has one too many.
    let wide = crate::toasty_compat::cursor::encode(&Value::Record(ValueRecord::from_vec(vec![
        Value::String("Alpha".to_string()),
        Value::String("x".to_string()),
        Value::I64(1),
    ])))
    .unwrap();
    let state = TableState {
        cursor: Some(crate::table::Cursor::After(wide)),
        ..TableState::default()
    };
    let error = TablePage::load(
        &cx,
        &table(),
        toasty::stmt::Query::<List<Task>>::all(),
        &state,
    )
    .await
    .expect_err("a cursor with too many fields must fail the load");
    assert!(
        crate::error::TabloError::is_cursor(&error),
        "a rejected cursor must carry the cursor marker, got {error}"
    );
    assert!(
        matches!(
            crate::error::TabloError::of(&error),
            Some(crate::error::TabloError::CursorRejected(_))
        ),
        "the refusal is not a decode failure, got {error}"
    );

    // A transient failure keeps the cursor.
    let transient = crate::error::unavailable("connection reset");
    assert!(
        !crate::error::TabloError::is_cursor(&transient),
        "only cursor failures drop pagination on retry"
    );

    // A cursor cut from this query's own ordering round-trips.
    let first = TablePage::load(
        &cx,
        &table(),
        toasty::stmt::Query::<List<Task>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    let state = TableState {
        cursor: first.next_cursor.clone().map(crate::table::Cursor::After),
        ..TableState::default()
    };
    assert!(
        TablePage::load(
            &cx,
            &table(),
            toasty::stmt::Query::<List<Task>>::all(),
            &state
        )
        .await
        .is_ok(),
        "a matching cursor must keep loading"
    );
}
