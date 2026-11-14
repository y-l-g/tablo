//! `url::auth`: the pages auth serves, named without spelling a prefix.

use tablo::{
    Auth, Page, PageDef, Panel, PasswordSignUp, RouterBuilderPanelExt,
    auth::{AdminUser, AuthSession, create_admin},
    url::{self, AuthPage},
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
    view::{View, view},
};

use crate::common::{TestClient, body_string, test_db};

/// Lists each auth page's URL on the panel serving it, `-` where the panel serves none.
struct Urls;

impl Page for Urls {
    fn declare() -> PageDef {
        PageDef::new().slug("urls")
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let line = [
            AuthPage::Login,
            AuthPage::Logout,
            AuthPage::Register,
            AuthPage::ForgotPassword,
            AuthPage::Profile,
            AuthPage::SwitchTenant,
        ]
        .map(|page| url::auth(cx, page).unwrap_or_else(|| "-".to_owned()))
        .join(" ");
        Ok(view! { <p id="urls">(line)</p> })
    }
}

async fn db() -> Db {
    test_db(toasty::models!(AdminUser, AuthSession)).await
}

fn router(db: &Db, panel: Panel) -> Router {
    let mut builder = Router::builder().discover().with(db.clone());
    builder.panel(panel).expect("the panel mounts");
    builder.build()
}

async fn urls(client: &TestClient<'_>, path: &str) -> String {
    let html = body_string(client.get(path).await).await;
    let (_, rest) = html
        .split_once(r#"<p id="urls">"#)
        .expect("the page lists the URLs");
    rest.split_once("</p>").expect("the list ends").0.to_owned()
}

/// A page auth serves has a URL under the panel's prefix; one the panel's `Auth` leaves out has
/// none.
#[tokio::test]
async fn an_auth_page_has_a_url_only_where_the_panel_serves_it() {
    let mut db = db().await;
    let admin = create_admin(&mut db, "admin@example.com", "secret", "Admin")
        .await
        .expect("seed the admin");
    let router = router(
        &db,
        Panel::new("back-office")
            .auth(Auth::password().registration(PasswordSignUp::pending()))
            .page::<Urls>(),
    );
    let client = TestClient::new(&router)
        .sign_in(&db, "back-office", &admin)
        .await;

    assert_eq!(
        urls(&client, "/back-office/urls").await,
        "/back-office/login /back-office/logout /back-office/register - - -"
    );
}

/// A panel without auth serves no sign-in page, so it has no URL.
#[tokio::test]
async fn a_panel_without_auth_has_no_auth_url() {
    let db = db().await;
    let router = router(
        &db,
        Panel::new("admin").auth(Auth::disabled()).page::<Urls>(),
    );

    assert_eq!(
        urls(&TestClient::new(&router), "/admin/urls").await,
        "- - - - - -"
    );
}
