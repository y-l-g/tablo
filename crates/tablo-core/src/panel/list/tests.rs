use toasty::Db;

use super::*;
use crate::{
    Ability, ResourceDef, lens,
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

/// Asserts two lists declare distinct signal ids.
#[tokio::test]
async fn two_lists_declare_distinct_signal_ids() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    macro_rules! list_resource {
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
                        .table(dummy_table().paginate(25))
                }
            }
        };
    }
    list_resource!(FirstResource, "firsts");
    list_resource!(SecondResource, "seconds");

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

/// A list reads its state from its query signal: a rerun carrying another query renders that
/// query's rows, whatever the URL says.
#[tokio::test]
async fn a_rerun_renders_the_query_its_signal_carries() {
    use http_body_util::BodyExt;

    struct SearchResource;
    impl Resource for SearchResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny))
                .table(crate::table::Table::new(
                    crate::table::TextColumn::new(lens!(Dummy.name)).searchable(),
                ))
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
    let router = mount(db, panel_for::<SearchResource>()).expect("panel builds");
    let read = |resp: http::Response<Body>| async move {
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8_lossy(&body).to_string()
    };

    let html = read(
        router
            .handle(
                http::Request::builder()
                    .uri("/admin/dummies?q=Bob")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await,
    )
    .await;
    assert!(html.contains(">Bob<") && !html.contains(">Ada<"), "{html}");
    let declaration = html
        .split("::topcoat::signal(")
        .skip(1)
        .find(|declaration| declaration.contains("&quot;v&quot;:&quot;q=Bob&quot;"))
        .unwrap_or_else(|| panic!("the page must declare its query signal, got {html}"));
    let id = declaration
        .split("&quot;id&quot;:&quot;")
        .nth(1)
        .and_then(|rest| rest.split("&quot;").next())
        .expect("a signal id");

    let rerun = read(
        router
            .handle(
                http::Request::builder()
                    .method(http::Method::POST)
                    .uri("/admin/dummies?q=Bob")
                    .header("X-Topcoat-Runtime", "true")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(r#"{{"signals":{{"{id}":"q=Ada"}}}}"#)))
                    .unwrap(),
            )
            .await,
    )
    .await;
    assert!(
        rerun.contains(">Ada<") && !rerun.contains(">Bob<"),
        "{rerun}"
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
        !html.contains("Delete selected"),
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

/// Asserts the GET `?q=` term is clamped to `MAX_QUERY_TERM`.
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
        "the GET term is clamped to MAX_QUERY_TERM"
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
    let cx = crate::test_support::panel_cx::<SubscriberResource>(&db).with(parts);
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
        !body.split("href=\"").skip(1).any(|href| href
            .split('"')
            .next()
            .unwrap_or_default()
            .contains("before=")),
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
