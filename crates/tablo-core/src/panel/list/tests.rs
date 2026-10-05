use toasty::Db;

use super::{super::TABLE_SEARCH_PATH, *};
use crate::{
    Ability, ReadOnly, ResourceDef, lens,
    panel::test_support::{Dummy, Subscriber, dummy_table, mount, panel_for},
};

/// Renders the list body with one seeded row.
async fn list_html<R: Resource>() -> String {
    list_html_with::<R>(&["Ada"]).await
}

/// Renders the list body with the named rows seeded in order.
async fn list_html_with<R: Resource>(names: &[&str]) -> String {
    list_html_via(names, panel_for::<R>).await
}

/// Renders the list body of the panel `panel` builds over a db seeded with `names`.
async fn list_html_via(names: &[&str], panel: fn() -> crate::Panel) -> String {
    use http_body_util::BodyExt;

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in names {
        toasty::create!(Dummy {
            name: (*name).to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db, panel()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&body).to_string()
}

/// Asserts two live lists declare distinct signal ids.
#[tokio::test]
async fn live_lists_declare_distinct_signal_ids() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    macro_rules! live_resource {
        ($name:ident, $slug:literal) => {
            struct $name;
            impl Resource for $name {
                type Model = Dummy;
                type Form = crate::NoForm<Self::Model>;

                fn declare() -> ResourceDef<Self> {
                    ResourceDef::new()
                        .slug($slug)
                        .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                            matches!(ability, Ability::ViewAny)
                        })
                        .table(dummy_table().paginate(25).live_search())
                }
            }
        };
    }
    live_resource!(FirstResource, "firsts");
    live_resource!(SecondResource, "seconds");

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    // The shell's sidebar signals match on every page; a bare layout leaves only the tables' own.
    fn bare<'a>(_cx: &'a Cx, slot: topcoat::router::Slot<'a>) -> topcoat::view::BoxView<'a> {
        topcoat::view::ViewExt::boxed(slot)
    }
    let router = mount(
        db,
        crate::Panel::new("admin")
            .resource::<FirstResource>()
            .resource::<SecondResource>()
            .auth(crate::Auth::disabled())
            .layout(bare),
    )
    .expect("panel builds");
    let mut ids = Vec::new();
    for uri in ["/admin/firsts", "/admin/seconds"] {
        let resp = router
            .handle(
                http::Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body).to_string();
        let page: std::collections::HashSet<String> = html
            .split("&quot;id&quot;:&quot;")
            .skip(1)
            .filter_map(|rest| rest.split("&quot;").next().map(str::to_string))
            .collect();
        assert!(
            !page.is_empty(),
            "{uri} must declare its table signals, got {html}"
        );
        ids.push(page);
    }
    assert!(
        ids[0].is_disjoint(&ids[1]),
        "two lists share signal ids {:?}",
        ids[0].intersection(&ids[1]).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn live_search_host_and_shard_dispatch() {
    // opt-in tables render the signal host (page bodies are
    // hoisted, so signals work there); the slug-dispatched shard serves
    // the table and 404s unknown paths.

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
    struct LiveResource;
    impl Resource for LiveResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                // the bulk transport this test pins renders where the policy
                // allows delete.
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::ViewAny
                            | Ability::View(_)
                            | Ability::DeleteAny
                            | Ability::Delete(_)
                    )
                })
                .table(
                    crate::table::Table::new(
                        crate::table::TextColumn::new(lens!(Dummy.name))
                            .searchable()
                            .sortable(),
                    )
                    .filters(crate::table::TernaryFilter::new(Dummy::fields().featured()))
                    .paginate(1)
                    .live_search(),
                )
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
        featured: false,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db.clone(), panel_for::<LiveResource>()).expect("panel builds");

    // List page carries the live host + GET fallback.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("data-live-search"),
        "opt-in table must render the shard host, got {html}"
    );
    // the filter bar is hoisted next to the search host — it
    // renders eagerly, above the swapped region, so a filter change cannot
    // rebuild the control the user is interacting with.
    let filter_at = html
        .find("data-filter-name=")
        .unwrap_or_else(|| panic!("live page must render the filter bar eagerly, got {html}"));
    let swapped_at = html
        .find("topcoat::region::start")
        .unwrap_or_else(|| panic!("live page must render the streamed region, got {html}"));
    assert!(
        filter_at < swapped_at,
        "the filter bar must sit outside the swapped region, got {html}"
    );
    assert!(
        html.contains("<noscript>"),
        "live table must keep the GET fallback, got {html}"
    );

    // Shard dispatch through the real runtime endpoint (JSON args + the
    // identity header the browser sends): unknown path fails, registered
    // path renders rows and the live controls bound to the caller's
    // signals.
    let sig = |n: u8, v: &str| {
        format!(
            r#"{{"t":"Signal","id":"{n:032x}","v":{}}}"#,
            serde_json::to_string(v).unwrap()
        )
    };
    /// Reads the signal id a rendered refresh control writes.
    fn revision_signal_id(html: &str) -> &str {
        const MARKER: &str = r#"id&quot;:&quot;"#;
        let at = html
            .find("data-table-revision")
            .unwrap_or_else(|| panic!("the refresh control, got {html}"));
        let tag_start = html[..at].rfind('<').expect("the control's opening tag");
        // The tag runs to the next `<`: an attribute value escapes its own
        // `>` (the handler's `=&gt;`), so the first `>` is not the tag's.
        let next = html[tag_start + 1..]
            .find('<')
            .map(|i| tag_start + 1 + i)
            .unwrap_or(html.len());
        let tag = &html[tag_start..next];
        let write = tag
            .find("data-topcoat-on:change")
            .unwrap_or_else(|| panic!("the control's write handler, got {tag}"));
        let id_at = tag[write..]
            .find(MARKER)
            .unwrap_or_else(|| panic!("the handler's signal, got {tag}"))
            + write
            + MARKER.len();
        let id = &tag[id_at..id_at + 32];
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "the handler must name a signal id, got {tag}"
        );
        id
    }
    // Args are positional shard inputs: the list path, the `query` signal
    // holding the list's URL query built from `pairs`, and the bulk handle
    // the table binds its selection transport to.
    let shard_args = |path: &str, pairs: &[(&str, &str)]| {
        let query = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        format!(
            "[{},{},{}]",
            serde_json::to_string(path).unwrap(),
            sig(1, &query),
            sig(2, "")
        )
    };
    async fn call_shard(router: &topcoat::router::Router, args: String) -> http::Response<Body> {
        router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri(TABLE_SEARCH_PATH)
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(topcoat::router::request::IDENTITY_HEADER, "A".repeat(22))
                    .body(Body::from(format!(r#"{{"args":{args},"signals":{{}}}}"#)))
                    .unwrap(),
            )
            .await
    }
    let nope = call_shard(&router, shard_args("/admin/nope", &[("q", "Ada")])).await;
    assert!(
        nope.status().is_client_error(),
        "unknown shard path must fail, got {}",
        nope.status()
    );
    let response = call_shard(&router, shard_args("/admin/dummies", &[("q", "Ada")])).await;
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes).to_string();
    assert_eq!(status, http::StatusCode::OK, "shard body: {table_html}");
    assert!(
        table_html.contains("Ada"),
        "live shard must render matching rows, got {table_html}"
    );
    // ...and not render it a second time inside the swapped table.
    assert!(
        !table_html.contains("data-filter-name="),
        "the swapped table must not duplicate the hoisted filter bar, got {table_html}"
    );
    // the bulk selection is signal-backed — the table binds its
    // transport to the selection signal, so a rerun re-renders the
    // selection instead of dropping it...
    assert!(
        table_html.contains(
            r#"data-topcoat-bind:value="(cx.hydrate({&quot;t&quot;:&quot;Signal&quot;,&quot;id&quot;:&quot;00000000000000000000000000000002&quot;})).get()""#
        ),
        "the table must bind the bulk transport to the selection signal, got {table_html}"
    );
    // The bulk delete uses a confirmation dialog: the trigger is a plain
    // button and the dialog's submit is the one that carries `confirm=1`
    // inside the same form.
    assert!(
        table_html.contains("data-bulk-confirm-trigger")
            && table_html.contains("data-bulk-confirm-dialog"),
        "the live table must carry the bulk confirmation, got {table_html}"
    );
    // ...while never reading it: selecting a row must not re-run the query.
    assert!(
        !table_html.contains(r#"::topcoat::dep("00000000000000000000000000000002")"#),
        "the bulk signal must not become a shard dependency, got {table_html}"
    );
    // The query is the one the shard reads.
    assert!(
        table_html.contains(r#"::topcoat::dep("00000000000000000000000000000001")"#),
        "the query signal must be a shard dependency, got {table_html}"
    );
    // the table's chrome is bound to the signals, so sort/pager
    // interactions re-render in place. `href` stays the no-JS fallback.
    assert!(
        table_html.contains("data-topcoat-on:click") && table_html.contains("sort=name"),
        "live table must bind the sort link and keep its href, got {table_html}"
    );
    // the shard's own refresh control. A mutation changes rows the
    // tracked inputs do not describe, so the client writes this token and
    // the shard re-runs. The write is only a re-run because the render read
    // the signal: the control's own write handler and the dep marker must
    // name the same id, and that id is not the bulk transport's.
    assert!(
        table_html.contains("data-table-revision") && table_html.contains("data-topcoat-on:change"),
        "the live table must carry the writable refresh control, got {table_html}"
    );
    let revision = revision_signal_id(&table_html);
    assert_ne!(
        revision, "00000000000000000000000000000002",
        "the refresh control must not reuse the bulk transport's signal"
    );
    assert!(
        table_html.contains(&format!(r#"::topcoat::dep("{revision}")"#)),
        "the refresh control's signal must be a shard dependency, got {table_html}"
    );
    assert!(
        table_html.contains("data-bulk-form") && table_html.contains("data-mutation-submit"),
        "the bulk form must opt into the in-place path, got {table_html}"
    );

    // A direct context for the loader/cursor assertions below.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/dummies")
        .body(())
        .unwrap()
        .into_parts();
    let cx = topcoat::context::CxTestBuilder::new()
        .request_context(parts)
        .app_context(db)
        .build();

    // Cursors are honored as sent: the sort/filter/search
    // handlers clear them in the browser, so a live cursor always belongs
    // to the current query; crafting one past a new query is the client's
    // own read-only inconsistency.
    let paged = crate::table::Table::<Dummy>::new(crate::table::TextColumn::new(lens!(Dummy.name)))
        .paginate(1);
    for name in ["Bob", "Cara"] {
        toasty::create!(Dummy {
            name: name.to_string(),
            featured: false,
        })
        .exec(&mut crate::db::db(&cx))
        .await
        .unwrap();
    }
    let page1 = load_table_page(
        &cx,
        &crate::resource::require_mounted::<LiveResource>(&cx).unwrap(),
        &paged,
        &crate::table::TableState::default(),
    )
    .await
    .unwrap();
    assert_eq!(page1.rows.len(), 1);
    let first_name = page1.rows[0].name.clone();
    let cursor = page1
        .next_cursor
        .clone()
        .expect("page 1 must have a cursor");
    let response = call_shard(&router, shard_args("/admin/dummies", &[("after", &cursor)])).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes);
    assert!(
        !table_html.contains(&first_name),
        "a cursor must continue past page-1 rows, got {table_html}"
    );
    // The pager renders its next/prev links with handlers too.
    assert!(
        table_html.contains("data-topcoat-on:click"),
        "live pager must bind its cursor handlers, got {table_html}"
    );
    // A fresh search with no cursor starts a new result set.
    let response = call_shard(&router, shard_args("/admin/dummies", &[("q", "Bob")])).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let table_html = String::from_utf8_lossy(&bytes);
    assert!(
        table_html.contains("Bob"),
        "fresh search must match new query, got {table_html}"
    );
    // the shard request's own URL carries no `?sort=`, so the sort the table
    // is rendered with exists only in the query signal it was invoked with;
    // the sorted column must report that direction.
    let response = call_shard(
        &router,
        shard_args("/admin/dummies", &[("sort", "name"), ("dir", "desc")]),
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let sorted_html = String::from_utf8_lossy(&bytes);
    assert!(
        sorted_html.contains("aria-sort=\"descending\""),
        "the live shard must report the query's sort direction, got {sorted_html}"
    );
}

#[tokio::test]
async fn live_search_input_debounces_keystrokes() {
    // The visible input is unbound (keystrokes stay
    // local until the debounce delay), the hidden transport carries the
    // bound `@change` write, and the GET form survives as the no-JS
    // fallback.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct LiveResource;
    impl Resource for LiveResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().slug("dummies").policy(ReadOnly).table(
                crate::table::Table::new(
                    crate::table::TextColumn::new(lens!(Dummy.name))
                        .searchable()
                        .sortable(),
                )
                .paginate(25)
                .live_search(),
            )
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
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("data-live-search-input"),
        "visible input must carry the debounce hook, got {html}"
    );
    assert!(
        html.contains("data-debounce-ms=\"200\""),
        "debounce delay must be pinned in the markup, got {html}"
    );
    assert!(
        html.contains("data-live-search-transport"),
        "hidden transport must carry the bound write, got {html}"
    );
    // The transport's value is bound to the query signal, so the script edits
    // the current query, not the one the page loaded with.
    let at = html.find("data-live-search-transport").unwrap();
    let tag = &html[html[..at].rfind('<').unwrap()..at + html[at..].find('>').unwrap()];
    assert!(
        tag.contains("data-topcoat-bind:value"),
        "the transport must bind its value to the query signal, got {tag}"
    );
    assert!(
        html.contains("data-topcoat-on:change"),
        "transport must write signals on change, got {html}"
    );
    assert!(
        !html.contains("data-topcoat-on:input"),
        "visible input must be unbound (debounce owns keystrokes), got {html}"
    );
    assert!(
        html.contains("<noscript>") && html.contains("name=\"q\""),
        "live table must keep the GET fallback, got {html}"
    );

    // The query signal is seeded with the request's query as written, so a
    // filter the parse drops (here the retired spelling) still warns on the
    // live table, and the empty table's Clear search is the script's to
    // handle: written in place, it would leave the input showing the term.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies?q=zzz&filters=status:draft")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("role=\"alert\"") && html.contains("dropped filters"),
        "a dropped filter must warn on the live table, got {html}"
    );
    let clear = html
        .rsplit('<')
        .find(|chunk| chunk.contains("Clear search"))
        .expect("the empty table's Clear search link");
    assert!(
        clear.contains("data-search-clear") && !clear.contains("data-topcoat-on:click"),
        "the live Clear search must be the script's, not a query write, got {clear}"
    );
}

#[tokio::test]
async fn read_only_resource_hides_delete_chrome() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct ReadOnlyResource;
    impl Resource for ReadOnlyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(dummy_table().paginate(25))
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
    let router = mount(db, panel_for::<ReadOnlyResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("Ada"),
        "the grid must render the seeded row, or the negative assertions below are vacuous, got {html}"
    );
    assert!(
        !html.contains("data-bulk-form") && !html.contains("Delete selected"),
        "read-only list must not render bulk chrome, got {html}"
    );
    assert!(
        !html.contains("/delete"),
        "read-only list must not render delete actions, got {html}"
    );
    // the read-only example must not emit an Edit link it cannot
    // honour — it has no record form and allows no delete.
    assert!(
        !html.contains("/edit") && !html.contains(">Edit<"),
        "read-only list must not render edit actions, got {html}"
    );
}

#[tokio::test]
async fn list_header_renders_create_entry_point_when_allowed() {
    use crate::resource::Resource;

    struct CreatableResource;
    impl Resource for CreatableResource {
        type Model = Dummy;
        type Form = CreatableForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(dummy_table().paginate(25))
                .form(crate::schema::Schema::new(crate::schema::Field::text(
                    Dummy::fields().name(),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct CreatableForm {
        name: String,
    }
    struct DenyCreateResource;
    impl Resource for DenyCreateResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(dummy_table().paginate(25))
        }
    }

    let html = list_html::<CreatableResource>().await;
    assert!(
        html.contains("href=\"/admin/dummies/create\"") && html.contains("Create"),
        "allowed list must link to the create page, got {html}"
    );
    let html = list_html::<DenyCreateResource>().await;
    assert!(
        !html.contains("/admin/dummies/create"),
        "denied list must not link to the create page, got {html}"
    );
}

#[tokio::test]
async fn non_editable_resource_hides_edit_links() {
    // The per-row Edit link follows the record form: a `NoForm` resource
    // hides it, a resource with a form links each row to
    // `{list}/{id}/edit`.

    use crate::resource::Resource;

    struct WritableResource;
    impl Resource for WritableResource {
        type Model = Dummy;
        type Form = WritableForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                // The row policy mirrors the edit route's own `View`
                // + `Update` check, so a form beside default-deny predicates
                // renders no link.
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                    )
                })
                .table(dummy_table().paginate(25))
                .form(crate::schema::Schema::new(crate::schema::Field::text(
                    Dummy::fields().name(),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct WritableForm {
        name: String,
    }
    struct LockedResource;
    impl Resource for LockedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(dummy_table().paginate(25))
        }
    }

    let html = list_html::<WritableResource>().await;
    assert!(
        html.contains("/edit") && html.contains("aria-label=\"Edit\""),
        "editable list must link rows to their edit pages, got {html}"
    );
    // The row and Create links navigate through the runtime, and the
    // panel's router turns prefetching off.
    assert!(
        html.contains("data-topcoat-link=\"never\"")
            && !html.contains("data-topcoat-link=\"intent\""),
        "panel links must use runtime navigation without prefetch, got {html}"
    );
    let html = list_html::<LockedResource>().await;
    assert!(
        !html.contains("/edit") && !html.contains(">Edit<"),
        "non-editable list must not render edit links, got {html}"
    );
}

/// Asserts denied rows render no edit chrome.
#[tokio::test]
async fn denied_rows_render_no_edit_chrome() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    /// The minimum a resource can declare: `ViewAny` so the list
    /// renders, a grid and a form so there is something to link to, and
    /// every other `can_*` left at its default.
    struct DeniedResource;
    impl Resource for DeniedResource {
        type Model = Dummy;
        type Form = DeniedForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(dummy_table().paginate(25))
                .form(crate::schema::Schema::new(crate::schema::Field::text(
                    Dummy::fields().name(),
                )))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct DeniedForm {
        name: String,
    }
    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<DeniedResource>()).expect("panel builds");

    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(
        html.contains("Ada"),
        "the grid must render the seeded row, or the negative assertions below are vacuous, got {html}"
    );
    assert!(
        !html.contains("/edit") && !html.contains(">Edit<"),
        "a row the policy denies must render no Edit link, got {html}"
    );
    assert!(
        !html.contains("Delete selected")
            && !html.contains("data-bulk-form")
            && !html.contains("/delete"),
        "a resource that allows no delete must render no delete affordance, got {html}"
    );

    // The route's own answer for the row the list no longer links.
    let resp = router
        .handle(
            http::Request::builder()
                .uri(format!("/admin/dummies/{}/edit", row.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::FORBIDDEN,
        "the edit route must deny the row the list no longer links"
    );
}

/// Asserts the GET `?q=` term is clamped like the shard's.
#[test]
fn from_cx_clamps_the_search_term() {
    use topcoat::context::CxTestBuilder;

    fn state_for(uri: &str) -> TableState {
        let (parts, ()) = http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new().request_context(parts).build();
        TableState::from_cx(&cx)
    }

    let long = "x".repeat(500);
    let state = state_for(&format!("/admin/users?q={long}"));
    assert_eq!(
        state.search.as_deref().map(str::len),
        Some(crate::query_term::MAX_QUERY_TERM),
        "the GET term is clamped to the same bound as the shard"
    );
    // Blank and absent stay None.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users?q=")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    assert!(TableState::from_cx(&cx).search.is_none());
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    assert!(TableState::from_cx(&cx).search.is_none());
}
#[tokio::test]
async fn both_cursors_render_the_first_page() {
    // Toasty pages from one cursor, so a URL naming `?after=` and `?before=`
    // together parses as no cursor: the first page, which is where the
    // cursor retry lands anyway. Both tokens below are valid.
    use topcoat::router::Body;

    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(|_cx: &Cx, ability: Ability<'_, Subscriber>| {
                    matches!(ability, Ability::ViewAny)
                })
                .table(
                    Table::<Subscriber>::new(crate::table::TextColumn::new(lens!(
                        Subscriber.email
                    )))
                    .paginate(1),
                )
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for email in ["a@b.c", "d@e.f"] {
        toasty::create!(Subscriber {
            email: email.to_string()
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db.clone(), panel_for::<SubscriberResource>()).expect("panel builds");

    // A valid cursor token: the first page of two rows has a next page.
    let (parts, ()) = http::Request::builder()
        .uri("/admin/subscribers")
        .body(())
        .unwrap()
        .into_parts();
    let cx = topcoat::context::CxTestBuilder::new()
        .request_context(parts)
        .app_context(db)
        .build();
    let table = crate::resource::require_mounted::<SubscriberResource>(&cx)
        .unwrap()
        .table
        .clone();
    let first = load_table_page(
        &cx,
        &crate::resource::require_mounted::<SubscriberResource>(&cx).unwrap(),
        &table,
        &TableState::default(),
    )
    .await
    .unwrap();
    let cursor = first
        .next_cursor
        .clone()
        .expect("page 1 must have a cursor");

    let response = router
        .handle(
            http::Request::builder()
                .uri(format!("/admin/subscribers?after={cursor}&before={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(
        response.status().is_success(),
        "page still streams, got status {}",
        response.status()
    );
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !body.contains("Couldn't load Subscribers"),
        "a URL naming both cursors is no error: {body}"
    );
    assert!(
        body.contains("a@b.c") && !body.contains("d@e.f"),
        "a URL naming both cursors lands on the first page: {body}"
    );
    assert!(
        !body.contains("before="),
        "the first page links forward only: {body}"
    );
}

#[test]
fn retry_url_for_error_drops_only_bad_cursors() {
    // a malformed cursor can never decode, so its retry link drops
    // pagination; any other failure keeps the full evidence.
    let state = TableState {
        search: Some("Ada".to_string()),
        cursor: Some(crate::table::Cursor::After("cur".to_string())),
        ..TableState::default()
    };
    let bad_cursor =
        crate::toasty_compat::cursor::decode("zz").expect_err("malformed cursor must fail");
    let retry = retry_url_for_error(&state, &bad_cursor, "/admin/users");
    assert!(
        !retry.contains("after="),
        "bad-cursor retry must drop pagination, got {retry}"
    );
    assert!(
        retry.contains("q=Ada"),
        "bad-cursor retry keeps the other state, got {retry}"
    );

    let db_error = crate::error::unavailable("connection reset");
    let retry = retry_url_for_error(&state, &db_error, "/admin/users");
    assert!(
        retry.contains("after=cur"),
        "transient failures retry the same evidence, got {retry}"
    );
}
