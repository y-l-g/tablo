use toasty::Db;

use super::{super::TABLE_SEARCH_PATH, *};
use crate::{
    Ability, Policy, ReadOnly, Tenancy,
    panel::test_support::{Dummy, dummy_table, mount, panel_for},
};

/// The minimal table-backed model the list-chrome tests share:
/// `list_html` was declared twice with byte-identical bodies apart from one
/// seeded row, so a change to the panel's list route had to be made twice.
/// The `GET /admin/dummies` body for a resource registered with one seeded
/// row, so the row-chrome assertions have a row to look at.
async fn list_html<R: Resource>() -> String {
    list_html_with::<R>(&["Ada"]).await
}

/// [`list_html`] with the named rows seeded in order, so a per-record
/// policy has rows to disagree about.
async fn list_html_with<R: Resource>(names: &[&str]) -> String {
    list_html_via(names, panel_for::<R>).await
}

/// The list body of the panel `panel` builds over a db seeded with `names`.
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

/// Runtime navigation restores every signal the next page shares with
/// the current one, so two resources' lists must declare different signal
/// ids, or one list's search filters the next (GH #395).
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
                fn slug() -> String {
                    $slug.to_string()
                }
                fn policy() -> impl Policy<Dummy> {
                    |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
                }
                fn table() -> crate::resource::Table<Dummy> {
                    dummy_table().paginate(25).live_search()
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
    // The shell's sidebar signals are the same on every page by design; a
    // bare layout leaves only the tables' own.
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
        fn slug() -> String {
            "dummies".to_string()
        }
        // the bulk transport this test pins renders where the policy
        // allows delete.
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                )
            }
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
    /// The signal id a rendered refresh control writes: read
    /// from the control's own `data-topcoat-on:change` handler, which is
    /// the side that re-runs the shard. Locating it by offset from the
    /// marker instead would read whatever payload happened to follow.
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
    // GH #184 replaced the disabled destructive submit with a confirmation
    // dialog: the trigger is a plain button and the dialog's submit is the
    // one that carries `confirm=1` inside the same form.
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
    let paged = crate::resource::Table::<Dummy>::new(
        |d: &Dummy| d.id.to_string(),
        crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| d.name.clone()),
    )
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
    let page1 =
        load_table_page::<LiveResource>(&cx, &paged, &crate::resource::TableState::default())
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
    // GH #172 decision 4: the visible input is unbound (keystrokes stay
    // local until the debounce delay), the hidden transport carries the
    // bound `@change` write, and the GET form survives as the no-JS
    // fallback.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct LiveResource;
    impl Resource for LiveResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            ReadOnly
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
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table().paginate(25)
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
    // GH #162 (Filament's List page `CreateAction` in the page header):
    // the Create link is eager page chrome, gated on `Create`.

    use crate::resource::Resource;

    struct CreatableResource;
    impl Resource for CreatableResource {
        type Model = Dummy;
        type Form = CreatableForm;
        fn form(_dx: &crate::schema::DeclCx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table().paginate(25)
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
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            CreatableResource::table()
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
        fn form(_dx: &crate::schema::DeclCx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        // GH #235: the row policy mirrors the edit route's own `View`
        // + `Update` check, so a form beside default-deny predicates
        // renders no link.
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(
                    ability,
                    Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
                )
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table().paginate(25)
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
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            WritableResource::table()
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

/// A list whose rows the policy denies renders no Edit link and no delete
/// chrome.
///
/// This resource has a form, so the Edit prefix is wired, and its
/// default-deny `View` / `Update` withhold the link per row. Its
/// `DeleteAny` is default-deny too, so no delete prefix is wired and
/// neither the Delete control nor the bulk column renders.
///
/// The row assertion comes first so the negative assertions below cannot
/// pass vacuously; the route assertion then records the route's own answer
/// for the row the list never links.
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
        fn form(_dx: &crate::schema::DeclCx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table().paginate(25)
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

/// a resource whose chrome is wired narrows it per record. The
/// panel wires each action from the predicate its route checks — `View`
/// for View, `View` + `Update` for Edit, `View` + `Delete`
/// for Delete and the bulk checkbox — so a refused row renders no link and
/// no checkbox instead of a control the route answers 403 to.
///
/// This is the panel half, which the render-level test cannot cover: a
/// hand-written row policy closure proves the renderer, not the wiring.
#[tokio::test]
async fn per_record_policy_narrows_the_wired_chrome() {
    use crate::{
        resource::Resource,
        schema::{Field, Schema},
    };

    /// Chrome wired for all three actions, with a policy that refuses one
    /// row per predicate so each half is separately visible.
    struct RowPolicyResource;
    impl Resource for RowPolicyResource {
        type Model = Dummy;
        type Form = RowPolicyForm;
        fn form(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                Ability::ViewAny => true,
                Ability::View(record) => record.name != "Hidden",
                Ability::Update(record) => record.name != "Locked",
                Ability::DeleteAny => true,
                Ability::Delete(record) => record.name != "Locked",
                _ => false,
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table().paginate(25)
        }
        fn view(_dx: &crate::schema::DeclCx) -> Schema {
            Schema::new(Field::text(Dummy::fields().name()))
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct RowPolicyForm {
        name: String,
    }
    let html = list_html_with::<RowPolicyResource>(&["Ada", "Hidden", "Locked"]).await;
    // Only the allowed row carries a checkbox; refused rows keep their
    // cells but no key chrome.
    let rows = rendered_rows(&html);
    assert_eq!(rows.len(), 1, "only Ada carries a checkbox: {html}");
    let ada = rows[0].0.clone();
    for name in ["Ada", "Hidden", "Locked"] {
        assert!(
            html.contains(&format!(">{name}<")),
            "the {name} row must still render: {html}"
        );
    }

    // The allowed row keeps all three links and the page's only checkbox.
    assert!(
        html.contains(&format!("href=\"/admin/dummies/{ada}\""))
            && html.contains(&format!("/admin/dummies/{ada}/edit"))
            && html.contains(&format!("delete={ada}")),
        "the allowed row must keep its View/Edit/Delete links, got {html}"
    );
    assert_eq!(
        html.matches("data-row-select").count(),
        1,
        "the allowed row owns the page's only checkbox, got {html}"
    );

    // Refused rows own no record href and no delete opener: three
    // record hrefs (Ada's View and Edit, Locked's View) and one delete
    // opener (Ada's) leave Hidden none.
    assert_eq!(
        html.matches("href=\"/admin/dummies/").count(),
        3,
        "only Ada and Locked may own a record href, got {html}"
    );
    assert_eq!(
        html.matches("delete=").count(),
        1,
        "only Ada may own a delete opener, got {html}"
    );
}

/// The `(record key, name cell)` pairs `html` renders, in document order.
///
/// A test cannot assume the seeding order — a paginated table with no
/// sortable column orders by the PK fallback, and the keys are random — so
/// it reads each row's own cells.
fn rendered_rows(html: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("data-row-select") {
        let start = rest[..at]
            .rfind("<input")
            .expect("the marker's opening tag");
        let tag = &rest[start..];
        let value_at = tag.find("value=\"").expect("a checkbox value");
        let after = &tag[value_at + "value=\"".len()..];
        let end = after.find('"').expect("a closed value");
        let id = after[..end].to_string();
        // The name is the cell after the checkbox's own `<td>`.
        let cell = &rest[at..];
        let cell_at = cell.find("<td").expect("the name cell");
        let text = &cell[cell_at..];
        let text_at = text.find('>').expect("the cell's opening tag") + 1;
        let text_end = text[text_at..].find('<').expect("the cell's text end");
        rows.push((id, text[text_at..text_at + text_end].trim().to_string()));
        rest = &rest[at + 1..];
    }
    rows
}

/// The GET `?q=` term is clamped like the shard's: bounded
/// echoed state.
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
        Some(crate::resource::MAX_QUERY_TERM),
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
async fn tenant_gated_resource_fails_closed_without_tenant() {
    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct Dummy {
        #[key]
        #[auto]
        id: uuid::Uuid,
        // The conventional column, present so this gated resource has a
        // scoping predicate the framework can derive: a gated
        // resource without one is a `build` error now, and the gate this
        // fixture tests is only isolable on a resource that builds. Nothing
        // here exercises the filter — every asserted request is tenantless
        // and must 403 at the gate, or tenant-bearing and must pass it —
        // which is the point: a valid declaration leaves the gate as the
        // only thing that can deny.
        tenant_id: uuid::Uuid,
        name: String,
    }
    struct GatedResource;
    impl Resource for GatedResource {
        type Model = Dummy;
        type Form = GatedForm;
        fn form(_dx: &crate::schema::DeclCx) -> crate::schema::Schema {
            crate::schema::Schema::new(crate::schema::Field::text(Dummy::fields().name()))
        }

        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(ability, Ability::ViewAny | Ability::Create)
            }
        }
        fn tenancy() -> Tenancy<Dummy> {
            Tenancy::column(Dummy::fields().tenant_id())
        }
        fn table() -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }),
            )
            .paginate(25)
        }
    }
    #[derive(crate::RecordForm)]
    #[form(model = Dummy)]
    struct GatedForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(db, panel_for::<GatedResource>()).expect("panel builds");
    // No tenant anywhere → 403, not unscoped rows.
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::FORBIDDEN);
    // Valid CSRF but still no tenant → 403 from the tenant gate.
    let token = uuid::Uuid::new_v4().to_string();
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/create")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("name=Ada&csrf_token={token}")))
                .unwrap(),
        )
        .await;
    assert_eq!(resp.status(), http::StatusCode::FORBIDDEN);
    // A server-set `Tenant` request extension supplies the tenant → gate
    // passes (create page 200). A request header does not.
    let tenant = uuid::Uuid::new_v4();
    let (mut parts, ()) = http::Request::builder()
        .uri("/admin/dummies/create")
        .body(())
        .unwrap()
        .into_parts();
    parts.extensions.insert(crate::Tenant(tenant));
    let resp = router
        .handle(http::Request::from_parts(parts, Body::empty()))
        .await;
    assert!(
        resp.status().is_success(),
        "tenant-gated GET with tenant must pass the gate, got {}",
        resp.status()
    );
}

/// A tenant-scoped resource whose request carries a tenant, and which
/// overrides nothing else (`query` stays the default): the framework's tenant
/// filter is the only thing scoping this list.
#[tokio::test]
async fn tenant_gated_resource_scopes_rows_to_the_request_tenant() {
    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct Scoped {
        #[key]
        #[auto]
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        name: String,
    }
    struct ScopedResource;
    impl Resource for ScopedResource {
        type Model = Scoped;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "scoped".to_string()
        }
        fn policy() -> impl Policy<Scoped> {
            |_cx: &Cx, ability: Ability<'_, Scoped>| matches!(ability, Ability::ViewAny)
        }
        fn tenancy() -> Tenancy<Scoped> {
            Tenancy::column(Scoped::fields().tenant_id())
        }
        fn table() -> crate::resource::Table<Scoped> {
            crate::resource::Table::new(
                |s: &Scoped| s.id.to_string(),
                crate::resource::TextColumn::r#for(Scoped::fields().name(), |s: &Scoped| {
                    s.name.clone()
                }),
            )
            .paginate(25)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Scoped))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let mine = uuid::Uuid::new_v4();
    let theirs = uuid::Uuid::new_v4();
    toasty::create!(Scoped {
        tenant_id: mine,
        name: "Mine Widget"
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Scoped {
        tenant_id: theirs,
        name: "Theirs Widget"
    })
    .exec(&mut db)
    .await
    .unwrap();

    let router = mount(db, panel_for::<ScopedResource>()).expect("panel builds");
    // A server-set `Tenant` request extension supplies the tenant.
    let (mut parts, ()) = http::Request::builder()
        .uri("/admin/scoped")
        .body(())
        .unwrap()
        .into_parts();
    parts.extensions.insert(crate::Tenant(mine));
    let resp = router
        .handle(http::Request::from_parts(parts, Body::empty()))
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::OK,
        "gated list with tenant"
    );
    let html = String::from_utf8_lossy(
        &http_body_util::BodyExt::collect(resp.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .into_owned();
    assert!(
        html.contains("Mine Widget"),
        "the request tenant's row must render: {html}"
    );
    assert!(
        !html.contains("Theirs Widget"),
        "another tenant's row must not render: {html}"
    );
}

#[tokio::test]
async fn list_renders_error_state_when_load_fails() {
    use topcoat::router::Body;

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        email: String,
    }
    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn policy() -> impl Policy<Subscriber> {
            |_cx: &Cx, ability: Ability<'_, Subscriber>| matches!(ability, Ability::ViewAny)
        }

        fn table() -> Table<Self::Model> {
            // A realistic paginated table: the tampered cursor must reach
            // the decode inside `load_table_page` (only paginated loads
            // decode cursors), not die earlier on missing declarations.
            Table::<Subscriber>::new(
                |s| s.id.to_string(),
                crate::resource::TextColumn::r#for(
                    Subscriber::fields().email(),
                    |s: &Subscriber| s.email.clone(),
                ),
            )
            .paginate(25)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Subscriber))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    toasty::create!(Subscriber { email: "a@b.c" })
        .exec(&mut db)
        .await
        .unwrap();
    let router = mount(db, panel_for::<SubscriberResource>()).expect("panel builds");

    // A tampered `?after=` cursor fails to decode inside the list load
    // the load resolves the error view without pending, so
    // suspense renders it in the initial paint with status 200 — the
    // skeleton streams only while a load pends, and a fast failure
    // answers with no streamed region and no swap payload. The branded
    // ErrorState still answers the failed load in place (no 500, no
    // empty state), and the retry link drops the cursor that broke the
    // load. This test binary declares no `#[layout]`, so the body is
    // the page fragment: the page header is the "shell still stands"
    // evidence (the document-level wrap is proven by the layout tests).
    let response = router
        .handle(
            http::Request::builder()
                .uri("/admin/subscribers?after=zz-not-a-cursor")
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
        body.contains(">Subscribers</h1>"),
        "page header must survive the failure: {body}"
    );
    // A load that fails before pending renders in place: no skeleton,
    // no streamed region.
    assert!(
        !body.contains("topcoat::region::start"),
        "a fast failure must not open a streamed region: {body}"
    );
    assert!(
        body.contains("Couldn't load Subscribers"),
        "error state must render in place: {body}"
    );
    assert!(
        !body.contains("No records yet"),
        "a failed load is not an empty state: {body}"
    );
    // the tampered cursor is the failure itself, so the retry link
    // drops `after`/`before` instead of re-requesting the identical broken
    // URL forever. The rest of the list state still retries.
    assert!(
        body.contains("href=\"/admin/subscribers\""),
        "retry link must target the bare list (cursor dropped): {body}"
    );
    assert!(
        !body.contains("after="),
        "a malformed cursor must not travel into the retry link: {body}"
    );

    // a cursor that decodes but was cut from another ordering is
    // refused by the engine, not the decoder. It is the same retry
    // contract — drop pagination rather than loop on the identical URL.
    let stale = crate::cursor::encode(&toasty::stmt::Value::Record(
        toasty_core::stmt::ValueRecord::from_vec(vec![
            toasty::stmt::Value::String("a@b.c".to_string()),
            toasty::stmt::Value::String("x".to_string()),
            toasty::stmt::Value::I64(1),
        ]),
    ))
    .unwrap();
    let response = router
        .handle(
            http::Request::builder()
                .uri(format!("/admin/subscribers?after={stale}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(response.status().is_success());
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("Couldn't load Subscribers"),
        "a stale cursor must render the error state: {body}"
    );
    assert!(
        body.contains("href=\"/admin/subscribers\""),
        "the stale-cursor retry must target the bare list: {body}"
    );
    assert!(
        !body.contains("after="),
        "a stale cursor must not travel into the retry link: {body}"
    );
}

#[tokio::test]
async fn both_cursors_render_the_first_page() {
    // Toasty pages from one cursor, so a URL naming `?after=` and `?before=`
    // together parses as no cursor: the first page, which is where the
    // cursor retry lands anyway. Both tokens below are valid.
    use topcoat::router::Body;

    #[derive(Debug, toasty::Model, Clone)]
    struct Subscriber {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[unique]
        email: String,
    }
    struct SubscriberResource;
    impl Resource for SubscriberResource {
        type Model = Subscriber;
        type Form = crate::NoForm<Self::Model>;

        fn policy() -> impl Policy<Subscriber> {
            |_cx: &Cx, ability: Ability<'_, Subscriber>| matches!(ability, Ability::ViewAny)
        }

        fn table() -> Table<Self::Model> {
            Table::<Subscriber>::new(
                |s| s.id.to_string(),
                crate::resource::TextColumn::r#for(
                    Subscriber::fields().email(),
                    |s: &Subscriber| s.email.clone(),
                ),
            )
            .paginate(1)
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
    let table = SubscriberResource::table();
    let first = load_table_page::<SubscriberResource>(&cx, &table, &TableState::default())
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
        cursor: Some(crate::resource::Cursor::After("cur".to_string())),
        ..TableState::default()
    };
    let bad_cursor = crate::cursor::decode("zz").expect_err("malformed cursor must fail");
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
