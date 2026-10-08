//! Shares the fixtures the crate's unit tests build on.

use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

pub(crate) fn cx() -> Cx {
    CxTestBuilder::new().build()
}

/// Builds a `Cx` carrying `name=value` when `value` is `Some`.
pub(crate) fn cx_with_cookie(name: &str, value: Option<&str>) -> Cx {
    let mut parts = http::Request::builder()
        .uri("/")
        .body(())
        .unwrap()
        .into_parts()
        .0;
    if let Some(value) = value {
        parts.headers.insert(
            http::header::COOKIE,
            format!("{name}={value}").parse().unwrap(),
        );
    }
    CxTestBuilder::new()
        .request_context(parts)
        .request_context(topcoat::cookie::CookieJarCell::new())
        .build()
}

/// Names a row for tests that only need one.
#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct User {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
}

/// The three-column model schema tests resolve lenses against.
#[derive(Debug, Clone, toasty::Model)]
pub(crate) struct DummyUser {
    #[key]
    #[auto]
    pub(crate) id: uuid::Uuid,
    pub(crate) name: String,
    #[unique]
    pub(crate) email: String,
}

/// A context outside any request where `R` answers as a panel mounting only it would.
pub(crate) fn panel_cx<R: crate::Resource>(db: &toasty::Db) -> Cx {
    crate::Panel::new("admin")
        .resource::<R>()
        .context(db)
        .expect("panel builds")
}

/// `R`'s declaration with its defaults filled in at a `/admin` prefix, for a unit that takes the
/// mounted def directly; a panel mounting `R` may adjust the def with `resource_with`.
pub(crate) fn mounted<R: crate::Resource>() -> std::sync::Arc<crate::resource::Mounted<R>> {
    std::sync::Arc::new(crate::resource::Mounted::new(
        R::declare(),
        "/admin",
        &crate::schema::FieldResolver::of(&cx()),
    ))
}

/// A fresh in-memory SQLite `Db` with the tables of `models` pushed.
pub(crate) async fn memory_db(models: toasty::schema::ModelSet) -> toasty::Db {
    let db = toasty::Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");
    db.push_schema().await.expect("push the schema");
    db
}

/// Reads a rendered view's HTML in one pass, for the markup tests.
pub(crate) trait Html {
    /// The HTML of the view this result holds, panicking on a render error.
    async fn html(self, cx: &Cx) -> String;
}

impl<V: topcoat::view::View> Html for topcoat::Result<V> {
    async fn html(self, cx: &Cx) -> String {
        self.expect("the view renders")
            .single()
            .await
            .expect("the view resolves")
            .render(cx)
    }
}

/// An in-memory SQLite `Db` that knows `models` but holds no tables: enough to mount a panel, and
/// a driver failure for any query.
pub(crate) async fn tableless_db(models: toasty::schema::ModelSet) -> toasty::Db {
    toasty::Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite")
}
