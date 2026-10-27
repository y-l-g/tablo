use toasty::stmt::{List, Query};
use topcoat::context::Cx;

use super::*;
use crate::{Ability, schema::Field, test_support::memory_db, values::FormValues};

/// Reports what a submit's option check says for one choice.
async fn check(field: &Field, cx: &Cx, value: &str) -> Vec<String> {
    field
        .validate_exists(cx, &value.into(), None, None)
        .await
        .iter()
        .map(crate::Text::english)
        .collect()
}
/// Provides the related-source fixtures the option-policy tests share.
#[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
struct PolicyAuthor {
    #[key]
    #[auto]
    id: uuid::Uuid,
    tenant_id: Option<crate::TenantId>,
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
    fn label(_cx: &Cx, record: &PolicyAuthor) -> String {
        record.name.clone()
    }
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
    fn label(_cx: &Cx, author: &PolicyAuthor) -> String {
        author.name.clone()
    }
}

struct TenantScopedAuthors;
impl OptionSource for TenantScopedAuthors {
    type Model = PolicyAuthor;
    fn label(_cx: &Cx, record: &PolicyAuthor) -> String {
        record.name.clone()
    }

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
    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct RefAuthor {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct RefAuthorSource;
    impl OptionSource for RefAuthorSource {
        type Model = RefAuthor;
        fn label(_cx: &Cx, record: &RefAuthor) -> String {
            record.name.clone()
        }
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

    let mut db = memory_db(toasty::models!(RefAuthor, RefPost)).await;
    for i in 0..(MAX_RELATIONSHIP_OPTIONS + 1) {
        toasty::create!(RefAuthor {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = Cx::default().with(db);
    let select = Field::choice(RefPost::fields().author_id()).relationship::<RefAuthorSource>();
    // Over the cap: bounded work and a visible retry error.
    let errs = check(&select, &cx, "whatever").await;
    assert!(
        errs.iter().any(|e| e.contains("could not load options")),
        "overflow must surface retry error, got {errs:?}"
    );
}

#[tokio::test]
async fn relationship_option_values_are_primary_keys_not_table_ids() {
    // Option values come from the record's typed PK, never the display label.
    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct RefAuthor {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct RefAuthorSource;
    impl OptionSource for RefAuthorSource {
        type Model = RefAuthor;
        fn label(_cx: &Cx, record: &RefAuthor) -> String {
            record.name.clone()
        }
        fn scoped_query(_cx: &Cx) -> Result<Query<List<RefAuthor>>> {
            Ok(Query::all())
        }
        fn allows(_cx: &Cx, ability: Ability<'_, RefAuthor>) -> bool {
            ability.is_read()
        }
    }

    let mut db = memory_db(toasty::models!(RefAuthor)).await;
    let row = toasty::create!(RefAuthor {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = Cx::default().with(db);
    let select = Field::choice(RefAuthor::fields().name()).relationship::<RefAuthorSource>();
    // The PK validates; the label never does.
    assert!(check(&select, &cx, &pk).await.is_empty());
    assert_eq!(
        check(&select, &cx, "Ada").await,
        vec!["Name is invalid".to_string()]
    );
    let options = select
        .as_choice()
        .expect("a choice")
        .search_options(&cx, "", None)
        .await
        .unwrap();
    assert_eq!(
        options,
        [(pk, "Ada".to_string())],
        "a row's value is its key"
    );
}

#[tokio::test]
async fn relationship_load_fails_closed_when_view_any_is_refused() {
    // A source that denies `ViewAny` leaks neither labels nor ids and reports "not available".

    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
    let row = toasty::create!(PolicyAuthor {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = Cx::default().with(db);
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<DenyAllAuthors>();
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    assert_eq!(
        select
            .as_choice()
            .expect("a choice")
            .search_options(&cx, "", None)
            .await,
        Err(OptionLoadError::Denied),
        "the load offers no row, so no label or key leaks"
    );
}

#[tokio::test]
async fn relationship_load_denies_tenantless_requests_for_tenant_scoped_targets() {
    // A tenant-scoped source serves no unscoped options and narrows the load to the request tenant.

    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
    let tenant = crate::TenantId::from(uuid::Uuid::new_v4());
    let row = toasty::create!(PolicyAuthor {
        tenant_id: Some(tenant),
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = row.id.to_string();
    let cx = Cx::default().with(db);
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<TenantScopedAuthors>();
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is not available".to_string()]
    );
    // A resolved tenant that owns the row loads normally.
    let tenanted = cx.with(crate::tenancy::Tenant(tenant));
    assert!(check(&select, &tenanted, &pk).await.is_empty());
    // Another tenant's request reports the row as invalid.
    let foreign = cx.with(crate::tenancy::Tenant(uuid::Uuid::new_v4().into()));
    assert_eq!(
        check(&select, &foreign, &pk).await,
        vec!["Id is invalid".to_string()]
    );
}

#[tokio::test]
async fn relationship_load_filters_rows_by_view() {
    // `View`-denied rows are absent from options and validation.

    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
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
    let cx = Cx::default().with(db);
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>();
    assert!(
        check(&select, &cx, &visible.id.to_string())
            .await
            .is_empty()
    );
    assert_eq!(
        check(&select, &cx, &hidden.id.to_string()).await,
        vec!["Id is invalid".to_string()]
    );
    let options = select
        .as_choice()
        .expect("a choice")
        .search_options(&cx, "", None)
        .await
        .unwrap();
    assert_eq!(options, [(visible.id.to_string(), "Visible".to_string())]);
}

#[tokio::test]
async fn relationship_cap_counts_raw_rows_not_viewable_ones() {
    // The cap is checked on the raw bounded fetch before `View` filtering.

    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
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
    let cx = Cx::default().with(db);
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>();
    // The raw fetch overflows.
    assert_eq!(
        check(&select, &cx, &hidden_pk).await,
        vec!["Id could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_can_view_filtering_out_every_row_yields_invalid() {
    // A row `View` refuses is absent from validation; the integration suite's
    // `relationship_options` shows it absent from the form.

    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
    let hidden = toasty::create!(PolicyAuthor {
        name: "Hidden".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let pk = hidden.id.to_string();
    let cx = Cx::default().with(db);
    let select = Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>();
    assert_eq!(
        check(&select, &cx, &pk).await,
        vec!["Id is invalid".to_string()]
    );
}

#[tokio::test]
async fn relationship_options_share_one_load_per_request_and_tenant() {
    // Selects over one source share a single bounded load per `(request, tenant)`.
    use std::sync::atomic::{AtomicUsize, Ordering};

    static OPTION_LOADS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
    struct Ref {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct CountingSource;
    impl OptionSource for CountingSource {
        type Model = Ref;
        fn label(_cx: &Cx, record: &Ref) -> String {
            record.name.clone()
        }
        fn scoped_query(_cx: &Cx) -> Result<Query<List<Ref>>> {
            OPTION_LOADS.fetch_add(1, Ordering::SeqCst);
            Ok(Query::all())
        }
        fn allows(_cx: &Cx, ability: Ability<'_, Ref>) -> bool {
            ability.is_read()
        }
    }

    let mut db = memory_db(toasty::models!(Ref)).await;
    let row = toasty::create!(Ref {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let id = row.id.to_string();
    let cx = Cx::default().with(db);

    // Two selects, different labels, same source: the cache holds records, not options.
    let s1 = Field::choice(Ref::fields().name()).relationship::<CountingSource>();
    let s2 = Field::choice(Ref::fields().name())
        .relationship_labelled::<CountingSource>(|r: &Ref| format!("{}!", r.name));

    OPTION_LOADS.store(0, Ordering::SeqCst);
    assert!(check(&s1, &cx, &id).await.is_empty());
    assert!(check(&s2, &cx, &id).await.is_empty());
    assert_eq!(
        OPTION_LOADS.load(Ordering::SeqCst),
        1,
        "two selects must share one load"
    );

    // Same cache, other tenant → separate load (no cross-tenant sharing).
    let cx_b = cx.with(crate::tenancy::Tenant(uuid::Uuid::new_v4().into()));
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

    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct BigRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct BigRefSource;
    impl OptionSource for BigRefSource {
        type Model = BigRef;
        fn label(_cx: &Cx, record: &BigRef) -> String {
            record.name.clone()
        }
        fn scoped_query(_cx: &Cx) -> Result<Query<List<BigRef>>> {
            Ok(Query::all())
        }
        fn allows(_cx: &Cx, ability: Ability<'_, BigRef>) -> bool {
            ability.is_read()
        }
    }

    let mut db = memory_db(toasty::models!(BigRef)).await;
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(BigRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = Cx::default().with(db);
    let tenant = crate::tenancy::tenant_id(&cx);
    let err = super::related_records::<BigRefSource>(&cx, tenant)
        .await
        .unwrap_err();
    assert_eq!(err, &super::OptionLoadError::Overflow);
    // Non-searchable keeps the retry message.
    let plain = Field::choice(BigRef::fields().name()).relationship::<BigRefSource>();
    assert_eq!(
        check(&plain, &cx, "whatever-not-a-uuid").await,
        vec!["Name could not load options, retry".to_string()]
    );
}

#[tokio::test]
async fn relationship_search_narrows_past_the_cap() {
    // A distinctive term returns its bounded match on a table that overflows unfiltered.

    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
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
        fn label(_cx: &Cx, record: &SearchRef) -> String {
            record.name.clone()
        }
    }

    let mut db = memory_db(toasty::models!(SearchRef)).await;
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
    let cx = Cx::default().with(db);
    let tenant = crate::tenancy::tenant_id(&cx);
    // Unfiltered overflows.
    let err = super::related_records::<SearchRefSource>(&cx, tenant)
        .await
        .unwrap_err();
    assert_eq!(err, &super::OptionLoadError::Overflow);
    // Distinctive term narrows to one.
    let rows = super::related_records_search::<SearchRefSource>(&cx, "Zebra".to_string(), None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Zebra Unique");
    // Empty q is the bounded head.
    let err = super::related_records_search::<SearchRefSource>(&cx, "".to_string(), None)
        .await
        .unwrap_err();
    assert_eq!(err, super::OptionLoadError::Overflow);
    let select = Field::choice(SearchRef::fields().name())
        .searchable()
        .relationship::<SearchRefSource>();
    let opts = select
        .as_choice()
        .expect("a choice")
        .search_options(&cx, "Zebra", None)
        .await
        .unwrap();
    assert_eq!(opts.len(), 1);
    assert_eq!(opts[0].1, "Zebra Unique");
    assert_eq!(opts[0].0, unique.id.to_string());
}

#[tokio::test]
async fn relationship_search_without_searchable_falls_back_to_cap() {
    // No searchable columns overflows large tables instead of truncating.

    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
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
        fn label(_cx: &Cx, record: &PlainRef) -> String {
            record.name.clone()
        }
        fn scoped_query(_cx: &Cx) -> Result<Query<List<PlainRef>>> {
            Ok(Query::all())
        }
        fn allows(_cx: &Cx, ability: Ability<'_, PlainRef>) -> bool {
            ability.is_read()
        }
    }

    let mut db = memory_db(toasty::models!(PlainRef)).await;
    for i in 0..=MAX_RELATIONSHIP_OPTIONS {
        toasty::create!(PlainRef {
            name: format!("author-{i}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = Cx::default().with(db);
    let err = super::related_records_search::<PlainRefSource>(&cx, "author-1".to_string(), None)
        .await
        .unwrap_err();
    assert_eq!(err, super::OptionLoadError::Overflow);
}

#[tokio::test]
async fn relationship_overflowed_searchable_validates_via_targeted_check() {
    // Searchable selects over overflowed tables validate legitimate FKs via the targeted PK check.

    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct CheckRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct CheckRefSource;
    impl OptionSource for CheckRefSource {
        type Model = CheckRef;
        fn label(_cx: &Cx, record: &CheckRef) -> String {
            record.name.clone()
        }
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

    let mut db = memory_db(toasty::models!(CheckRef)).await;
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
    let cx = Cx::default().with(db);
    let searchable = Field::choice(CheckRef::fields().name())
        .searchable()
        .relationship::<CheckRefSource>();
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
    let plain = Field::choice(CheckRef::fields().name()).relationship::<CheckRefSource>();
    assert_eq!(
        check(&plain, &cx, &visible_pk).await,
        vec!["Name could not load options, retry".to_string()]
    );
}

/// Re-checks a relationship key through the write's own transaction.
#[tokio::test]
async fn recheck_resolves_the_key_through_the_write_transaction() {
    let mut db = memory_db(toasty::models!(PolicyAuthor)).await;
    let tenant = crate::TenantId::from(uuid::Uuid::new_v4());
    let author = |name: &str, tenant_id: crate::TenantId| {
        toasty::create!(PolicyAuthor {
            tenant_id: Some(tenant_id),
            name: name.to_string(),
        })
    };
    let own = author("Own", tenant).exec(&mut db).await.unwrap();
    let foreign = author("Foreign", uuid::Uuid::new_v4().into())
        .exec(&mut db)
        .await
        .unwrap();
    let hidden = author("Hidden", tenant).exec(&mut db).await.unwrap();
    let cx = Cx::default().with(db).with(crate::tenancy::Tenant(tenant));
    let scoped = crate::schema::Schema::new(
        Field::choice(PolicyAuthor::fields().id()).relationship::<TenantScopedAuthors>(),
    );
    let viewable = crate::schema::Schema::new(
        Field::choice(PolicyAuthor::fields().id()).relationship::<HideOneAuthor>(),
    );
    let values = |id: uuid::Uuid| FormValues::from_iter([("id".to_string(), id.to_string())]);
    let messages = |errors: crate::values::FieldErrors| -> Vec<String> {
        errors
            .iter()
            .map(|error| error.message("Id").english())
            .collect()
    };

    let mut handle = crate::db::db(&cx).unwrap();
    let mut tx = handle.transaction().await.unwrap();
    assert!(
        scoped
            .recheck_relationships(&cx, &values(own.id), &FormValues::new(), &mut tx)
            .await
            .is_empty()
    );
    assert_eq!(
        messages(
            scoped
                .recheck_relationships(&cx, &values(foreign.id), &FormValues::new(), &mut tx)
                .await
        ),
        ["Id is invalid"]
    );
    assert_eq!(
        messages(
            viewable
                .recheck_relationships(&cx, &values(hidden.id), &FormValues::new(), &mut tx)
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
                .recheck_relationships(&cx, &values(own.id), &FormValues::new(), &mut tx)
                .await
        ),
        ["Id is invalid"]
    );
}
