//! A second factor across panels: the panels of one user type share it, and a panel of another
//! user type does not.

use http::header::LOCATION;
use tablo::{
    Auth, Authenticator, Panel, PanelUser, RouterBuilderPanelExt, TwoFactor,
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, create_admin, session_cookie},
    testing::{
        TestClient, authenticator_code_at, body_string, form_body, now, recovery_codes,
        session_cookie_value, two_factor_secret, two_factor_state,
    },
};
use toasty::Db;
use topcoat::{
    context::Cx,
    cookie::Key,
    router::{Router, RouterBuilderDiscoverExt},
};

use crate::common::test_db;

const PASSWORD: &str = "correct horse battery staple";

/// An account of another user type whose id is an admin's: two user tables may hold one id.
#[derive(Debug)]
struct Guest(AdminUser);

impl PanelUser for Guest {
    fn user_id(&self) -> String {
        self.0.user_id()
    }

    fn display_name(&self) -> &str {
        self.0.display_name()
    }

    fn can_access_panel(&self, _panel: &tablo::PanelId) -> bool {
        true
    }
}

/// Loads a guest by its id; signs nobody in by password, since the test signs in directly.
struct GuestAuth;

impl Authenticator for GuestAuth {
    type User = Guest;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<Guest>> {
        Ok(None)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<Guest>> {
        let mut db = tablo::db::db(cx)?;
        let admins = AdminUser::all().exec(&mut db).await?;
        Ok(admins
            .into_iter()
            .find(|admin| admin.user_id() == id)
            .map(Guest))
    }
}

/// A database holding Ada, whose password is [`PASSWORD`].
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(
        AdminUser,
        AuthSession,
        AuthToken,
        AuthFactor
    ))
    .await;
    let ada = create_admin(&mut db.clone(), "ada@example.com", PASSWORD, "Ada")
        .await
        .unwrap();
    (db, ada)
}

/// The `admin` and `staff` panels, both signing admins in by password; `staff` requires a second
/// factor. The `guests` panel signs in another user type and requires one too.
fn router(db: Db) -> Router {
    let mut builder = Router::builder().discover().with(db).with(Key::generate());
    builder
        .panel(Panel::new("admin").auth(Auth::password().two_factor(TwoFactor::new())))
        .unwrap();
    builder
        .panel(Panel::new("staff").auth(Auth::password().two_factor(TwoFactor::new().required())))
        .unwrap();
    builder
        .panel(
            Panel::new("guests")
                .auth(Auth::custom(GuestAuth).two_factor(TwoFactor::new().required())),
        )
        .unwrap();
    builder.build()
}

/// Ada turns two-factor sign-in on from the `admin` panel; answers her authenticator's secret and
/// her recovery codes.
async fn enrol(router: &Router, db: &Db, ada: &AdminUser) -> (String, Vec<String>) {
    let client = TestClient::new(router).sign_in(db, "admin", ada).await;
    let started = client
        .submit(
            "/admin/two-factor",
            &form_body(&[("intent", "start"), ("current_password", PASSWORD)]),
        )
        .await;
    let secret = two_factor_secret(&body_string(started).await).expect("the set-up shows its key");
    let confirmed = client
        .submit(
            "/admin/two-factor",
            &form_body(&[("intent", "confirm"), ("code", &code(&secret, 0))]),
        )
        .await;
    assert_eq!(confirmed.status(), 200);
    (secret, recovery_codes(&body_string(confirmed).await))
}

/// The code an authenticator holding `secret` shows `steps` 30-second steps from now.
fn code(secret: &str, steps: i64) -> String {
    authenticator_code_at(secret, now().saturating_add_signed(steps * 30))
}

/// Ada's password posted to `panel`'s login form: the client and where the panel sends it.
async fn password<'r>(router: &'r Router, panel: &str) -> (TestClient<'r>, String, bool) {
    let (client, response) = TestClient::new(router)
        .log_in(panel, "ada@example.com", PASSWORD, "")
        .await;
    assert_eq!(response.status(), 303);
    let location = response.headers().get(LOCATION).unwrap().to_str().unwrap();
    (
        client,
        location.to_string(),
        session_cookie_value(&response).is_some(),
    )
}

/// `panel`'s challenge answered with `code`.
async fn answer(
    client: &TestClient<'_>,
    panel: &str,
    code: &str,
) -> http::Response<topcoat::router::Body> {
    client
        .submit(
            &format!("/{panel}/two-factor-challenge"),
            &form_body(&[("code", code)]),
        )
        .await
}

/// A factor set up on one panel stands on a sibling of the same user type: the password alone
/// opens no session there and sets up no factor, and the same authenticator signs in.
#[tokio::test]
async fn a_factor_stands_on_every_panel_of_its_user_type() {
    let (db, ada) = db().await;
    let router = router(db.clone());
    let (secret, _) = enrol(&router, &db, &ada).await;

    let (challenged, location, signed_in) = password(&router, "staff").await;
    assert!(
        location.starts_with("/staff/two-factor-challenge"),
        "{location}"
    );
    assert!(!signed_in, "a password alone opens no session on a sibling");
    assert_eq!(
        challenged.get("/staff/two-factor").await.status(),
        307,
        "the challenge reaches no set-up"
    );

    let response = answer(&challenged, "staff", &code(&secret, 1)).await;
    assert_eq!(response.status(), 303);
    let session = session_cookie_value(&response).expect("the code opens a session");
    let client = TestClient::new(&router).cookie(&session_cookie("staff"), &session);
    let page = body_string(client.get("/staff/two-factor").await).await;
    assert_eq!(
        two_factor_state(&page).as_deref(),
        Some("on"),
        "the sibling requires a factor and holds this one"
    );
}

/// A recovery code issued on one panel signs in to a sibling, once on either.
#[tokio::test]
async fn a_recovery_code_signs_in_to_a_sibling_once() {
    let (db, ada) = db().await;
    let router = router(db.clone());
    let (_, codes) = enrol(&router, &db, &ada).await;

    let (challenged, _, _) = password(&router, "staff").await;
    assert_eq!(answer(&challenged, "staff", &codes[0]).await.status(), 303);
    let (again, _, _) = password(&router, "admin").await;
    assert_eq!(answer(&again, "admin", &codes[0]).await.status(), 422);
    assert_eq!(answer(&again, "admin", &codes[1]).await.status(), 303);
}

/// Turning the factor off on a sibling turns it off on the panel that set it up.
#[tokio::test]
async fn turning_it_off_on_a_sibling_turns_it_off_everywhere() {
    let (db, ada) = db().await;
    let router = router(db.clone());
    let (secret, codes) = enrol(&router, &db, &ada).await;

    let (challenged, _, _) = password(&router, "staff").await;
    let response = answer(&challenged, "staff", &code(&secret, 1)).await;
    let session = session_cookie_value(&response).expect("the code opens a session");
    let client = TestClient::new(&router).cookie(&session_cookie("staff"), &session);
    let disabled = client
        .submit(
            "/staff/two-factor",
            &form_body(&[("intent", "disable"), ("code", &codes[0])]),
        )
        .await;
    assert_eq!(disabled.status(), 303);
    assert!(
        AuthFactor::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );

    let (_, _, signed_in) = password(&router, "admin").await;
    assert!(signed_in, "the password alone signs in again");
}

/// A panel of another user type shares nothing: an account there with the same id holds no
/// factor, so the panel sends it to set one up.
#[tokio::test]
async fn a_panel_of_another_user_type_shares_no_factor() {
    let (db, ada) = db().await;
    let router = router(db.clone());
    enrol(&router, &db, &ada).await;

    let guest = Guest(ada);
    let client = TestClient::new(&router)
        .sign_in(&db, "guests", &guest)
        .await;
    let page = body_string(client.get("/guests/two-factor").await).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("off"));
}
