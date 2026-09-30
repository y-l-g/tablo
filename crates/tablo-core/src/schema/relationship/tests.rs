use toasty::stmt::{List, Query};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::*,
};

use super::*;
use crate::schema::{Field, FieldLens, tree::Mode};

/// What a submit reports for one choice: its rules, then — when they pass —
/// its option existence, as `Schema::validate_async` asks per field.
async fn check(field: &Field, cx: &Cx, value: &str) -> Vec<String> {
    let errors = field.validate(value);
    if !errors.is_empty() {
        return errors;
    }
    field.validate_exists(cx, value).await
}
/// Related-source fixtures shared by the option-policy tests.
///
/// Each one implements only the [`OptionSource`] surface its test reads —
/// no `Resource`, no `Table`. `tenant_id` is optional so the
/// fixtures that do not care about tenancy keep creating rows without one;
/// `TenantScopedAuthors` needs a discoverable `tenant_id` column
/// for the framework to derive its scope from, and the test that uses it
/// creates its row with a tenant.
#[derive(Debug, toasty::Model, Clone)]
struct PolicyAuthor {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: Option<uuid::Uuid>,
    name: String,
}

/// The search fixtures' one declared search expression: a substring `LIKE`
/// over the model's `name` column, which is all the loaders ask a source
/// for. A real resource answers it from its `Table`'s `searchable()`
/// columns; that spelling is `Table::search_expr`'s own test.
fn name_search_expr<M: toasty::schema::Model>(
    path: FieldLens<M, String>,
    term: &str,
) -> Option<Expr<bool>> {
    Some(path.like_with_escape(format!("%{term}%"), '\\'))
}

/// Denies every request: the whole load fails closed (the trait's
/// `can_view_any` default).
struct DenyAllAuthors;
impl OptionSource for DenyAllAuthors {
    type Model = PolicyAuthor;
    fn scoped_query(_cx: &Cx) -> Result<Query<List<PolicyAuthor>>> {
        Ok(Query::all())
    }
}

struct HideOneAuthor;
impl OptionSource for HideOneAuthor {
    type Model = PolicyAuthor;
    fn scoped_query(_cx: &Cx) -> Result<Query<List<PolicyAuthor>>> {
        Ok(Query::all())
    }
    fn can_view_any(_cx: &Cx) -> bool {
        true
    }
    fn can_view(_cx: &Cx, record: &PolicyAuthor) -> bool {
        record.name != "Hidden"
    }
}

struct TenantScopedAuthors;
impl OptionSource for TenantScopedAuthors {
    type Model = PolicyAuthor;

    /// Mirrors `Resource`'s blanket impl (`resource::scoped_query`), which
    /// `schema` cannot call without re-creating the cycle.
    fn scoped_query(cx: &Cx) -> Result<Query<List<PolicyAuthor>>> {
        let tenant = crate::tenancy::require_tenant(cx)?;
        let filter = crate::tenancy::derived_tenant_filter::<PolicyAuthor>(tenant)
            .expect("PolicyAuthor declares a tenant_id column");
        Ok(Query::all().filter(filter))
    }

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }
    fn can_view(_cx: &Cx, _record: &PolicyAuthor) -> bool {
        true
    }
    fn requires_tenant() -> bool {
        true
    }
}

#[tokio::test]
async fn relationship_loader_fails_past_option_cap() {
    #[derive(Debug, toasty::Model, Clone)]
    struct RefAuthor {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct RefAuthorSource;
    impl OptionSource for RefAuthorSource {
        type Model = RefAuthor;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<RefAuthor>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &RefAuthor) -> bool {
            true
        }
    }
    #[derive(Debug, toasty::Model)]
    struct RefPost {
        #[key]
        #[auto]
        id: uuid::Uuid,
        author_id: uuid::Uuid,
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(RefAuthor, RefPost))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..(MAX_RELATIONSHIP_OPTIONS + 1) {
        toasty::create!(RefAuthor {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(RefPost::fields().author_id()).relationship::<RefAuthorSource>(
        |_cx| Query::all(),
        |a: &RefAuthor| a.id,
        |a: &RefAuthor| a.name.clone(),
    );
    // Over the cap: bounded work, visible retry error — never an
    // empty-options passthrough.
    let errs = check(&select, &cx, "whatever").await;
    assert!(
        errs.iter().any(|e| e.contains("could not load options")),
        "overflow must surface retry error, got {errs:?}"
    );
    // An overflowed load keeps the stored FK selectable: a
    // failed load must not blank the relation into a required-error.
    let html = select
        .render(&cx, Some("stored-fk"), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("value=\"stored-fk\""),
        "over-cap render must keep the stored value: {html}"
    );
}

#[tokio::test]
async fn relationship_option_values_are_primary_keys_not_table_ids() {
    // the table key is a display projection — option
    // values must come from the record's typed PK, or a display string
    // silently stores a label in the FK column. The source surface has no
    // table row-key projection to reach for at all, so the
    // caller's typed projection is the only option-value seam.
    #[derive(Debug, toasty::Model, Clone)]
    struct RefAuthor {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct RefAuthorSource;
    impl OptionSource for RefAuthorSource {
        type Model = RefAuthor;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<RefAuthor>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &RefAuthor) -> bool {
            true
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(RefAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(RefAuthor {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(RefAuthor::fields().name()).relationship::<RefAuthorSource>(
        |_cx| Query::all(),
        |a: &RefAuthor| a.id,
        |a: &RefAuthor| a.name.clone(),
    );
    // The PK validates; the label never does.
    assert!(check(&select, &cx, &pk).await.is_empty());
    assert_eq!(
        check(&select, &cx, "Ada").await,
        vec!["Name is invalid".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains(&format!("value=\"{pk}\"")),
        "option value must be the PK, got {html}"
    );
    assert!(
        !html.contains("value=\"Ada\""),
        "the label leaked into option values: {html}"
    );
}

#[tokio::test]
async fn relationship_load_fails_closed_when_can_view_any_denies() {
    // a related source that denies `can_view_any` must not
    // leak labels or ids through a dependent form, and the error must be
    // "not available" — retrying cannot fix a permission decision.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(PolicyAuthor {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<DenyAllAuthors>(
        |_cx| Query::all(),
        |a: &PolicyAuthor| a.id,
        |a: &PolicyAuthor| a.name.clone(),
    );
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("Ada"),
        "denied labels must not render: {html}"
    );
    assert!(
        !html.contains(&pk),
        "denied values must not render either: {html}"
    );
    // The empty select must explain itself on GET (no incoming error):
    // otherwise the user sees an unrequireable field with no reason.
    assert!(
        html.contains("Id is not available"),
        "denied render must show the form-level error: {html}"
    );
}

#[tokio::test]
async fn relationship_load_denies_tenantless_requests_for_tenant_scoped_targets() {
    // option loads are another path into the related
    // resource's rows; a tenant-scoped related resource must not serve
    // unscoped options just because the parent form is reachable without a
    // tenant, and the derived filter must narrow the load to the request
    // tenant's rows.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let tenant = uuid::Uuid::new_v4();
    let row = toasty::create!(PolicyAuthor {
        tenant_id: Some(tenant),
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<TenantScopedAuthors>(
        |_cx| Query::all(),
        |a: &PolicyAuthor| a.id,
        |a: &PolicyAuthor| a.name.clone(),
    );
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    // A resolved tenant that owns the row loads normally (a separate
    // memoize key too).
    let tenanted = cx.with(crate::tenancy::Tenant(tenant));
    assert!(check(&select, &tenanted, &pk).await.is_empty());
    // Another tenant's request sees nothing: the option load runs the
    // source's `scoped_query` — the fixture spells it as the framework's
    // derived tenant filter, and `Resource`'s blanket impl spells it as
    // `scoped_query::<R>` — never an unscoped base. The load itself
    // succeeds (the source is viewable), so the row is *absent from the
    // options* rather than denied — "invalid", the empty-set answer, not
    // the "not available" the gate gives.
    let foreign = cx.with(crate::tenancy::Tenant(uuid::Uuid::new_v4()));
    assert_eq!(
        check(&select, &foreign, &pk).await,
        vec!["Id is invalid".to_string()]
    );
}

#[tokio::test]
async fn relationship_load_filters_rows_by_can_view() {
    // `can_view`-denied rows are absent from options and
    // validation — a value outside the viewable set is invalid, not
    // merely unlisted.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let visible = toasty::create!(PolicyAuthor {
        name: "Visible".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let hidden = toasty::create!(PolicyAuthor {
        name: "Hidden".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>(
        |_cx| Query::all(),
        |a: &PolicyAuthor| a.id,
        |a: &PolicyAuthor| a.name.clone(),
    );
    assert!(
        check(&select, &cx, &visible.id.to_string())
            .await
            .is_empty()
    );
    assert_eq!(
        check(&select, &cx, &hidden.id.to_string()).await,
        vec!["Id is invalid".to_string()]
    );
    let html = select
        .render(&cx, Some(&visible.id.to_string()), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Visible"), "viewable row must render: {html}");
    assert!(
        !html.contains("Hidden"),
        "can_view-denied row must not render: {html}"
    );
}

#[tokio::test]
async fn relationship_cap_counts_raw_rows_not_viewable_ones() {
    // GH #91 + #108: the cap is checked on the raw bounded fetch. If it
    // counted post-`can_view` rows, a single hidden record would defeat
    // it and silently truncate a larger table, misreporting viewable FKs
    // as "invalid" — the exact failure the cap exists to prevent.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mut hidden_pk = String::new();
    // One past the cap, with one hidden row: the raw fetch overflows
    // even though the filtered count would fit.
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        let name = if i == 0 {
            "Hidden".to_string()
        } else {
            format!("author-{i}")
        };
        let row = toasty::create!(PolicyAuthor { name })
            .exec(&mut db)
            .await
            .unwrap();
        if i == 0 {
            hidden_pk = row.id.to_string();
        }
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>(
        |_cx| Query::all(),
        |a: &PolicyAuthor| a.id,
        |a: &PolicyAuthor| a.name.clone(),
    );
    // The raw fetch sees MAX+1 rows: overflow fails visibly instead of
    // rendering the 200 viewable rows as if they were the whole table.
    assert_eq!(
        check(&select, &cx, &hidden_pk).await,
        vec!["Id could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_can_view_filtering_out_every_row_yields_invalid() {
    // `can_view` filtering happens before labels render, so a
    // row the user may not view is absent from options and does not
    // validate — and the stored value is not re-rendered on the form.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let hidden = toasty::create!(PolicyAuthor {
        name: "Hidden".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = hidden.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>(
        |_cx| Query::all(),
        |a: &PolicyAuthor| a.id,
        |a: &PolicyAuthor| a.name.clone(),
    );
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is invalid".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        !html.contains("Hidden") && !html.contains(&pk),
        "filtered-out stored value must not render: {html}"
    );
}

#[tokio::test]
async fn relationship_options_share_one_load_per_request_and_tenant() {
    // selects over one source share a single bounded load per
    // (request, tenant) — validate and re-render share the one load.
    use std::sync::atomic::{AtomicUsize, Ordering};

    static OPTION_LOADS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Clone, toasty::Model)]
    struct Ref {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct CountingSource;
    impl OptionSource for CountingSource {
        type Model = Ref;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<Ref>>> {
            OPTION_LOADS.fetch_add(1, Ordering::SeqCst);
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Ref) -> bool {
            true
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(Ref))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Ref {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let id = row.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();

    // Two selects, different labels, same source.
    let s1 = Field::choice(Ref::fields().name()).relationship::<CountingSource>(
        |_cx| Query::all(),
        |r: &Ref| r.id,
        |r: &Ref| r.name.clone(),
    );
    let s2 = Field::choice(Ref::fields().name()).relationship::<CountingSource>(
        |_cx| Query::all(),
        |r: &Ref| r.id,
        |r: &Ref| format!("{}!", r.name),
    );

    OPTION_LOADS.store(0, Ordering::SeqCst);
    assert!(check(&s1, &cx, &id).await.is_empty());
    assert!(check(&s2, &cx, &id).await.is_empty());
    let _ = s1
        .render(&cx, Some(&id), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert_eq!(
        OPTION_LOADS.load(Ordering::SeqCst),
        1,
        "two selects + re-render must share one load"
    );

    // Same cache, other tenant → separate load (no cross-tenant sharing).
    let cx_b = cx.with(crate::tenancy::Tenant(uuid::Uuid::new_v4()));
    assert!(check(&s1, &cx_b, &id).await.is_empty());
    assert_eq!(
        OPTION_LOADS.load(Ordering::SeqCst),
        2,
        "a second tenant must not reuse the first tenant's options"
    );
}

#[tokio::test]
async fn relationship_overflow_is_distinct_from_load_failed() {
    // GH #150 D3: over-cap is `Overflow`, not `LoadFailed`, so searchable
    // selects degrade to type-to-search while DB errors stay retryable.

    #[derive(Debug, toasty::Model, Clone)]
    struct BigRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct BigRefSource;
    impl OptionSource for BigRefSource {
        type Model = BigRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<BigRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &BigRef) -> bool {
            true
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(BigRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(BigRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let tenant = crate::tenancy::tenant_id(&cx);
    let err = super::related_records::<BigRefSource>(&cx, tenant)
        .await
        .unwrap_err();
    assert_eq!(err, &super::OptionLoadError::Overflow);
    // Non-searchable keeps the retry message.
    let plain = Field::choice(BigRef::fields().name()).relationship::<BigRefSource>(
        |_cx| Query::all(),
        |r: &BigRef| r.id,
        |r: &BigRef| r.name.clone(),
    );
    assert_eq!(
        check(&plain, &cx, "whatever-not-a-uuid").await,
        vec!["Name could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_search_narrows_past_the_cap() {
    // GH #150 D1: `related_records_search` reuses the source's declared
    // search expression — a 201-row table overflows unfiltered but a
    // distinctive term returns its bounded match.

    #[derive(Debug, toasty::Model, Clone)]
    struct SearchRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct SearchRefSource;
    impl OptionSource for SearchRefSource {
        type Model = SearchRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<SearchRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &SearchRef) -> bool {
            true
        }
        fn search_expr(_cx: &Cx, term: &str) -> Option<Expr<bool>> {
            name_search_expr(SearchRef::fields().name(), term)
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(SearchRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(SearchRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let unique = toasty::create!(SearchRef {
        name: "Zebra Unique".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let tenant = crate::tenancy::tenant_id(&cx);
    // Unfiltered overflows (201 rows).
    let err = super::related_records::<SearchRefSource>(&cx, tenant)
        .await
        .unwrap_err();
    assert_eq!(err, &super::OptionLoadError::Overflow);
    // Distinctive term narrows to one.
    let rows = super::related_records_search::<SearchRefSource>(&cx, "Zebra".to_string())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Zebra Unique");
    // Empty q is the bounded head → still overflows on this table.
    let err = super::related_records_search::<SearchRefSource>(&cx, "".to_string())
        .await
        .unwrap_err();
    assert_eq!(err, super::OptionLoadError::Overflow);
    // `Select::search_options` shares the same seam.
    let select = Field::choice(SearchRef::fields().name())
        .searchable()
        .relationship::<SearchRefSource>(
            |_cx| Query::all(),
            |r: &SearchRef| r.id,
            |r: &SearchRef| r.name.clone(),
        );
    let opts = select
        .as_choice()
        .expect("a choice")
        .search_options(&cx, "Zebra")
        .await
        .unwrap();
    assert_eq!(opts.len(), 1);
    assert_eq!(opts[0].1, "Zebra Unique");
    assert_eq!(opts[0].0, unique.id.to_string());
}

#[tokio::test]
async fn relationship_search_without_searchable_falls_back_to_cap() {
    // GH #150 D1a: no searchable columns → unfiltered bounded load, which
    // overflows large tables instead of silently truncating.

    #[derive(Debug, toasty::Model, Clone)]
    struct PlainRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    /// Declares no search expression (the trait's `None` default), which is
    /// what a resource with no `searchable()` column answers.
    struct PlainRefSource;
    impl OptionSource for PlainRefSource {
        type Model = PlainRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<PlainRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &PlainRef) -> bool {
            true
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PlainRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(PlainRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let err = super::related_records_search::<PlainRefSource>(&cx, "author-1".to_string())
        .await
        .unwrap_err();
    assert_eq!(err, super::OptionLoadError::Overflow);
}

#[tokio::test]
async fn relationship_overflowed_searchable_validates_via_targeted_check() {
    // GH #150 D4: searchable selects over overflowed tables validate
    // legitimate FKs via the targeted PK check, not membership.

    #[derive(Debug, toasty::Model, Clone)]
    struct CheckRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct CheckRefSource;
    impl OptionSource for CheckRefSource {
        type Model = CheckRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<CheckRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, record: &CheckRef) -> bool {
            record.name != "Hidden"
        }
        fn search_expr(_cx: &Cx, term: &str) -> Option<Expr<bool>> {
            name_search_expr(CheckRef::fields().name(), term)
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(CheckRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mut visible_pk = String::new();
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        let name = if i == 0 {
            "Hidden".to_string()
        } else {
            format!("author-{i}")
        };
        let row = toasty::create!(CheckRef { name })
            .exec(&mut db)
            .await
            .unwrap();
        if i == 1 {
            visible_pk = row.id.to_string();
        }
    }
    let hidden = CheckRef::filter(CheckRef::fields().name().eq("Hidden"))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .unwrap();
    let hidden_pk = hidden.id.to_string();
    let cx = CxTestBuilder::new().app_context(db).build();
    let searchable = Field::choice(CheckRef::fields().name())
        .searchable()
        .relationship::<CheckRefSource>(
            |_cx| Query::all(),
            |r: &CheckRef| r.id,
            |r: &CheckRef| r.name.clone(),
        );
    // Legitimate FK beyond the cap passes via targeted check.
    assert!(check(&searchable, &cx, &visible_pk).await.is_empty());
    // Hidden row → invalid (not leaked), unknown → invalid.
    assert_eq!(
        check(&searchable, &cx, &hidden_pk).await,
        vec!["Name is invalid".to_string()]
    );
    assert_eq!(
        check(&searchable, &cx, &uuid::Uuid::new_v4().to_string()).await,
        vec!["Name is invalid".to_string()]
    );
    // Non-searchable over the same source keeps the retry error.
    let plain = Field::choice(CheckRef::fields().name()).relationship::<CheckRefSource>(
        |_cx| Query::all(),
        |r: &CheckRef| r.id,
        |r: &CheckRef| r.name.clone(),
    );
    assert_eq!(
        check(&plain, &cx, &visible_pk).await,
        vec!["Name could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_overflowed_searchable_renders_hint_and_keeps_value() {
    // GH #150 D6: over-cap searchable renders stored value + search input
    // + hint, with server data-attributes for the fetch.

    #[derive(Debug, toasty::Model, Clone)]
    struct HintRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct HintRefSource;
    impl OptionSource for HintRefSource {
        type Model = HintRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<HintRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &HintRef) -> bool {
            true
        }
        fn search_expr(_cx: &Cx, term: &str) -> Option<Expr<bool>> {
            name_search_expr(HintRef::fields().name(), term)
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(HintRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(HintRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(HintRef::fields().name())
        .searchable()
        .relationship::<HintRefSource>(
            |_cx| Query::all(),
            |r: &HintRef| r.id,
            |r: &HintRef| r.name.clone(),
        );
    let html = select
        .render(&cx, Some("stored-fk"), &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("value=\"stored-fk\""),
        "overflow must keep stored value: {html}"
    );
    assert!(
        html.contains("Too many options — type to search"),
        "overflow searchable must hint: {html}"
    );
    assert!(
        html.contains("data-options-server"),
        "overflow searchable must flag server fetch: {html}"
    );
    assert!(
        html.contains("data-options-field"),
        "server fetch needs the field name: {html}"
    );
}

#[tokio::test]
async fn relationship_bounded_searchable_keeps_client_filter() {
    // GH #150 + #91: bounded searchable sets narrow by label substring in
    // the browser — the server flag is overflow-only, or every small
    // table pays a debounced round-trip per keystroke.

    #[derive(Debug, toasty::Model, Clone)]
    struct SmallRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct SmallRefSource;
    impl OptionSource for SmallRefSource {
        type Model = SmallRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<SmallRef>>> {
            Ok(Query::all())
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &SmallRef) -> bool {
            true
        }
        fn search_expr(_cx: &Cx, term: &str) -> Option<Expr<bool>> {
            name_search_expr(SmallRef::fields().name(), term)
        }
    }

    let mut db = toasty::Db::builder()
        .models(toasty::models!(SmallRef))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(SmallRef {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let cx = CxTestBuilder::new().app_context(db).build();
    let select = Field::choice(SmallRef::fields().name())
        .searchable()
        .relationship::<SmallRefSource>(
            |_cx| Query::all(),
            |r: &SmallRef| r.id,
            |r: &SmallRef| r.name.clone(),
        );
    let html = select
        .render(&cx, None, &[], Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-options-filter"),
        "bounded searchable keeps the client filter input: {html}"
    );
    assert!(
        !html.contains("data-options-server"),
        "bounded searchable must not flag server fetch: {html}"
    );
    assert!(
        !html.contains("Too many options"),
        "bounded searchable must not hint: {html}"
    );
}
