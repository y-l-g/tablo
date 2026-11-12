//! The Adding a panel to an existing app chapter's snippets.

use std::time::Duration;

use tablo::{Auth, ExternalAuth, PanelUser, auth, prelude::*};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    cookie::CookieLayer,
    router::{Router, RouterBuilderDiscoverExt},
    session::{self, SessionLifetime, TokenHash},
};

pub struct Employee {
    pub id: String,
    pub name: String,
    pub staff: bool,
}

impl PanelUser for Employee {
    fn user_id(&self) -> String {
        self.id.clone()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn can_access_panel(&self, panel: &tablo::PanelId) -> bool {
        panel == "/admin" && self.staff
    }
}

async fn employee_for(_cx: &Cx, _session: &TokenHash) -> Result<Option<Employee>> {
    todo!()
}

// ANCHOR: existing-app-external
/// The app's own sessions, as the panel reads them.
pub struct AppSession;

impl ExternalAuth for AppSession {
    type User = Employee;

    async fn current_user(&self, cx: &Cx) -> Result<Option<Employee>> {
        match session::token_hash(cx).await? {
            Some(hash) => employee_for(cx, &hash).await,
            None => Ok(None),
        }
    }

    fn login_url(&self, _cx: &Cx, next: &str) -> String {
        format!("/login?return_to={next}")
    }

    fn logout_url(&self, _cx: &Cx) -> String {
        "/sign-out".to_string()
    }
}
// ANCHOR_END: existing-app-external

// ANCHOR: existing-app-router-body
pub fn app_router(db: Db) -> Result<Router> {
    let mut builder = Router::builder()
        .discover() // the app's own pages and routes, and Tablo's
        .with(db)
        .layer(CookieLayer::new())
        .with(SessionLifetime(Duration::from_hours(24 * 14))); // the app's sessions, untouched by the panel
    builder.panel(Panel::new("admin").auth(Auth::external(AppSession)))?;
    Ok(builder.build())
}
// ANCHOR_END: existing-app-router-body

// ANCHOR: existing-app-return
/// Where the app's login page sends a user it just signed in.
pub fn landing(return_to: Option<&str>) -> &str {
    return_to.and_then(auth::safe_next).unwrap_or("/")
}
// ANCHOR_END: existing-app-return
