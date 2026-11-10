//! Provider sign-in: the button that leaves for an OpenID Connect provider, the callback that
//! comes back signed in as the account the linker names, the answers it refuses, the second
//! factor it still asks for, and the mistakes mounting refuses.

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, IdentityLinker, NotificationStatus, Oidc, OidcFault, OidcProvider,
    Panel, ProviderIdentity, RouterBuilderPanelExt, TwoFactor,
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, create_admin},
    testing::{
        FakeAccount, FakeProvider, TestClient, authenticator_code, body_string, form_body,
        notification, provider_buttons, refusal, response_cookies, session_cookie_value,
        two_factor_secret,
    },
};
use toasty::Db;
use topcoat::{
    context::Cx,
    cookie::Key,
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::test_db;

const BASE_URL: &str = "https://app.test";
const PASSWORD: &str = "a password";

/// Which admin an identity at a provider signs in as.
#[derive(Debug, Clone, toasty::Model)]
struct AdminIdentity {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    admin_id: Uuid,
    issuer: String,
    subject: String,
}

/// Signs a linked identity in as its admin, and links an unlinked one to the admin holding the
/// address the provider verified.
struct Linker;

impl IdentityLinker for Linker {
    type User = AdminUser;

    async fn sign_in(
        &self,
        _cx: &Cx,
        identity: &ProviderIdentity,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Option<AdminUser>> {
        let linked = AdminIdentity::filter(
            AdminIdentity::fields()
                .issuer()
                .eq(identity.issuer.clone())
                .and(
                    AdminIdentity::fields()
                        .subject()
                        .eq(identity.subject.clone()),
                ),
        )
        .first()
        .exec(&mut *ex)
        .await?;
        if let Some(linked) = linked {
            return Ok(AdminUser::filter_by_id(linked.admin_id)
                .first()
                .exec(ex)
                .await?);
        }
        let (Some(email), true) = (&identity.email, identity.email_verified) else {
            return Ok(None);
        };
        let Some(admin) = AdminUser::filter_by_email(email)
            .first()
            .exec(&mut *ex)
            .await?
        else {
            return Ok(None);
        };
        toasty::create!(AdminIdentity {
            admin_id: admin.id,
            issuer: identity.issuer.clone(),
            subject: identity.subject.clone(),
        })
        .exec(ex)
        .await?;
        Ok(Some(admin))
    }
}

/// A database holding Ada, whose password is [`PASSWORD`].
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(
        AdminUser,
        AuthSession,
        AuthToken,
        AuthFactor,
        AdminIdentity
    ))
    .await;
    let ada = create_admin(&mut db.clone(), "ada@example.com", PASSWORD, "Ada")
        .await
        .unwrap();
    (db, ada)
}

fn mount(db: Db, auth: Auth) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .with(db)
        .with(Key::generate())
        .panel(Panel::new("admin").auth(auth))?
        .build())
}

/// The `admin` panel signing in by password and through `fake` under the key `fake`.
fn router(db: Db, fake: &FakeProvider, two_factor: bool) -> Router {
    let oidc = Oidc::new(BASE_URL, Linker).provider(fake.provider("fake"));
    let mut auth = Auth::password().oidc(oidc);
    if two_factor {
        auth = auth.two_factor(TwoFactor::new());
    }
    mount(db, auth).expect("panel builds")
}

/// A browser's visit to `start`: the provider's authorization URL it is sent to, and the cookies
/// it holds after.
async fn leave(router: &Router, start: &str) -> (String, Vec<(String, String)>) {
    let response = TestClient::new(router).get(start).await;
    assert_eq!(response.status(), 303);
    let location = response.headers()[LOCATION].to_str().unwrap().to_string();
    (location, response_cookies(&response))
}

/// The browser lands on `callback` holding `cookies`.
async fn land(
    router: &Router,
    callback: &str,
    cookies: &[(String, String)],
) -> http::Response<topcoat::router::Body> {
    TestClient::new(router).cookies(cookies).get(callback).await
}

/// Whether `response` sends the browser back to the login page with an error and no session.
fn refused(response: &http::Response<topcoat::router::Body>) -> bool {
    let location = response
        .headers()
        .get(LOCATION)
        .map(|value| value.to_str().unwrap());
    response.status() == 303
        && location.is_some_and(|location| location.starts_with("/admin/login"))
        && session_cookie_value(response).is_none()
        && notification(response).is_some_and(|note| note.status == NotificationStatus::Error)
}

#[tokio::test]
async fn a_verified_address_signs_its_account_in_and_links_it() {
    let (db, ada) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db.clone(), &fake, false);

    let page = body_string(
        TestClient::new(&router)
            .get("/admin/login?next=/admin/x")
            .await,
    )
    .await;
    let buttons = provider_buttons(&page);
    assert_eq!(buttons.len(), 1);
    assert_eq!(buttons[0].0, "fake");

    let (location, cookies) = leave(&router, &buttons[0].1).await;
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );
    assert!(callback.starts_with("/admin/sign-in/fake/callback?"));
    let landed = land(&router, &callback, &cookies).await;
    assert_eq!(landed.status(), 303);
    assert_eq!(landed.headers()[LOCATION], "/admin/x");
    assert!(session_cookie_value(&landed).is_some());
    let sessions = AuthSession::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].user_id, ada.id.to_string());

    let links = AdminIdentity::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(
        (
            links[0].admin_id,
            links[0].issuer.as_str(),
            links[0].subject.as_str()
        ),
        (ada.id, fake.issuer(), "ada-1")
    );
}

#[tokio::test]
async fn an_unverified_address_links_no_account() {
    let (db, _) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db.clone(), &fake, false);
    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    let account = FakeAccount {
        email_verified: false,
        ..FakeAccount::verified("mallory", "ada@example.com")
    };
    let landed = land(&router, &fake.authorize(&location, &account), &cookies).await;
    assert!(refused(&landed));
    assert!(
        AdminIdentity::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}

/// The answer is bound to the browser that left: without the cookie it set, or with another
/// sign-in's, the code is never redeemed.
#[tokio::test]
async fn an_answer_needs_the_browser_that_asked_for_it() {
    let (db, _) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db, &fake, false);
    let (location, _) = leave(&router, "/admin/sign-in/fake").await;
    let (_, other_cookies) = leave(&router, "/admin/sign-in/fake").await;
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );

    assert!(refused(&land(&router, &callback, &[]).await));
    assert!(refused(&land(&router, &callback, &other_cookies).await));
    assert_eq!(fake.redeemed(), 0);
}

#[tokio::test]
async fn a_declined_sign_in_and_a_replayed_code_sign_nobody_in() {
    let (db, _) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db, &fake, false);

    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    assert!(refused(
        &land(&router, &fake.deny(&location), &cookies).await
    ));

    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );
    assert_eq!(land(&router, &callback, &cookies).await.status(), 303);
    assert_eq!(fake.redeemed(), 1);
    // The provider takes each code once, so a replay with the old cookie signs nobody in.
    assert!(refused(&land(&router, &callback, &cookies).await));
}

/// The pending cookie stays good for its lifetime, so the sign-in throttle bounds the token
/// requests one client makes the panel send by replaying it with made-up codes.
#[tokio::test]
async fn replaying_one_pending_sign_in_is_throttled() {
    let (db, _) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db, &fake, false);
    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    let state = form_urlencoded::parse(location.split_once('?').unwrap().1.as_bytes())
        .find(|(name, _)| name == "state")
        .unwrap()
        .1
        .into_owned();
    for attempt in 0..5 {
        let made_up = format!("/admin/sign-in/fake/callback?code=made-up-{attempt}&state={state}");
        assert!(refused(&land(&router, &made_up, &cookies).await));
    }
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );
    assert!(refused(&land(&router, &callback, &cookies).await));
    assert_eq!(
        fake.redeemed(),
        0,
        "the sixth answer never reached the provider"
    );
}

/// A provider's issuer is matched to the character: a discovery document naming another
/// spelling of it is not that provider's.
#[tokio::test]
async fn a_discovery_document_naming_another_issuer_signs_nobody_in() {
    let (db, _) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let slashed = OidcProvider::new(
        "fake",
        "fake",
        format!("{}/", fake.issuer()),
        "client",
        "secret",
    );
    let router = mount(
        db,
        Auth::password().oidc(Oidc::new(BASE_URL, Linker).provider(slashed)),
    )
    .expect("panel builds");
    let response = TestClient::new(&router).get("/admin/sign-in/fake").await;
    assert!(refused(&response));
}

#[tokio::test]
async fn an_account_without_panel_access_is_refused() {
    let (db, ada) = db().await;
    AdminUser::filter_by_id(ada.id)
        .update()
        .active(false)
        .exec(&mut db.clone())
        .await
        .unwrap();
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db, &fake, false);
    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );
    assert!(refused(&land(&router, &callback, &cookies).await));
}

/// A provider sign-in skips no second factor: an account holding one answers the challenge.
#[tokio::test]
async fn an_account_with_a_second_factor_answers_the_challenge() {
    let (db, ada) = db().await;
    let fake = FakeProvider::start("client", "secret").await;
    let router = router(db.clone(), &fake, true);
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
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
            &form_body(&[
                ("intent", "confirm"),
                ("code", &authenticator_code(&secret)),
            ]),
        )
        .await;
    assert_eq!(confirmed.status(), 200);

    let (location, cookies) = leave(&router, "/admin/sign-in/fake").await;
    let callback = fake.authorize(
        &location,
        &FakeAccount::verified("ada-1", "ada@example.com"),
    );
    let landed = land(&router, &callback, &cookies).await;
    assert_eq!(landed.status(), 303);
    assert!(
        landed.headers()[LOCATION]
            .to_str()
            .unwrap()
            .starts_with("/admin/two-factor-challenge")
    );
    assert!(session_cookie_value(&landed).is_none());
}

#[tokio::test]
async fn mounting_refuses_a_provider_sign_in_it_cannot_serve() {
    let (db, _) = db().await;
    let google = || OidcProvider::google("client", "secret");
    let faults = |mounted: topcoat::Result<Router>| -> Vec<OidcFault> {
        refusal(mounted)
            .into_iter()
            .filter_map(|error| match error.kind {
                DeclarationErrorKind::Oidc(fault) => Some(fault),
                _ => None,
            })
            .collect()
    };
    let without_key = Router::builder()
        .discover()
        .with(db.clone())
        .panel(
            Panel::new("admin")
                .auth(Auth::password().oidc(Oidc::new(BASE_URL, Linker).provider(google()))),
        )
        .map(|builder| builder.build());
    assert_eq!(faults(without_key), [OidcFault::MissingKey]);

    let mistakes = Oidc::new("http://app.example.com", Linker)
        .provider(google())
        .provider(google())
        .provider(OidcProvider::new(
            "Bad Key",
            "Bad",
            "https://idp.test",
            "c",
            "s",
        ))
        .provider(OidcProvider::new(
            "plain",
            "Plain",
            "http://idp.test",
            "c",
            "s",
        ))
        .provider(
            OidcProvider::new("extra", "Extra", "https://idp.test", "c", "s")
                .param("prompt", "login")
                .param("redirect_uri", "https://elsewhere.test"),
        );
    assert_eq!(
        faults(mount(db.clone(), Auth::password().oidc(mistakes))),
        [
            OidcFault::BaseUrl("http://app.example.com".to_string()),
            OidcFault::DuplicateProvider("google".to_string()),
            OidcFault::ProviderKey("Bad Key".to_string()),
            OidcFault::Issuer("plain".to_string()),
            OidcFault::ReservedParam("extra".to_string(), "redirect_uri".to_string()),
        ]
    );
    assert_eq!(
        faults(mount(
            db,
            Auth::password().oidc(Oidc::new(BASE_URL, Linker))
        )),
        [OidcFault::NoProvider]
    );
}
