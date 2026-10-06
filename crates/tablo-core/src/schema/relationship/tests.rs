use toasty::stmt::{List, Query};
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::*,
};

use super::*;
use crate::{
    Ability,
    schema::{Field, tree::Mode},
};

/// Reports what a submit's option check says for one choice.
async fn check(field: &Field, cx: &Cx, value: &str) -> Vec<String> {
    field.validate_exists(cx, value).await
}
/// Provides the related-source fixtures the option-policy tests share.
#[derive(Debug, toasty::Model, Clone)]
struct PolicyAuthor {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: Option<uuid::Uuid>,
    name: String,
}

/// Provides the search fixtures' substring `LIKE` over `name`.
fn name_search_expr<M: toasty::schema::Model>(
    path: toasty::stmt::Path<M, String>,
    term: &str,
) -> Option<Expr<bool>> {
    Some(path.like_with_escape(format!("%{term}%"), '\\'))
}

/// Denies every request and fails the whole load closed.
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
    fn allows(_cx: &Cx, ability: Ability<'_, PolicyAuthor>) -> bool {
        match ability {
            Ability::ViewAny => true,
            Ability::View(record) => record.name != "Hidden",
            _ => false,
        }
    }
}

struct TenantScopedAuthors;
impl OptionSource for TenantScopedAuthors {
    type Model = PolicyAuthor;

    /// Filters on the `tenant_id` column.
    fn scoped_query(cx: &Cx) -> Result<Query<List<PolicyAuthor>>> {
        let tenant = crate::tenancy::require_tenant(cx)?;
        Ok(Query::all().filter(PolicyAuthor::fields().tenant_id().eq(tenant)))
    }

    fn allows(_cx: &Cx, ability: Ability<'_, PolicyAuthor>) -> bool {
        ability.is_read()
    }
    fn requires_tenant(_cx: &Cx) -> bool {
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
        fn allows(_cx: &Cx, ability: Ability<'_, RefAuthor>) -> bool {
            ability.is_read()
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
    let select = Field::choice(RefPost::fields().author_id())
        .relationship::<RefAuthorSource>(|a: &RefAuthor| a.name.clone());
    // Over the cap: bounded work and a visible retry error.
    let errs = check(&select, &cx, "whatever").await;
    assert!(
        errs.iter().any(|e| e.contains("could not load options")),
        "overflow must surface retry error, got {errs:?}"
    );
    // An overflowed load keeps the stored FK selectable.
    let html = select
        .render(&cx, Some("stored-fk"), None, Mode::Form)
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
    // Option values come from the record's typed PK, never the display label.
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
        fn allows(_cx: &Cx, ability: Ability<'_, RefAuthor>) -> bool {
            ability.is_read()
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
    let select = Field::choice(RefAuthor::fields().name())
        .relationship::<RefAuthorSource>(|a: &RefAuthor| a.name.clone());
    // The PK validates; the label never does.
    assert!(check(&select, &cx, &pk).await.is_empty());
    assert_eq!(
        check(&select, &cx, "Ada").await,
        vec!["Name is invalid".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), None, Mode::Form)
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
async fn relationship_load_fails_closed_when_view_any_is_refused() {
    // A source that denies `ViewAny` leaks neither labels nor ids and reports "not available".

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
    let select = Field::choice(PolicyAuthor::fields().id())
        .relationship::<DenyAllAuthors>(|a: &PolicyAuthor| a.name.clone());
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), None, Mode::Form)
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
    // The empty select explains itself on GET.
    assert!(
        html.contains("Id is not available"),
        "denied render must show the form-level error: {html}"
    );
}

#[tokio::test]
async fn relationship_load_denies_tenantless_requests_for_tenant_scoped_targets() {
    // A tenant-scoped source serves no unscoped options and narrows the load to the request tenant.

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
    let select = Field::choice(PolicyAuthor::fields().id())
        .relationship::<TenantScopedAuthors>(|a: &PolicyAuthor| a.name.clone());
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    // A resolved tenant that owns the row loads normally.
    let tenanted = cx.with(crate::tenancy::Tenant(tenant));
    assert!(check(&select, &tenanted, &pk).await.is_empty());
    // Another tenant's request reports the row as invalid.
    let foreign = cx.with(crate::tenancy::Tenant(uuid::Uuid::new_v4()));
    assert_eq!(
        check(&select, &foreign, &pk).await,
        vec!["Id is invalid".to_string()]
    );
}

#[tokio::test]
async fn relationship_load_filters_rows_by_view() {
    // `View`-denied rows are absent from options and validation.

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
    let select = Field::choice(PolicyAuthor::fields().id())
        .relationship::<HideOneAuthor>(|a: &PolicyAuthor| a.name.clone());
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
        .render(&cx, Some(&visible.id.to_string()), None, Mode::Form)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(html.contains("Visible"), "viewable row must render: {html}");
    assert!(
        !html.contains("Hidden"),
        "a row `View` refuses must not render: {html}"
    );
}

#[tokio::test]
async fn relationship_cap_counts_raw_rows_not_viewable_ones() {
    // The cap is checked on the raw bounded fetch before `View` filtering.

    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mut hidden_pk = String::new();
    // One past the cap, with one hidden row.
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
    let select = Field::choice(PolicyAuthor::fields().id())
        .relationship::<HideOneAuthor>(|a: &PolicyAuthor| a.name.clone());
    // The raw fetch overflows.
    assert_eq!(
        check(&select, &cx, &hidden_pk).await,
        vec!["Id could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_can_view_filtering_out_every_row_yields_invalid() {
    // A row `View` refuses is absent from options, validation, and re-render.

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
    let select = Field::choice(PolicyAuthor::fields().id())
        .relationship::<HideOneAuthor>(|a: &PolicyAuthor| a.name.clone());
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is invalid".to_string()]
    );
    let html = select
        .render(&cx, Some(&pk), None, Mode::Form)
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
    // Selects over one source share a single bounded load per `(request, tenant)`.
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
        fn allows(_cx: &Cx, ability: Ability<'_, Ref>) -> bool {
            ability.is_read()
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
    let s1 = Field::choice(Ref::fields().name())
        .relationship::<CountingSource>(|r: &Ref| r.name.clone());
    let s2 = Field::choice(Ref::fields().name())
        .relationship::<CountingSource>(|r: &Ref| format!("{}!", r.name));

    OPTION_LOADS.store(0, Ordering::SeqCst);
    assert!(check(&s1, &cx, &id).await.is_empty());
    assert!(check(&s2, &cx, &id).await.is_empty());
    let _ = s1
        .render(&cx, Some(&id), None, Mode::Form)
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
    // Over-cap reports `Overflow` so searchable selects degrade to type-to-search.

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
        fn allows(_cx: &Cx, ability: Ability<'_, BigRef>) -> bool {
            ability.is_read()
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
    let plain = Field::choice(BigRef::fields().name())
        .relationship::<BigRefSource>(|r: &BigRef| r.name.clone());
    assert_eq!(
        check(&plain, &cx, "whatever-not-a-uuid").await,
        vec!["Name could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_search_narrows_past_the_cap() {
    // A distinctive term returns its bounded match on a table that overflows unfiltered.

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
        fn allows(_cx: &Cx, ability: Ability<'_, SearchRef>) -> bool {
            ability.is_read()
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
    // Unfiltered overflows.
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
    // Empty q is the bounded head.
    let err = super::related_records_search::<SearchRefSource>(&cx, "".to_string())
        .await
        .unwrap_err();
    assert_eq!(err, super::OptionLoadError::Overflow);
    let select = Field::choice(SearchRef::fields().name())
        .searchable()
        .relationship::<SearchRefSource>(|r: &SearchRef| r.name.clone());
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
    // No searchable columns overflows large tables instead of truncating.

    #[derive(Debug, toasty::Model, Clone)]
    struct PlainRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    /// Declares no search expression.
    struct PlainRefSource;
    impl OptionSource for PlainRefSource {
        type Model = PlainRef;
        fn scoped_query(_cx: &Cx) -> Result<Query<List<PlainRef>>> {
            Ok(Query::all())
        }
        fn allows(_cx: &Cx, ability: Ability<'_, PlainRef>) -> bool {
            ability.is_read()
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
    // Searchable selects over overflowed tables validate legitimate FKs via the targeted PK check.

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
        fn allows(_cx: &Cx, ability: Ability<'_, CheckRef>) -> bool {
            match ability {
                Ability::ViewAny => true,
                Ability::View(record) => record.name != "Hidden",
                _ => false,
            }
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
        .relationship::<CheckRefSource>(|r: &CheckRef| r.name.clone());
    // Legitimate FK beyond the cap passes via targeted check.
    assert!(check(&searchable, &cx, &visible_pk).await.is_empty());
    // Hidden and unknown rows report invalid.
    assert_eq!(
        check(&searchable, &cx, &hidden_pk).await,
        vec!["Name is invalid".to_string()]
    );
    assert_eq!(
        check(&searchable, &cx, &uuid::Uuid::new_v4().to_string()).await,
        vec!["Name is invalid".to_string()]
    );
    // Non-searchable over the same source keeps the retry error.
    let plain = Field::choice(CheckRef::fields().name())
        .relationship::<CheckRefSource>(|r: &CheckRef| r.name.clone());
    assert_eq!(
        check(&plain, &cx, &visible_pk).await,
        vec!["Name could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_overflowed_searchable_renders_hint_and_keeps_value() {
    // Over-cap searchable renders the stored value, the search input, and the hint.

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
        fn allows(_cx: &Cx, ability: Ability<'_, HintRef>) -> bool {
            ability.is_read()
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
        .relationship::<HintRefSource>(|r: &HintRef| r.name.clone());
    let html = select
        .render(&cx, Some("stored-fk"), None, Mode::Form)
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
    // Bounded searchable sets filter in the browser; the server flag is overflow-only.

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
        fn allows(_cx: &Cx, ability: Ability<'_, SmallRef>) -> bool {
            ability.is_read()
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
        .relationship::<SmallRefSource>(|r: &SmallRef| r.name.clone());
    let html = select
        .render(&cx, None, None, Mode::Form)
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

/// Re-checks a relationship key through the write's own transaction.
#[tokio::test]
async fn recheck_resolves_the_key_through_the_write_transaction() {
    let mut db = toasty::Db::builder()
        .models(toasty::models!(PolicyAuthor))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let tenant = uuid::Uuid::new_v4();
    let author = |name: &str, tenant_id: uuid::Uuid| {
        toasty::create!(PolicyAuthor {
            tenant_id: Some(tenant_id),
            name: name.to_string(),
        })
    };
    let own = author("Own", tenant).exec(&mut db).await.unwrap();
    let foreign = author("Foreign", uuid::Uuid::new_v4())
        .exec(&mut db)
        .await
        .unwrap();
    let hidden = author("Hidden", tenant).exec(&mut db).await.unwrap();
    let cx = CxTestBuilder::new()
        .app_context(db)
        .request_context(crate::tenancy::Tenant(tenant))
        .build();
    let scoped = crate::schema::Schema::new(
        Field::choice(PolicyAuthor::fields().id())
            .relationship::<TenantScopedAuthors>(|a: &PolicyAuthor| a.name.clone()),
    );
    let viewable = crate::schema::Schema::new(
        Field::choice(PolicyAuthor::fields().id())
            .relationship::<HideOneAuthor>(|a: &PolicyAuthor| a.name.clone()),
    );
    let values =
        |id: uuid::Uuid| std::collections::HashMap::from([("id".to_string(), id.to_string())]);
    let messages = |errors: crate::form::FieldErrors| -> Vec<String> {
        errors.iter().map(|error| error.message("Id")).collect()
    };

    let mut handle = crate::db::db(&cx);
    let mut tx = handle.transaction().await.unwrap();
    assert!(
        scoped
            .recheck_relationships(&cx, &values(own.id), &mut tx)
            .await
            .is_empty()
    );
    assert_eq!(
        messages(
            scoped
                .recheck_relationships(&cx, &values(foreign.id), &mut tx)
                .await
        ),
        ["Id is invalid"]
    );
    assert_eq!(
        messages(
            viewable
                .recheck_relationships(&cx, &values(hidden.id), &mut tx)
                .await
        ),
        ["Id is invalid"]
    );
    PolicyAuthor::filter(PolicyAuthor::fields().id().eq(own.id))
        .delete()
        .exec(&mut tx)
        .await
        .unwrap();
    assert_eq!(
        messages(
            scoped
                .recheck_relationships(&cx, &values(own.id), &mut tx)
                .await
        ),
        ["Id is invalid"]
    );
}
