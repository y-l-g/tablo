use toasty::{Db, stmt::List};
use topcoat::context::{Cx, CxTestBuilder};

use super::*;
use crate::{
    resource::{Resource, SelectFilter, Sort, TablePage, TableState, TernaryFilter, TextColumn},
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

fn status_table(_cx: &Cx) -> Table<Task> {
    Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .filters(SelectFilter::r#for(
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
    let col = TextColumn::r#for(User::fields().name(), |u| u.name.clone()).searchable();
    let expr = col.to_search_expr("Ada").unwrap();
    let mut db = crate::db::db(&cx);
    let rows = User::filter(expr).exec(&mut db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Ada");
    // Empty term → None
    assert!(col.to_search_expr("").is_none());
    assert!(col.to_search_expr("   ").is_none());
}

/// The panel's page-owned seam must attach the chrome the resource
/// declares. `bulk_enabled` is the witness and is private to this module,
/// which is why the test lives here.
#[tokio::test]
async fn wired_table_carries_the_declared_action_chrome() {
    struct ChromeResource;
    impl Resource for ChromeResource {
        type Model = User;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &Cx) -> Table<User> {
            Table::new(
                |u: &User| u.id.to_string(),
                TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
            )
            .paginate(25)
        }

        fn can_delete_any(_cx: &Cx) -> bool {
            true
        }
    }

    let db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // The wiring derives the action URLs from the request path, so the Cx
    // needs one; a bare builder has no request for `panel_prefix` to read.
    let parts = http::Request::builder()
        .uri("/admin/dummies")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    let cx = CxTestBuilder::new()
        .app_context(db)
        .request_context(parts)
        .build();
    // The declaration alone carries no chrome: `Resource::table` is bare,
    // so a table that renders action links comes from the panel's wiring.
    assert!(!ChromeResource::table(&cx).bulk_enabled());
    let wired = crate::panel::wired_table::<ChromeResource>(&cx);
    assert!(
        wired.bulk_enabled(),
        "wired_table must attach the delete/bulk chrome the resource declares"
    );
    assert_eq!(
        wired.page_size(),
        ChromeResource::table(&cx).page_size(),
        "wired_table must keep the declared page size"
    );
}

#[tokio::test]
async fn table_load_rejects_both_cursors() {
    // `?after=` + `?before=` together must fail loudly instead of
    // silently preferring `after` (the fail-open family). The
    // failure carries the `CursorDecodeError` marker so the retry link
    // drops pagination.
    let mut db = Db::builder()
        .models(toasty::models!(User))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["Ada", "Bob"] {
        toasty::create!(User {
            name: name.to_string()
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let tbl = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
    )
    .paginate(1);
    // A valid cursor token: the first page of two rows has a next page.
    let first = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    let cursor = first
        .next_cursor
        .clone()
        .expect("page 1 must have a cursor");
    // Sanity: a single cursor still loads.
    let state = TableState {
        after: Some(cursor.clone()),
        ..TableState::default()
    };
    let second = TablePage::load(&cx, &tbl, toasty::stmt::Query::<List<User>>::all(), &state)
        .await
        .unwrap();
    assert_eq!(second.rows.len(), 1);
    // Both cursors together fail with the cursor marker — no silent
    // precedence for whichever comes first.
    let state = TableState {
        after: Some(cursor.clone()),
        before: Some(cursor),
        ..TableState::default()
    };
    let err = TablePage::load(&cx, &tbl, toasty::stmt::Query::<List<User>>::all(), &state)
        .await
        .expect_err("after+before must fail loudly");
    assert!(
        err.downcast_ref::<crate::cursor::CursorDecodeError>()
            .is_some(),
        "conflict must carry the cursor marker for the retry contract, got {err}"
    );
}

#[test]
fn table_search_expr_ors_across_searchable_columns() {
    // distinct names — title + status, not one field twice.
    let tasks_table = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().title(), |t| t.title.clone()).searchable(),
            TextColumn::r#for(Task::fields().status(), |t| t.status.clone()).searchable(),
        ),
    );
    assert!(tasks_table.search_expr("Ada").is_some());
    assert!(tasks_table.search_expr("").is_none());
    assert!(tasks_table.search_expr("   ").is_none());
    let table_none = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    assert!(table_none.search_expr("Ada").is_none());
}

#[test]
fn table_order_by_returns_first_sortable() {
    // distinct names — title sortable + status plain.
    let tasks_table = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().title(), |t| t.title.clone()).sortable(),
            TextColumn::r#for(Task::fields().status(), |t| t.status.clone()),
        ),
    );
    assert!(tasks_table.order_by(false).is_some());
    let table_none = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    assert!(table_none.order_by(false).is_none());
}

/// A page of no rows is a misdeclaration, refused where it is written: the
/// panel calls `Resource::table` at build, so this surfaces at boot.
#[test]
#[should_panic(expected = "a page size must be at least 1")]
fn paginate_refuses_a_zero_page_size() {
    let _ = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
    )
    .paginate(0);
}

#[test]
fn table_order_bys_single_sort_column() {
    let users_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
    );
    let orders = users_table.order_bys_for(&TableState::default());
    // Single sortable column, no app-level PK suffix — toasty's engine
    // appends the physical PK columns to ambiguous cursor orderings
    // internally.
    assert_eq!(orders.len(), 1, "sortable column only, got {orders:?}");
    // No sortable column → the PK alone, the deterministic order cursor
    // pagination needs.
    let table_none = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    assert_eq!(
        table_none.order_bys_for(&TableState::default()).len(),
        1,
        "an unsorted table falls back to the PK"
    );
}

#[test]
fn order_bys_for_resolves_sort_param_with_fallbacks() {
    let sorted = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
    )
    .paginate(25);

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

    // Unknown sort column → declared default (name asc)
    let state = TableState {
        sort: Some(Sort {
            column: "nope".to_string(),
            descending: false,
        }),
        ..TableState::default()
    };
    assert_eq!(sorted.order_bys_for(&state).len(), 1);

    // No sort at all → declared default
    assert_eq!(sorted.order_bys_for(&TableState::default()).len(), 1);

    // No sortable column → PK-only deterministic order
    let unsorted = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    )
    .paginate(25);
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
    let users_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
    );
    let mut db = crate::db::db(&cx);

    // Page 1 of 1-per-page: full page → real next cursor.
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

    // The encoded cursor resumes the walk without skipping tied rows.
    let tp1_decoded = crate::cursor::decode(&cursor).unwrap();
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

#[tokio::test]
async fn table_renders_inside_the_boundary_region() {
    // Core owns the boundary contract; the showcase owns HTTP wiring, and
    // the topcoat `#[memoize]` half stays upstream. The region is
    // unconditional, so the table always lands where a morph can swap it.
    use topcoat::view::ViewExt;

    let cx = CxTestBuilder::new().build();
    let table = Table::<User>::new(
        |u: &User| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
    );

    let page = crate::resource::TablePage::<User>::from(vec![]);
    let html = table
        .render(&cx, page)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let boundary_at = html
        .find("data-boundary=\"table\"")
        .unwrap_or_else(|| panic!("the table must render inside the morph boundary, got {html}"));
    let root_at = html
        .find("data-table-root=\"\"")
        .unwrap_or_else(|| panic!("the swapped table must carry its table root, got {html}"));
    assert!(
        boundary_at < root_at && !html[boundary_at..root_at].contains("</div>"),
        "the boundary must wrap the table root, got {html}"
    );
}

#[test]
fn unapplied_filters_flags_unknown_keys_and_rejected_values() {
    // typo'd keys and allowlist-missed values must be visible,
    // never silently unfiltered.
    let cx = CxTestBuilder::new().build();
    let tbl = status_table(&cx);
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
fn unapplied_filters_flags_a_refused_filters_transport() {
    // an oversized `?filters=` is refused whole rather than
    // partially applied, and it reads as its own reason — not as a
    // malformed segment — so the list banner explains itself and the
    // export's 400 is the fail-closed guard instead of a silent drop.
    let cx = CxTestBuilder::new().build();
    let tbl = status_table(&cx);
    let huge = format!("status:published,{}", "k:v,".repeat(2 * 1024 * 1024));
    let state = TableState::from_live_args("", &huge, "", "", "");
    assert!(
        state.filters.is_empty() && tbl.filter_expr(&state).is_none(),
        "the refused transport must apply no predicate"
    );
    assert_eq!(
        tbl.unapplied_filters(&state),
        vec![(
            "filters=overflow".to_string(),
            "too many filters: refused whole (GH #205)".to_string()
        )]
    );
}

#[test]
fn ternary_all_is_a_neutral_noop_not_an_invalid_value() {
    // `all` is the documented TernaryFilter no-op — it selects
    // no predicate AND is never flagged, so the list shows no warning
    // and the export (which refuses on any unapplied filter) stays 200.
    let tbl = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
    )
    .filters(TernaryFilter::r#for(Task::fields().featured()));
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
    // an invalid-only request applies no predicate, so the
    // banner must say "showing unfiltered results" — "other filter(s)
    // still apply" would be the lie. Mixed valid+invalid keeps the other
    // filters.
    use topcoat::view::ViewExt;
    let cx = CxTestBuilder::new().build();
    let tbl = status_table(&cx);
    let render_banner = async |pairs: &[(&str, &str)]| {
        let page = crate::resource::TablePage::<Task>::from(vec![]);
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
        "mixed banner keeps the GH #148 tail, got {html}"
    );
}

/// A column source that yields none: the one way to reach the
/// constructor's column guard now that every shipped [`IntoColumns`] impl
/// yields at least one column.
struct NoColumns;

impl<M> IntoColumns<M> for NoColumns {
    fn into_columns(self) -> Vec<TextColumn<M>> {
        Vec::new()
    }
}

#[test]
#[should_panic(expected = "at least one column")]
fn empty_column_set_panics_at_the_constructor() {
    let _ = Table::<User>::new(|u| u.id.to_string(), NoColumns);
}

#[test]
#[should_panic(expected = "duplicate column name")]
fn duplicate_column_name_panics_on_field_computed_collision() {
    // computed("Status") derives name "status", colliding with
    // the field column's name — the TextColumn::name namespace must stay
    // unique even though computeds are never sortable today.
    let _ = Table::<Task>::new(
        |t| t.id.to_string(),
        (
            TextColumn::r#for(Task::fields().status(), |t: &Task| t.status.clone()).sortable(),
            TextColumn::computed("Status", |t: &Task| t.status.clone()),
        ),
    );
}

#[test]
#[should_panic(expected = "duplicate column name")]
fn duplicate_column_name_panics_on_case_only_computed_collision() {
    // computed names are label.to_lowercase(), so labels
    // differing only by case still collide.
    let _ = Table::<User>::new(
        |u| u.id.to_string(),
        (
            TextColumn::computed("Status", |u: &User| u.name.clone()),
            TextColumn::computed("STATUS", |u: &User| u.name.clone()),
        ),
    );
}

#[test]
#[should_panic(expected = "duplicate column name")]
fn duplicate_column_name_panics_on_duplicate_field() {
    // same guard covers two bindings of one field.
    let _ = Table::<User>::new(
        |u| u.id.to_string(),
        (
            TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
            TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
        ),
    );
}

#[test]
#[should_panic(expected = "duplicate filter name")]
fn duplicate_filter_name_panics_on_duplicate_field() {
    // the transport names a filter by its field, and the parser
    // keeps the first value for a duplicated key, so two filters on one
    // field would silently drop one. Refuse the declaration instead.
    let _ = Table::<Task>::new(
        |t| t.id.to_string(),
        TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
    )
    .filters((
        SelectFilter::r#for(Task::fields().status(), vec!["published".to_string()]),
        SelectFilter::r#for(Task::fields().status(), vec!["draft".to_string()]),
    ));
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
    Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()).sortable(),
    )
    .paginate(per_page)
}

#[tokio::test]
async fn full_walk_reaches_every_row_exactly_once_without_phantoms() {
    // prev/next existence must be exact at every boundary — no
    // phantom links to empty pages, and no skipped rows. A `LIMIT
    // per_page+1` fold with the extra row trimmed would anchor the next
    // link past the extra row (the engine derives cursors from the last
    // *fetched* row), dropping every `(per_page+1)`th row from forward
    // walks — this walk fails loudly if that ever lands.
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
                    after: Some(cursor),
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
            before: Some(cursor),
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
    // a full page sitting exactly at the boundary (4 rows,
    // `paginate(2)`) must report no next page — the engine's optimistic
    // `next_cursor` alone would be a phantom link to an empty page.
    let cx = seeded_users(&["u01", "u02", "u03", "u04"]).await;
    let tbl = paged_users_table(2);
    let query = || toasty::stmt::Query::<List<User>>::all();
    let first = TablePage::load(&cx, &tbl, query(), &TableState::default())
        .await
        .unwrap();
    assert_eq!(first.rows.len(), 2);
    let cursor = first.next_cursor.clone().expect("page 1 of 2 has a next");
    let state = TableState {
        after: Some(cursor),
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
/// Tracing caches per-callsite interest globally at first use: a sibling
/// test executing first pins the driver's callsite as `never`, after
/// which no thread-local subscriber can observe it. So the budget test
/// installs this as the *global* default once (registration then sticks
/// at `always`) and attributes execs by span — sibling tests' execs fall
/// outside the marker span and are ignored.
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

/// Install the budget counter as the process-global default (once) and
/// hand back its handle. Later interest-cache state cannot regress: no
/// other subscriber exists in this binary, so the driver's callsite stays
/// `always` from here on.
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
    // a full page costs the main fetch plus exactly one `LIMIT
    // 1` existence probe — next on forward/first landings, prev on
    // backward landings (each direction probes only the edge that can
    // lie). A short forward page costs the main fetch alone. Counts are
    // calibrated in-test against bare toasty execs, so no
    // engine-internal constant is pinned. `current_thread`: the marker
    // span is entered and polled on one thread (no hops), so the
    // thread-local attribution below holds.
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
    // Baselines: one bare main-shaped exec and one bare probe-shaped exec.
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
    // Short terminal page: main alone, no probe (paginate(3) over 4
    // rows ends on a 1-row page).
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
        after: head.next_cursor.clone(),
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
    // Backward landing on a full page: main + exactly one prev probe
    // (pp=2 table: page 2 [u03,u04], then back to full page 1).
    let p1 = TablePage::load(
        &cx,
        &tbl,
        toasty::stmt::Query::<List<User>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    let p2_state = TableState {
        after: p1.next_cursor.clone(),
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
        before: p2.prev_cursor.clone(),
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
    // a token cut from another ordering decodes but the engine
    // refuses the statement (the cursor's field count no longer matches
    // the query's `ORDER BY`). That failure is the cursor's, so it carries
    // a cursor marker and the retry drops pagination instead of repeating
    // the identical failing request forever.
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
    let table = || {
        Table::<Task>::new(
            |t| t.id.to_string(),
            TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()).sortable(),
        )
        .paginate(2)
    };
    // The query orders by `title` then the PK to break ties, so a cursor
    // with three fields has one too many.
    let wide = crate::cursor::encode(&Value::Record(ValueRecord::from_vec(vec![
        Value::String("Alpha".to_string()),
        Value::String("x".to_string()),
        Value::I64(1),
    ])))
    .unwrap();
    let state = TableState {
        after: Some(wide),
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
        crate::cursor::is_cursor_error(&error),
        "a rejected cursor must carry the cursor marker, got {error}"
    );
    assert!(
        error
            .downcast_ref::<crate::cursor::CursorRejectedError>()
            .is_some(),
        "the refusal is not a decode failure, got {error}"
    );

    // A transient failure keeps the cursor: a failure the cursor
    // did not cause carries no marker, so `retry_url_for_error` keeps the
    // pagination it was given.
    let transient = topcoat::Error::from(std::io::Error::other("database unavailable"));
    assert!(
        !crate::cursor::is_cursor_error(&transient),
        "only cursor failures drop pagination on retry"
    );

    // A cursor cut from this query's own ordering round-trips: the guard
    // marks a rejected cursor, not every request that carries one.
    let first = TablePage::load(
        &cx,
        &table(),
        toasty::stmt::Query::<List<Task>>::all(),
        &TableState::default(),
    )
    .await
    .unwrap();
    let state = TableState {
        after: first.next_cursor.clone(),
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
