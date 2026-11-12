//! A host app with its own users, its own Topcoat sessions and its own sign-in routes.

use std::time::Duration;

use tablo::testing::{TestClient, response_cookies};
use toasty::Db;
use topcoat::{
    context::Cx,
    cookie::CookieLayer,
    router::{
        Body, RouteFn, RouteFuture, RouterBuilder, error::see_other, request::uri,
        response::IntoResponse,
    },
    session::{self, SessionLifetime},
};
use uuid::Uuid;

/// The cookie the host's Topcoat sessions ride in.
pub const HOST_COOKIE: &str = "__Host-session";

/// The host's user table.
#[derive(Debug, Clone, toasty::Model)]
pub struct Employee {
    #[key]
    #[auto]
    pub id: Uuid,
    pub name: String,
    pub admin: bool,
}

/// The host's session table.
#[derive(Debug, Clone, toasty::Model)]
pub struct HostSession {
    #[key]
    pub token_hash: String,
    pub employee: Uuid,
}

impl tablo::PanelUser for Employee {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn can_access_panel(&self, _panel: &tablo::PanelId) -> bool {
        self.admin
    }
}

pub async fn employee(db: &Db, name: &str, admin: bool) -> Employee {
    toasty::create!(Employee {
        name: name.to_string(),
        admin,
    })
    .exec(&mut db.clone())
    .await
    .expect("seed the employee")
}

fn key(hash: &session::TokenHash) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The employee the request's host session signs in.
pub async fn current_employee(cx: &Cx) -> topcoat::Result<Option<Employee>> {
    let Some(hash) = session::token_hash(cx).await? else {
        return Ok(None);
    };
    let mut db = tablo::db::db(cx)?;
    let row = HostSession::filter(HostSession::fields().token_hash().eq(key(&hash)))
        .first()
        .exec(&mut db)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Employee::filter(Employee::fields().id().eq(row.employee))
        .first()
        .exec(&mut db)
        .await?)
}

/// `POST /app/login?{employee id}` signs the employee in to the host.
fn login(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let employee: Uuid = uri(cx)
            .query()
            .and_then(|id| id.parse().ok())
            .ok_or_else(topcoat::router::error::not_found)?;
        let started = session::start(cx).await?;
        toasty::create!(HostSession {
            token_hash: key(&started.token_hash),
            employee,
        })
        .exec(&mut tablo::db::db(cx)?)
        .await?;
        see_other("/app/me").into_response(cx)
    })
}

/// `POST /app/logout` ends the host session.
fn logout(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        if let Some(hash) = session::stop(cx).await? {
            HostSession::filter(HostSession::fields().token_hash().eq(key(&hash)))
                .delete()
                .exec(&mut tablo::db::db(cx)?)
                .await?;
        }
        see_other("/app/me").into_response(cx)
    })
}

/// `GET /app/me` answers the signed-in employee's name, or `anonymous`.
fn me(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let name = current_employee(cx)
            .await?
            .map_or_else(|| "anonymous".to_string(), |employee| employee.name);
        name.into_response(cx)
    })
}

/// The host's routes and its sessions, which live `lifetime`, ahead of any panel.
pub fn host_routes(builder: RouterBuilder, lifetime: Duration) -> RouterBuilder {
    let path = |path: &str| {
        topcoat::router::Path::from_str(path)
            .expect("a literal route path")
            .to_owned()
    };
    builder
        .layer(CookieLayer::new())
        .with(SessionLifetime(lifetime))
        .route(RouteFn::new(http::Method::POST, path("/app/login"), login))
        .route(RouteFn::new(
            http::Method::POST,
            path("/app/logout"),
            logout,
        ))
        .route(RouteFn::new(http::Method::GET, path("/app/me"), me))
}

/// `client` signed in to the host as `employee`.
pub async fn host_login<'a>(client: &TestClient<'a>, employee: &Employee) -> TestClient<'a> {
    let response = client
        .post_form(&format!("/app/login?{}", employee.id), String::new())
        .await;
    client.cookies(&response_cookies(&response))
}
