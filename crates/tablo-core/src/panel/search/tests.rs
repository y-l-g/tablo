use toasty::Db;
use topcoat::router::Body;

use super::{super::Panel, *};
use crate::panel::test_support::{Dummy, current_panel, mount, panel_for, panel_state};

/// The shard's positional args as the browser sends them: the list path, the
/// `query` signal holding the list's URL query built from `pairs`, and the
/// `bulk` signal the table binds its selection transport to.
fn shard_args(path: &str, pairs: &[(&str, &str)]) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    let sig = |n: u8, v: &str| {
        format!(
            r#"{{"t":"Signal","id":"{n:032x}","v":{}}}"#,
            serde_json::to_string(v).unwrap()
        )
    };
    format!(
        "[{},{},{}]",
        serde_json::to_string(path).unwrap(),
        sig(1, &query),
        sig(2, "")
    )
}
/// The signal id the live retry link writes: read from the
/// control's own `increment()` handler, which is the side that re-runs the
/// shard. Locating it by offset from the marker instead would read whatever
/// payload happened to follow.
fn retry_signal_id(html: &str) -> &str {
    const MARKER: &str = r#"id&quot;:&quot;"#;
    let at = html
        .find("data-retry-attempt")
        .unwrap_or_else(|| panic!("the retry control, got {html}"));
    let tag_start = html[..at].rfind('<').expect("the control's opening tag");
    // The tag runs to the next `<`: an attribute value escapes its own
    // `>` (the handler's `=&gt;`), so the first `>` is not the tag's.
    let next = html[tag_start + 1..]
        .find('<')
        .map(|i| tag_start + 1 + i)
        .unwrap_or(html.len());
    let tag = &html[tag_start..next];
    let write = tag
        .find("increment()")
        .unwrap_or_else(|| panic!("the control's write handler, got {tag}"));
    // The id sits in the handler's `cx.hydrate({"t":"Signal","id":"…"})`,
    // the last one before the increment.
    let id_at = tag[..write]
        .rfind(MARKER)
        .unwrap_or_else(|| panic!("the handler's signal, got {tag}"))
        + MARKER.len();
    let id = &tag[id_at..id_at + 32];
    assert!(
        id.chars().all(|c| c.is_ascii_hexdigit()),
        "the handler must name a signal id, got {tag}"
    );
    id
}

/// The live-search shard answers the gate before the registry lookup
/// an unauthenticated probe cannot distinguish a registered
/// slug from an unregistered one.
#[tokio::test]
async fn search_shard_answers_auth_before_the_registry_lookup() {
    use topcoat::{context::CxTestBuilder, router::response::IntoResponse};

    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
    }

    // A registry that really knows the `users` slug, so the known-path
    // probe is a resolution the gate must preempt.
    let registry = |auth| {
        let mut panel = panel_state("/admin", auth);
        panel
            .search
            .insert("users".to_string(), search_handler_for::<DummyResource>());
        current_panel(panel)
    };
    let (parts, ()) = http::Request::builder()
        .method(http::Method::POST)
        .uri(crate::auth::RUNTIME_PREFIX)
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(registry(crate::Auth::password()))
        .build();

    // The gate's answer comes before the lookup: both a registered and an
    // unregistered slug answer 401 identically (no 404 oracle).
    let unknown = match search_entry(&cx, "not-a-slug") {
        Ok(_) => panic!("unauthenticated probe must not resolve an entry"),
        Err(err) => err,
    };
    let registered = match search_entry(&cx, "users") {
        Ok(_) => panic!("an unauthenticated probe must never reach the registry"),
        Err(err) => err,
    };
    let unknown_status = unknown
        .into_response(&cx)
        .expect("gate answer renders")
        .status();
    let registered_status = registered
        .into_response(&cx)
        .expect("gate answer renders")
        .status();
    assert_eq!(
        unknown_status, registered_status,
        "unauthenticated probes must not distinguish registered slugs"
    );
    assert_eq!(
        registered_status,
        http::StatusCode::UNAUTHORIZED,
        "runtime probes answer 401 (ADR-0013), got {registered_status}"
    );

    // Auth disabled (the shard's own lookup is what remains): a
    // registered path resolves and an unknown path is a plain 404 again.
    let (parts, ()) = http::Request::builder()
        .method(http::Method::POST)
        .uri(crate::auth::RUNTIME_PREFIX)
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(registry(crate::Auth::disabled()))
        .build();
    assert!(
        search_entry(&cx, "users").is_ok(),
        "with auth disabled a registered path resolves through the lookup"
    );
    let err = match search_entry(&cx, "not-a-slug") {
        Ok(_) => panic!("an unregistered path must not resolve"),
        Err(err) => err,
    };
    assert!(
        err.downcast_ref::<topcoat::router::error::NotFoundError>()
            .is_some(),
        "with auth disabled the unknown path is a plain 404, got {err}"
    );
}

#[tokio::test]
async fn live_shard_malformed_cursor_renders_error_state() {
    // GH #158: a tampered `after=`/`before=` signal fails `cursor::decode`
    // inside the shard invocation — the invocation must render the branded
    // in-region `ErrorState` + retry link (same as the streamed list via
    // `retry_url_for_error`), not error the shard.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct LiveResource;
    impl Resource for LiveResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                })
                .searchable()
                .sortable(),
            )
            .paginate(1)
            .live_search()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<LiveResource>()).expect("panel builds");

    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(
                    r#"{{"args":{},"signals":{{}}}}"#,
                    shard_args("/admin/dummies", &[("after", "zz-not-a-cursor")])
                )))
                .unwrap(),
        )
        .await;
    assert_eq!(
        response.status(),
        http::StatusCode::OK,
        "malformed live cursor must render in place, not error the shard"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Couldn't load Dummies"),
        "error state must render in the shard output: {table_html}"
    );
    assert!(
        table_html.contains("role=\"alert\""),
        "error state must carry the alert role: {table_html}"
    );
    assert!(
        table_html.contains("href=\"/admin/dummies\""),
        "retry link must target the bare list (cursor dropped): {table_html}"
    );
    assert!(
        !table_html.contains("after="),
        "a malformed cursor must not travel into the retry link: {table_html}"
    );
    // GH #166: the retry writes the cursor signal in place — the same reset
    // its href spells out — so recovering keeps the signal-held search,
    // filters, and sort instead of reloading the page. The error state
    // renders no other control, so any click binding here is the retry.
    assert!(
        table_html.contains("data-topcoat-on:click"),
        "live retry must write the signals instead of navigating: {table_html}"
    );
    assert!(
        table_html.contains("set((cx.hydrate(&quot;&quot;)).clone())"),
        "live retry must clear the cursor signal: {table_html}"
    );
    // GH #294: the retry re-runs the shard through a token it reads and
    // increments, so the click re-runs the load even when every query
    // signal already holds the failing value.
    assert!(
        table_html.contains("data-retry-attempt="),
        "live retry must carry the shard-read retry token: {table_html}"
    );
    assert!(
        table_html.contains("increment()"),
        "live retry must increment the token: {table_html}"
    );
    // GH #294: the shard re-runs on the token only if it *read* the token,
    // which is what emits the token's own `dep` marker. Assert that marker,
    // not the presence of any dep — the shard's argument signals emit those
    // whether or not the error view ever reads the token.
    let retry = retry_signal_id(&table_html);
    assert!(
        table_html.contains(&format!(r#"::topcoat::dep("{retry}")"#)),
        "the retry token must be a shard dependency, got {table_html}"
    );

    // The `before` signal path is symmetric: a tampered backward cursor
    // renders the same cursor-stripped ErrorState. A tampered `group_by`
    // shard arg is normalized with the same state, so it must
    // not echo through the retry link either.
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(
                    r#"{{"args":{},"signals":{{}}}}"#,
                    shard_args(
                        "/admin/dummies",
                        &[("before", "zz-not-a-cursor"), ("group_by", "nope")]
                    )
                )))
                .unwrap(),
        )
        .await;
    assert_eq!(
        response.status(),
        http::StatusCode::OK,
        "malformed live before-cursor must render in place, not error the shard"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Couldn't load Dummies"),
        "error state must render for a bad before-cursor: {table_html}"
    );
    assert!(
        !table_html.contains("before="),
        "a malformed before-cursor must not travel into the retry link: {table_html}"
    );
    assert!(
        !table_html.contains("group_by"),
        "an unknown group_by must not echo through the shard retry link: {table_html}"
    );
}

#[tokio::test]
async fn live_shard_stale_cursor_retry_drops_pagination() {
    // GH #294: a token that decodes but was cut from another ordering is
    // refused by the engine, not by the decoder. The retry must still drop
    // pagination instead of repeating the identical failing request.

    use http_body_util::BodyExt;
    use toasty::stmt::Value;
    use toasty_core::stmt::ValueRecord;

    use crate::resource::Resource;

    struct LiveResource;
    impl Resource for LiveResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                })
                .searchable()
                .sortable(),
            )
            .paginate(1)
            .live_search()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["Ada", "Bob"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db, panel_for::<LiveResource>()).expect("panel builds");

    // The query orders by name then the primary key, so three fields is
    // one too many: the token decodes, the statement does not verify.
    let stale = crate::cursor::encode(&Value::Record(ValueRecord::from_vec(vec![
        Value::String("Ada".to_string()),
        Value::String("x".to_string()),
        Value::I64(1),
    ])))
    .unwrap();
    let args = shard_args("/admin/dummies", &[("after", &stale)]);
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Couldn't load Dummies"),
        "a stale cursor must render the error state: {table_html}"
    );
    assert!(
        table_html.contains("href=\"/admin/dummies\""),
        "the stale-cursor retry must target the bare list: {table_html}"
    );
    assert!(
        !table_html.contains("after="),
        "a stale cursor must not travel into the retry link: {table_html}"
    );
    assert!(
        table_html.contains("set((cx.hydrate(&quot;&quot;)).clone())"),
        "the stale-cursor retry must reset the cursor signal: {table_html}"
    );
}

#[tokio::test]
async fn live_shard_retry_preserves_the_query() {
    // GH #294: a failure the cursor did not cause must be retried with the
    // query that failed — search, filters, and sort included. The retry
    // re-runs through the token it increments, so it is not inert when the
    // query signals already hold the values the failed request used.
    //
    // A database with no schema pushed is the deterministic non-cursor
    // failure: the list query fails in the driver, whatever the cursor.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct Dummy {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
        featured: bool,
    }
    struct FailingLive;
    impl Resource for FailingLive {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                })
                .searchable()
                .sortable(),
            )
            .filters(crate::resource::TernaryFilter::r#for(
                Dummy::fields().featured(),
            ))
            .live_search()
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let router = mount(db, panel_for::<FailingLive>()).expect("panel builds");

    let args = shard_args(
        "/admin/dummies",
        &[
            ("q", "Ada"),
            ("f.featured", "true"),
            ("sort", "name"),
            ("dir", "desc"),
        ],
    );
    let response = router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Couldn't load Dummies"),
        "the failed live load must render the error state: {table_html}"
    );
    // The href keeps the failed query; the no-JS fallback retries it.
    let href = table_html
        .split("href=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string();
    assert!(
        href.contains("q=Ada") && href.contains("f.featured=true") && href.contains("sort=name"),
        "the retry href must keep the failed query, got {href}"
    );
    assert!(
        table_html.contains("data-retry-attempt=") && table_html.contains("increment()"),
        "the retry must re-run through its token: {table_html}"
    );
    // The shard re-runs only if it read the token: assert the token's own
    // dep marker, not the shard arguments' dep markers.
    let retry = retry_signal_id(&table_html);
    assert!(
        table_html.contains(&format!(r#"::topcoat::dep("{retry}")"#)),
        "the retry token must be a shard dependency, got {table_html}"
    );
    assert!(
        !table_html.contains("set((cx.hydrate"),
        "a non-cursor failure must not clear the query signals: {table_html}"
    );
}

#[tokio::test]
async fn live_shard_group_by_query_drives_grouping() {
    // GH #157: grouping travels in the live query, not a page-load snapshot
    // — the shard groups by the query's `group_by`, so a rerun with it set
    // renders headers and a rerun without it does not.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct GroupedResource;
    impl Resource for GroupedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                })
                .searchable()
                .sortable(),
            )
            .group_by("name", |d: &Dummy| d.name.clone())
            .paginate(25)
            .live_search()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["Ada", "Grace"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db, panel_for::<GroupedResource>()).expect("panel builds");

    let grouped = |group_by: &str| shard_args("/admin/dummies", &[("group_by", group_by)]);
    let post_shard = |args: String| {
        router.handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
                .unwrap(),
        )
    };

    let response = post_shard(grouped("name")).await;
    assert_eq!(
        response.status(),
        http::StatusCode::OK,
        "grouped shard rerun must succeed"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Ada (1 on this page)")
            && table_html.contains("Grace (1 on this page)"),
        "the query's group_by must drive group headers in the shard output: {table_html}"
    );

    let response = post_shard(grouped("")).await;
    assert_eq!(
        response.status(),
        http::StatusCode::OK,
        "ungrouped shard rerun must succeed"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        !table_html.contains("on this page"),
        "a query without group_by must render no group headers: {table_html}"
    );
}

/// Post one live-table shard rerun with an optional `Tenant` request
/// extension.
///
/// The first positional arg is the list path the registry is keyed by, so
/// the caller chooses the resource; the identity header is the one the
/// browser runtime sends.
async fn post_table_shard(
    router: &topcoat::router::Router,
    path: &str,
    tenant: Option<uuid::Uuid>,
) -> http::Response<Body> {
    let args = shard_args(path, &[]);
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri(TABLE_SEARCH_PATH)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
        .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
        .unwrap();
    let (mut parts, body) = request.into_parts();
    if let Some(tenant) = tenant {
        parts.extensions.insert(crate::tenancy::Tenant(tenant));
    }
    router.handle(http::Request::from_parts(parts, body)).await
}

/// The live-search shard re-checks the tenant and policy gates itself,
/// because page guards do not run on shard requests: a gated
/// resource with no tenant must be refused instead of running an unscoped
/// query, a tenanted rerun must serve only that tenant's rows, and a
/// `can_view_any` denial is refused even with a tenant present.
#[tokio::test]
async fn live_shard_enforces_tenant_and_policy_gates() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, Clone, toasty::Model)]
    struct TenantDummy {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[index]
        tenant_id: uuid::Uuid,
        name: String,
    }

    struct TenantLive;
    impl Resource for TenantLive {
        type Model = TenantDummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "tenant-dummies".to_string()
        }
        fn requires_tenant() -> bool {
            true
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &TenantDummy) -> bool {
            true
        }
        fn table() -> crate::resource::Table<TenantDummy> {
            crate::resource::Table::new(
                |d: &TenantDummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(
                    TenantDummy::fields().name(),
                    |d: &TenantDummy| d.name.clone(),
                )
                .searchable()
                .sortable(),
            )
            .paginate(10)
            .live_search()
        }
    }

    struct DeniedLive;
    impl Resource for DeniedLive {
        type Model = TenantDummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "denied-dummies".to_string()
        }
        fn requires_tenant() -> bool {
            true
        }
        fn can_view_any(_cx: &Cx) -> bool {
            false
        }
        fn table() -> crate::resource::Table<TenantDummy> {
            crate::resource::Table::new(
                |d: &TenantDummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(
                    TenantDummy::fields().name(),
                    |d: &TenantDummy| d.name.clone(),
                )
                .searchable()
                .sortable(),
            )
            .paginate(10)
            .live_search()
        }
    }

    let tenant_a = uuid::Uuid::from_u128(1);
    let tenant_b = uuid::Uuid::from_u128(2);
    let mut db = Db::builder()
        .models(toasty::models!(TenantDummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for (tenant, name) in [(tenant_a, "Alpha"), (tenant_b, "Bravo")] {
        toasty::create!(TenantDummy {
            tenant_id: tenant,
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(
        db,
        Panel::new("admin")
            .resource::<TenantLive>()
            .resource::<DeniedLive>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    let response = post_table_shard(&router, "/admin/tenant-dummies", None).await;
    assert_eq!(
        response.status(),
        http::StatusCode::FORBIDDEN,
        "a tenantless shard rerun must be refused"
    );

    let response = post_table_shard(&router, "/admin/tenant-dummies", Some(tenant_a)).await;
    assert_eq!(
        response.status(),
        http::StatusCode::OK,
        "a tenanted shard rerun must succeed"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        table_html.contains("Alpha"),
        "the shard must render the tenant's own row: {table_html}"
    );
    assert!(
        !table_html.contains("Bravo"),
        "the shard must not render another tenant's row: {table_html}"
    );

    let response = post_table_shard(&router, "/admin/denied-dummies", Some(tenant_a)).await;
    assert_eq!(
        response.status(),
        http::StatusCode::FORBIDDEN,
        "a can_view_any denial must refuse the shard rerun"
    );
}

/// Post one live relation-table shard rerun: the (`parent`, `child`) pair the
/// relation registry is keyed by, the owner's seed, the record page, and the
/// query pairs the `query` signal carries.
fn relation_shard_args(
    parent: &str,
    child: &str,
    seed: &str,
    page: &str,
    read_only: bool,
    pairs: &[(&str, &str)],
) -> String {
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    let sig = |n: u8, v: &str| {
        format!(
            r#"{{"t":"Signal","id":"{n:032x}","v":{}}}"#,
            serde_json::to_string(v).unwrap()
        )
    };
    format!(
        "[{},{},{},{},{}]",
        serde_json::to_string(&format!("{parent}/{child}/{seed}")).unwrap(),
        serde_json::to_string(page).unwrap(),
        read_only,
        sig(1, &query),
        sig(2, "")
    )
}

async fn post_relation_shard(
    router: &topcoat::router::Router,
    args: String,
) -> http::Response<Body> {
    router
        .handle(
            http::Request::builder()
                .method(http::Method::POST)
                .uri(TABLE_RELATION_SEARCH_PATH)
                .header(http::header::CONTENT_TYPE, "application/json")
                .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
                .unwrap(),
        )
        .await
}

/// The relation shard serves one owner's rows through the child's own table:
/// the seed scopes the load, the prefixed query drives search and sort, and
/// the controls write the signals in place. An unregistered pair is a 404,
/// and a seed or page the handler cannot honor is a 400.
#[tokio::test]
async fn live_relation_shard_serves_the_seeded_owner_in_place() {
    use http_body_util::BodyExt;

    use crate::resource::{Relation, Resource};

    #[derive(Debug, Clone, toasty::Model)]
    struct Shelf {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    #[derive(Debug, Clone, toasty::Model)]
    struct Book {
        #[key]
        #[auto]
        id: uuid::Uuid,
        title: String,
        shelf_id: uuid::Uuid,
    }

    struct BookResource;
    impl Resource for BookResource {
        type Model = Book;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "books".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Book) -> bool {
            true
        }
        fn table() -> crate::resource::Table<Book> {
            crate::resource::Table::new(
                |b: &Book| b.id.to_string(),
                crate::resource::TextColumn::r#for(Book::fields().title(), |b: &Book| {
                    b.title.clone()
                })
                .searchable()
                .sortable(),
            )
            .live_search()
        }
    }

    struct ShelfResource;
    impl Resource for ShelfResource {
        type Model = Shelf;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "shelves".to_string()
        }
        fn can_view_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Shelf) -> bool {
            true
        }
        fn relations() -> Vec<Relation<Shelf>> {
            vec![Relation::has_many::<BookResource, _>(
                Book::fields().shelf_id(),
                |shelf: &Shelf| shelf.id,
            )]
        }
        fn table() -> crate::resource::Table<Shelf> {
            crate::resource::Table::new(
                |s: &Shelf| s.id.to_string(),
                crate::resource::TextColumn::r#for(Shelf::fields().name(), |s: &Shelf| {
                    s.name.clone()
                }),
            )
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Shelf, Book))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let atlas = toasty::create!(Shelf {
        name: "Atlas".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let bravo = toasty::create!(Shelf {
        name: "Bravo".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    for title in ["Apple", "Avocado"] {
        toasty::create!(Book {
            title: title.to_string(),
            shelf_id: atlas.id,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    toasty::create!(Book {
        title: "Berry".to_string(),
        shelf_id: bravo.id,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(
        db,
        Panel::new("admin")
            .resource::<ShelfResource>()
            .resource::<BookResource>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    // The seed scopes the load to the owner's rows.
    let args = relation_shard_args(
        "shelves",
        "books",
        &atlas.id.to_string(),
        &format!("/admin/shelves/{}", atlas.id),
        true,
        &[("books.q", "Apple")],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let table_html =
        String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
            .to_string();
    assert!(
        table_html.contains("Apple"),
        "the shard renders the matching row: {table_html}"
    );
    assert!(
        !table_html.contains("Avocado"),
        "the search narrows the owner's rows: {table_html}"
    );
    assert!(
        !table_html.contains("Berry"),
        "the seed withholds the other owner's rows: {table_html}"
    );

    // The other owner's seed serves its own rows.
    let args = relation_shard_args(
        "shelves",
        "books",
        &bravo.id.to_string(),
        &format!("/admin/shelves/{}", bravo.id),
        true,
        &[],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let table_html =
        String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
            .to_string();
    assert!(
        table_html.contains("Berry"),
        "the shard renders the other owner's rows: {table_html}"
    );
    assert!(
        !table_html.contains("Apple"),
        "the other seed withholds the first owner's rows: {table_html}"
    );

    // The prefixed sort orders the rows and writes the signals in place.
    let args = relation_shard_args(
        "shelves",
        "books",
        &atlas.id.to_string(),
        &format!("/admin/shelves/{}", atlas.id),
        true,
        &[("books.sort", "title"), ("books.dir", "desc")],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let table_html =
        String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
            .to_string();
    let avo = table_html.find("Avocado").expect("the rows render");
    let apple = table_html.find("Apple").expect("the rows render");
    assert!(
        avo < apple,
        "descending sort orders the relation rows: {table_html}"
    );
    assert!(
        table_html.contains("books.sort="),
        "the sort links keep the relation prefix: {table_html}"
    );
    assert!(
        table_html.contains("data-topcoat-on:click"),
        "the sort links write the signals instead of navigating: {table_html}"
    );

    // An unregistered pair is a 404; a seed or page the handler cannot
    // honor is a 400.
    let args = relation_shard_args(
        "shelves",
        "nope",
        &atlas.id.to_string(),
        &format!("/admin/shelves/{}", atlas.id),
        true,
        &[],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(
        response.status(),
        http::StatusCode::NOT_FOUND,
        "an unregistered relation pair must 404"
    );
    let args = relation_shard_args(
        "shelves",
        "books",
        "not-a-uuid",
        &format!("/admin/shelves/{}", atlas.id),
        true,
        &[],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(
        response.status(),
        http::StatusCode::BAD_REQUEST,
        "an unparseable seed must be refused"
    );
    let args = relation_shard_args(
        "shelves",
        "books",
        &atlas.id.to_string(),
        "/elsewhere",
        true,
        &[],
    );
    let response = post_relation_shard(&router, args).await;
    assert_eq!(
        response.status(),
        http::StatusCode::BAD_REQUEST,
        "a page outside the panel must be refused"
    );
}

/// topcoat#441: the shard is served at the named path, so its endpoint is the
/// same in every build and the tests post to it by name.
#[test]
fn table_search_endpoint_is_the_named_path() {
    use topcoat::router::Route as _;

    assert_eq!(table_search.path().as_str(), TABLE_SEARCH_PATH);
}

/// The relation shard's endpoint carries the same stability contract: the
/// literal in [`table_relation_search`]'s attribute is the named path.
#[test]
fn table_relation_search_endpoint_is_the_named_path() {
    use topcoat::router::Route as _;

    assert_eq!(
        table_relation_search.path().as_str(),
        TABLE_RELATION_SEARCH_PATH
    );
}
