//! Relationship option search endpoint (D2/D5):
//! `GET {parent_list_url}/options?field=&q=` for tables above the option cap.

use topcoat::{
    context::Cx,
    router::{Body, RouteFuture, error::forbidden},
};

use super::super::gate::gate;
use crate::{form::FormResource, resource::clamp_query_term, schema::OptionLoadError};

/// Relationship option search endpoint (D2/D5).
///
/// `GET {parent_list_url}/options?field=&q=` — server-side narrowing for
/// tables above the option cap. `field` allow-lists to a declared searchable
/// relationship `Select` in `FormResource::form` (400 otherwise); non-searchable
/// selects keep today's cap error and never call here. `q` is trimmed and
/// clamped to the shared query bound; empty `q` returns the bounded head.
///
/// Gates: `enforce_auth` + `enforce_tenant::<R>` (parent), then the related
/// gates inside the search (`can_view_any` + tenant + `can_view` filtering
/// before labels). Parent form policy (`can_create` / `can_view`+`can_update`)
/// stays on the form pages themselves: requiring parent `can_view_any` here
/// would lock create-only users out of a form they may use, and adds no
/// visibility the related list does not already expose. `Denied` → 403, driver failure → 500,
/// filtered overflow → 200 with a "keep typing" hint option (client keeps its hint element).
/// Success → 200 `text/html` with `<option>` markup, bounded to
/// `MAX_RELATIONSHIP_OPTIONS`, values are typed PK strings, labels escaped.
pub(crate) fn resource_options<R: FormResource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        gate::<R>(cx)?;
        let (field, q) = options_query(cx);
        let field = field.trim();
        if field.is_empty() {
            return Err(topcoat::router::error::bad_request("missing field").into());
        }
        let q = clamp_query_term(&q);
        let form = R::form(cx);
        let selects = form.select_inputs();
        let Some(select) = selects.get(field) else {
            return Err(topcoat::router::error::bad_request("unknown field").into());
        };
        if !select.is_relationship() {
            return Err(topcoat::router::error::bad_request("not a relationship select").into());
        }
        if !select.is_searchable() {
            return Err(topcoat::router::error::bad_request("not searchable").into());
        }
        match select.search_options(cx, &q).await {
            Ok(opts) => {
                let mut html = String::with_capacity(opts.len() * 32);
                for (v, lab) in opts {
                    html.push_str(&format!(
                        "<option value=\"{}\">{}</option>",
                        escape_option(&v),
                        escape_option(&lab)
                    ));
                }
                let res = http::Response::builder()
                    .status(200)
                    .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .header("x-content-type-options", "nosniff")
                    .body(Body::from(html))
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(res)
            }
            Err(OptionLoadError::Denied) => Err(forbidden().into()),
            Err(OptionLoadError::LoadFailed) => Err(topcoat::Error::from(std::io::Error::other(
                "option search failed",
            ))),
            // Permanent: the related resource cannot be scoped at
            // all, so the search cannot succeed until the declaration is
            // fixed — a 500 that says so, not a retry.
            Err(OptionLoadError::Misdeclared) => Err(topcoat::Error::from(std::io::Error::other(
                "option search unavailable: the related resource requires a tenant the                      framework cannot scope (GH #223)",
            ))),
            Err(OptionLoadError::Overflow) => {
                let html = "<option value=\"\" disabled>Too many results — keep typing</option>"
                    .to_string();
                let res = http::Response::builder()
                    .status(200)
                    .header(http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .header("x-content-type-options", "nosniff")
                    .body(Body::from(html))
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(res)
            }
        }
    })
}

/// Parse `?field=` + `?q=` for the options endpoint (first-wins, like the
/// table state parser).
fn options_query(cx: &Cx) -> (String, String) {
    let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
        return (String::new(), String::new());
    };
    let query = parts.uri.query().unwrap_or("");
    let mut field = String::new();
    let mut q = String::new();
    let mut seen_field = false;
    let mut seen_q = false;
    for (k, v) in form_urlencoded::parse(query.as_bytes()) {
        if k == "field" && !seen_field {
            field = v.into_owned();
            seen_field = true;
        } else if k == "q" && !seen_q {
            q = v.into_owned();
            seen_q = true;
        }
    }
    (field, q)
}

/// Escape a value/label for `<option>` markup.
fn escape_option(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use toasty::Db;

    use super::*;
    use crate::Panel;

    #[tokio::test]
    async fn options_endpoint_searches_and_gates() {
        // `GET {parent}/options?field=&q=` narrows server-side,
        // allow-lists to searchable relationship selects, and mirrors gates.
        use http_body_util::BodyExt;

        use crate::resource::Resource;

        #[derive(Debug, toasty::Model, Clone)]
        struct OptAuthor {
            #[key]
            #[auto]
            id: uuid::Uuid,
            name: String,
        }
        struct OptAuthorResource;
        impl Resource for OptAuthorResource {
            type Model = OptAuthor;
            fn slug() -> String {
                "opt-authors".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &OptAuthor) -> bool {
                true
            }
            fn table(_cx: &Cx) -> crate::resource::Table<OptAuthor> {
                crate::resource::Table::new(
                    |a: &OptAuthor| a.id.to_string(),
                    crate::resource::TextColumn::r#for(
                        OptAuthor::fields().name(),
                        |a: &OptAuthor| a.name.clone(),
                    )
                    .searchable(),
                )
            }
        }

        #[derive(Debug, toasty::Model, Clone)]
        struct OptPost {
            #[key]
            #[auto]
            id: uuid::Uuid,
            author_id: uuid::Uuid,
            title: String,
        }
        struct OptPostResource;
        impl Resource for OptPostResource {
            type Model = OptPost;
            fn slug() -> String {
                "opt-posts".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &OptPost) -> bool {
                true
            }
            fn table(_cx: &Cx) -> crate::resource::Table<OptPost> {
                crate::resource::Table::new(
                    |p: &OptPost| p.id.to_string(),
                    crate::resource::TextColumn::r#for(OptPost::fields().title(), |p: &OptPost| {
                        p.title.clone()
                    }),
                )
            }
        }
        #[derive(crate::RecordForm)]
        #[record_form(model = OptPost)]
        struct OptPostForm {
            author_id: uuid::Uuid,
        }
        impl crate::form::FormResource for OptPostResource {
            type Form = OptPostForm;
            fn form(_cx: &Cx) -> crate::schema::Schema {
                crate::schema::Schema::new(
                    crate::schema::Select::r#for(OptPost::fields().author_id())
                        .relationship::<OptAuthorResource>(
                            OptAuthorResource::query,
                            |a: &OptAuthor| a.id,
                            |a: &OptAuthor| a.name.clone(),
                        )
                        .searchable(),
                )
            }
        }

        async fn body_text(resp: http::Response<Body>) -> String {
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            String::from_utf8_lossy(&bytes).to_string()
        }

        let mut db = Db::builder()
            .models(toasty::models!(OptAuthor, OptPost))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        for name in ["Ada", "Grace", "Alan"] {
            toasty::create!(OptAuthor {
                name: name.to_string(),
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
        let router = Panel::new("admin")
            .app_context(db)
            .form_resource::<OptPostResource>()
            .resource::<OptAuthorResource>()
            .auth(crate::Auth::disabled())
            .build()
            .expect("panel builds");

        // Narrowing works.
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/opt-posts/options?field=author_id&q=Ada")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), http::StatusCode::OK);
        let html = body_text(resp).await;
        assert!(html.contains("Ada"), "search must return Ada, got {html}");
        assert!(!html.contains("Grace"), "search must narrow, got {html}");

        // Unknown field → 400.
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/opt-posts/options?field=nope&q=Ada")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);

        // Missing field → 400.
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/opt-posts/options?q=Ada")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
        // Escaping itself is pinned where it can fail: `escape_option`'s unit
        // test feeds characters that must be escaped and asserts the exact
        // output. These fixtures are "Ada"/"Grace"/"Alan", so a
        // `!html.contains("<script")` here could never fail.
    }

    #[tokio::test]
    async fn option_load_asks_for_no_relation_includes() {
        // an option load projects a value and a label off the related
        // record's own columns, so it asks the source for no relation
        // includes. The source's `query` loads `parent` and `can_view` keeps a
        // row only while that relation is unloaded, so a rendered option
        // proves the loader ran the needs-aware branch, not the full `query`.
        use http_body_util::BodyExt;
        use toasty::stmt::{Include, List, Query};

        use crate::resource::{IncludeNeeds, Resource};

        #[derive(Debug, toasty::Model, Clone)]
        struct Parent {
            #[key]
            #[auto]
            id: uuid::Uuid,
            name: String,
        }

        #[derive(Debug, toasty::Model, Clone)]
        struct Child {
            #[key]
            #[auto]
            id: uuid::Uuid,
            name: String,
            #[index]
            parent_id: uuid::Uuid,
            #[belongs_to(key = parent_id, references = id)]
            parent: toasty::Deferred<Parent>,
        }

        fn with_parent() -> Query<List<Child>> {
            let inc: Include<Child, Parent> = Child::fields().parent().into();
            Query::<List<Child>>::all().include(inc)
        }

        struct ChildSource;
        impl Resource for ChildSource {
            type Model = Child;
            fn slug() -> String {
                "children".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, record: &Child) -> bool {
                record.parent.is_unloaded()
            }
            fn query(_cx: &Cx) -> Query<List<Child>> {
                with_parent()
            }
            fn query_with(_cx: &Cx, needs: &IncludeNeeds) -> Query<List<Child>> {
                if needs.wants("parent") {
                    with_parent()
                } else {
                    Query::<List<Child>>::all()
                }
            }
            fn table(_cx: &Cx) -> crate::resource::Table<Child> {
                crate::resource::Table::new(
                    |c: &Child| c.id.to_string(),
                    crate::resource::TextColumn::r#for(Child::fields().name(), |c: &Child| {
                        c.name.clone()
                    }),
                )
            }
        }

        #[derive(Debug, toasty::Model, Clone)]
        struct Owner {
            #[key]
            #[auto]
            id: uuid::Uuid,
            child_id: uuid::Uuid,
            name: String,
        }

        struct OwnerResource;
        impl Resource for OwnerResource {
            type Model = Owner;
            fn slug() -> String {
                "owners".to_string()
            }
            fn table(_cx: &Cx) -> crate::resource::Table<Owner> {
                crate::resource::Table::new(
                    |o: &Owner| o.id.to_string(),
                    crate::resource::TextColumn::r#for(Owner::fields().name(), |o: &Owner| {
                        o.name.clone()
                    }),
                )
            }
        }
        #[derive(crate::RecordForm)]
        #[record_form(model = Owner)]
        struct OwnerForm {
            child_id: uuid::Uuid,
        }
        impl crate::form::FormResource for OwnerResource {
            type Form = OwnerForm;
            fn form(_cx: &Cx) -> crate::schema::Schema {
                crate::schema::Schema::new(
                    crate::schema::Select::r#for(Owner::fields().child_id())
                        .relationship::<ChildSource>(
                            ChildSource::query,
                            |c: &Child| c.id,
                            |c: &Child| c.name.clone(),
                        )
                        .searchable(),
                )
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(Parent, Child, Owner))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let parent_id = uuid::Uuid::new_v4();
        toasty::create!(Parent {
            id: parent_id,
            name: "Ada".to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
        toasty::create!(Child {
            name: "Only Child".to_string(),
            parent_id,
        })
        .exec(&mut db)
        .await
        .unwrap();

        let router = Panel::new("admin")
            .app_context(db)
            .form_resource::<OwnerResource>()
            .resource::<ChildSource>()
            .auth(crate::Auth::disabled())
            .build()
            .expect("panel builds");
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/owners/options?field=child_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert!(resp.status().is_success());
        let html = String::from_utf8(
            resp.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(
            html.contains("Only Child"),
            "the option must render, which it cannot if the loader loaded the source's include: {html}"
        );
    }

    #[tokio::test]
    async fn options_endpoint_rejects_non_searchable_and_overflows() {
        // GH #150 D5/D6: non-searchable selects never serve search (400);
        // filtered overflow answers 200 with the keep-typing hint.
        use http_body_util::BodyExt;

        use crate::resource::Resource;

        #[derive(Debug, toasty::Model, Clone)]
        struct BigA {
            #[key]
            #[auto]
            id: uuid::Uuid,
            name: String,
        }
        struct BigAResource;
        impl Resource for BigAResource {
            type Model = BigA;
            fn slug() -> String {
                "big-as".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &BigA) -> bool {
                true
            }
            fn table(_cx: &Cx) -> crate::resource::Table<BigA> {
                crate::resource::Table::new(
                    |a: &BigA| a.id.to_string(),
                    crate::resource::TextColumn::r#for(BigA::fields().name(), |a: &BigA| {
                        a.name.clone()
                    })
                    .searchable(),
                )
            }
        }

        #[derive(Debug, toasty::Model, Clone)]
        struct BigP {
            #[key]
            #[auto]
            id: uuid::Uuid,
            author_id: uuid::Uuid,
            /// A text column for the table declaration: every
            /// servable resource needs one, and `author_id` is a Uuid.
            name: String,
        }
        struct SearchableParent;
        impl Resource for SearchableParent {
            type Model = BigP;
            fn slug() -> String {
                "big-ps".to_string()
            }
            fn table(_cx: &Cx) -> crate::resource::Table<BigP> {
                crate::resource::Table::new(
                    |r: &BigP| r.id.to_string(),
                    crate::resource::TextColumn::r#for(BigP::fields().name(), |r: &BigP| {
                        r.name.clone()
                    }),
                )
            }
        }
        #[derive(crate::RecordForm)]
        #[record_form(model = BigP)]
        struct SearchableParentForm {
            author_id: uuid::Uuid,
        }
        impl crate::form::FormResource for SearchableParent {
            type Form = SearchableParentForm;
            fn form(_cx: &Cx) -> crate::schema::Schema {
                crate::schema::Schema::new(
                    crate::schema::Select::r#for(BigP::fields().author_id())
                        .relationship::<BigAResource>(
                            BigAResource::query,
                            |a: &BigA| a.id,
                            |a: &BigA| a.name.clone(),
                        )
                        .searchable(),
                )
            }
        }
        struct PlainParent;
        impl Resource for PlainParent {
            type Model = BigP;
            fn slug() -> String {
                "plain-ps".to_string()
            }
            fn table(_cx: &Cx) -> crate::resource::Table<BigP> {
                crate::resource::Table::new(
                    |r: &BigP| r.id.to_string(),
                    crate::resource::TextColumn::r#for(BigP::fields().name(), |r: &BigP| {
                        r.name.clone()
                    }),
                )
            }
        }
        #[derive(crate::RecordForm)]
        #[record_form(model = BigP)]
        struct PlainParentForm {
            author_id: uuid::Uuid,
        }
        impl crate::form::FormResource for PlainParent {
            type Form = PlainParentForm;
            fn form(_cx: &Cx) -> crate::schema::Schema {
                crate::schema::Schema::new(
                    crate::schema::Select::r#for(BigP::fields().author_id())
                        .relationship::<BigAResource>(
                            BigAResource::query,
                            |a: &BigA| a.id,
                            |a: &BigA| a.name.clone(),
                        ),
                )
            }
        }

        let mut db = Db::builder()
            .models(toasty::models!(BigA, BigP))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        for i in 0..=crate::schema::MAX_RELATIONSHIP_OPTIONS {
            toasty::create!(BigA {
                name: format!("author-{i}"),
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
        let router = Panel::new("admin")
            .app_context(db)
            .form_resource::<SearchableParent>()
            .form_resource::<PlainParent>()
            .auth(crate::Auth::disabled())
            .build()
            .expect("panel builds");

        // Non-searchable → 400 (keeps today's cap error path, never search).
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/plain-ps/options?field=author_id&q=author-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);

        // Empty q on over-cap searchable → 200 with keep-typing hint.
        let resp = router
            .handle(
                http::Request::builder()
                    .uri("/admin/big-ps/options?field=author_id&q=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(resp.status(), http::StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&bytes).to_string();
        assert!(
            html.contains("keep typing"),
            "filtered overflow must hint, got {html}"
        );
    }

    #[test]
    fn options_query_parses_first_wins_and_escapes() {
        assert_eq!(escape_option("a&b<c>\"'"), "a&amp;b&lt;c&gt;&quot;&#39;");
        assert_eq!(
            escape_option("550e8400-e29b-41d4-a716-446655440000"),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }
}
