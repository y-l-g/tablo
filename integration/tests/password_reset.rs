//! Password reset: the link the shipped `PasswordAuth` mails, the same answer for an address no
//! account uses, what a reset ends, the links it refuses, and the mistakes mounting refuses.

use std::time::Duration;

use http::header::{CACHE_CONTROL, LOCATION, REFERRER_POLICY};
use jiff::Timestamp;
use tablo::{
    Auth, DeclarationErrorKind, LoginThrottle, NoForm, NotificationStatus, Panel, PanelUser,
    PasswordAuth, PasswordReset, ReadOnly, ResetFault, Resource, ResourceDef,
    RouterBuilderPanelExt, Table, TextColumn,
    auth::{
        AdminUser, AuthSession, PasswordResetToken, PasswordResetter, create_admin, verify_password,
    },
    lens,
    testing::{find_href_with, form_body},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    mail::{Mail, MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt, TextBody},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{TestClient, body_string, field_error, flash, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(
        Note,
        AdminUser,
        AuthSession,
        PasswordResetToken
    ))
    .await
}

fn reset() -> PasswordReset {
    PasswordReset::new(
        PasswordAuth,
        Mailbox::new("no-reply@example.com").unwrap(),
        "https://admin.example.com/",
    )
}

fn panel(auth: Auth) -> Panel {
    Panel::new("admin").auth(auth).resource::<NoteResource>()
}

/// `panels` on a router that sends mail through `mail`.
fn mount_panels(
    db: Db,
    panels: impl IntoIterator<Item = Panel>,
    mail: &MemoryTransport,
) -> topcoat::Result<Router> {
    let mut builder = Router::builder()
        .discover()
        .app_context(db)
        .mail(MailConfig::builder().transport(mail.clone()).build());
    for panel in panels {
        builder = builder.panel(panel)?;
    }
    Ok(builder.build())
}

/// `panel` on a router that sends mail through `mail`.
fn mount_with_mail(db: Db, panel: Panel, mail: &MemoryTransport) -> topcoat::Result<Router> {
    mount_panels(db, [panel], mail)
}

fn router(db: Db, mail: &MemoryTransport) -> Router {
    mount_with_mail(db, panel(Auth::password().password_reset(reset())), mail)
        .expect("panel builds")
}

async fn ada(db: &Db) -> AdminUser {
    create_admin(&mut db.clone(), "ada@example.com", "old password", "Ada")
        .await
        .unwrap()
}

async fn reload(db: &Db, admin: &AdminUser) -> AdminUser {
    AdminUser::filter(AdminUser::fields().id().eq(admin.id))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the admin exists")
}

async fn tokens(db: &Db) -> Vec<PasswordResetToken> {
    PasswordResetToken::all()
        .exec(&mut db.clone())
        .await
        .unwrap()
}

async fn request_link(router: &Router, email: &str) -> http::Response<topcoat::router::Body> {
    TestClient::new(router)
        .submit("/admin/forgot-password", &form_body(&[("email", email)]))
        .await
}

/// The mail `mail` holds once it holds `count`: links are mailed after the response.
async fn mails(mail: &MemoryTransport, count: usize) -> Vec<Mail> {
    for _ in 0..500 {
        let sent = mail.sent();
        if sent.len() >= count {
            return sent;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("expected {count} mails, got {}", mail.sent().len());
}

fn text(mail: &Mail) -> &str {
    match mail.text() {
        TextBody::Text(text) => text,
        other => panic!("the reset mail is plain text, got {other:?}"),
    }
}

/// The path and query of the reset link `mail` carries.
fn link(mail: &Mail) -> String {
    let start = text(mail)
        .find("https://admin.example.com/")
        .expect("the mail carries the link");
    let url = text(mail)[start..].split_whitespace().next().unwrap();
    url.strip_prefix("https://admin.example.com")
        .unwrap()
        .to_string()
}

fn token(link: &str) -> String {
    let query = link.split_once('?').unwrap().1;
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.into_owned())
        .expect("the link carries a token")
}

async fn choose(
    router: &Router,
    token: &str,
    password: &str,
) -> http::Response<topcoat::router::Body> {
    TestClient::new(router)
        .submit(
            "/admin/reset-password",
            &form_body(&[
                ("token", token),
                ("password", password),
                ("password_confirmation", password),
            ]),
        )
        .await
}

/// A known address gets a link to the stored address; an unknown one gets none, and the two
/// answers match.
#[tokio::test]
async fn a_reset_request_answers_the_same_whether_an_account_uses_the_address() {
    let db = db().await;
    ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);

    let known = request_link(&router, " ada@example.com ").await;
    let unknown = request_link(&router, "nobody@example.com").await;

    for response in [&known, &unknown] {
        assert_eq!(response.status(), 303);
        assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/login");
        let notice = flash(response);
        assert_eq!(notice.status, NotificationStatus::Success);
    }
    let sent = mails(&mail, 1).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].to()[0].address(), "ada@example.com");
    assert!(link(&sent[0]).starts_with("/admin/reset-password?token="));
    assert_eq!(tokens(&db).await.len(), 1);
}

/// The link sets the new password, works once, and ends every session and link the account
/// holds.
#[tokio::test]
async fn a_reset_link_sets_the_password_once_and_ends_every_session() {
    let db = db().await;
    let admin = ada(&db).await;
    tablo::auth::mint_session(&mut db.clone(), "admin", &admin)
        .await
        .unwrap();
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    let link = link(&mails(&mail, 1).await[0]);

    let page = TestClient::new(&router).get(&link).await;
    assert_eq!(page.status(), 200);
    assert_eq!(page.headers().get(REFERRER_POLICY).unwrap(), "no-referrer");
    assert_eq!(page.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    let saved = choose(&router, &token(&link), "new password").await;

    assert_eq!(saved.status(), 303);
    assert_eq!(saved.headers().get(LOCATION).unwrap(), "/admin/login");
    assert_eq!(flash(&saved).status, NotificationStatus::Success);
    let admin = reload(&db, &admin).await;
    assert!(verify_password("new password", Some(&admin.password_hash)).await);
    assert!(
        AuthSession::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(tokens(&db).await.is_empty());
    let (_, login) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "new password", "")
        .await;
    assert_eq!(login.status(), 303);

    let reused = choose(&router, &token(&link), "another password").await;
    assert_eq!(reused.status(), 404);
    assert!(
        verify_password(
            "new password",
            Some(&reload(&db, &admin).await.password_hash)
        )
        .await
    );
}

/// An expired link resets nothing, and neither does an unknown one.
#[tokio::test]
async fn an_expired_or_unknown_link_resets_nothing() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    let link = link(&mails(&mail, 1).await[0]);
    PasswordResetToken::all()
        .update()
        .expires_at(Timestamp::now() - Duration::from_secs(1))
        .exec(&mut db.clone())
        .await
        .unwrap();

    assert_eq!(TestClient::new(&router).get(&link).await.status(), 404);
    let expired = choose(&router, &token(&link), "new password").await;
    let unknown = choose(&router, "not-a-token", "new password").await;

    assert_eq!(expired.status(), 404);
    assert_eq!(unknown.status(), 404);
    let admin = reload(&db, &admin).await;
    assert!(verify_password("old password", Some(&admin.password_hash)).await);
}

/// A new password the rules refuse keeps the link working.
#[tokio::test]
async fn a_refused_password_keeps_the_link() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    let token = token(&link(&mails(&mail, 1).await[0]));

    let refused = TestClient::new(&router)
        .submit(
            "/admin/reset-password",
            &form_body(&[
                ("token", token.as_str()),
                ("password", "short"),
                ("password_confirmation", "other"),
            ]),
        )
        .await;

    assert_eq!(refused.status(), 422);
    let html = body_string(refused).await;
    assert!(field_error(&html, "password").is_some(), "{html}");
    assert!(
        field_error(&html, "password_confirmation").is_some(),
        "{html}"
    );
    assert!(
        verify_password(
            "old password",
            Some(&reload(&db, &admin).await.password_hash)
        )
        .await
    );
    assert_eq!(choose(&router, &token, "new password").await.status(), 303);
}

/// A new request replaces the account's earlier link.
#[tokio::test]
async fn a_new_request_replaces_the_earlier_link() {
    let db = db().await;
    ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    mails(&mail, 1).await;
    request_link(&router, "ada@example.com").await;

    let sent = mails(&mail, 2).await;
    assert_eq!(tokens(&db).await.len(), 1);
    assert_eq!(
        choose(&router, &token(&link(&sent[0])), "new password")
            .await
            .status(),
        404
    );
    assert_eq!(
        choose(&router, &token(&link(&sent[1])), "new password")
            .await
            .status(),
        303
    );
}

/// An account that may not enter the panel gets no link.
#[tokio::test]
async fn an_inactive_account_gets_no_link() {
    let db = db().await;
    let admin = ada(&db).await;
    AdminUser::filter(AdminUser::fields().id().eq(admin.id))
        .update()
        .active(false)
        .exec(&mut db.clone())
        .await
        .unwrap();
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);

    let response = request_link(&router, "ada@example.com").await;

    assert_eq!(response.status(), 303);
    assert!(mail.sent().is_empty());
    assert!(tokens(&db).await.is_empty());
}

/// Past the address's limit, a request answers 429 and mails nothing.
#[tokio::test]
async fn reset_requests_past_the_limit_are_refused() {
    let db = db().await;
    ada(&db).await;
    let mail = MemoryTransport::new();
    let reset = reset().throttle(LoginThrottle::new(1, Duration::from_secs(60)));
    let router = mount_with_mail(db, panel(Auth::password().password_reset(reset)), &mail)
        .expect("panel builds");

    assert_eq!(request_link(&router, "ada@example.com").await.status(), 303);
    let throttled = request_link(&router, "ADA@example.com").await;

    assert_eq!(throttled.status(), 429);
    assert_eq!(mails(&mail, 1).await.len(), 1);
}

/// The login page links to the forgot-password page only when the panel offers a reset, and
/// without one the reset paths are gated like any other.
#[tokio::test]
async fn the_login_page_links_to_a_reset_only_when_offered() {
    let mail = MemoryTransport::new();
    let offered = router(db().await, &mail);
    let html = body_string(TestClient::new(&offered).get("/admin/login").await).await;
    assert!(
        find_href_with(&html, "/admin/forgot-password").is_some(),
        "{html}"
    );
    assert_eq!(
        TestClient::new(&offered)
            .get("/admin/forgot-password")
            .await
            .status(),
        200
    );

    let plain = mount_with_mail(db().await, panel(Auth::password()), &mail).expect("panel builds");
    let html = body_string(TestClient::new(&plain).get("/admin/login").await).await;
    assert!(
        find_href_with(&html, "/admin/forgot-password").is_none(),
        "{html}"
    );
    let gated = TestClient::new(&plain).get("/admin/forgot-password").await;
    assert_eq!(gated.status(), 307);
}

/// A forged reset POST answers 403 and changes nothing.
#[tokio::test]
async fn a_forged_reset_answers_403() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    let token = token(&link(&mails(&mail, 1).await[0]));

    let forged = TestClient::new(&router)
        .post_form(
            "/admin/reset-password",
            form_body(&[
                ("token", token.as_str()),
                ("password", "new password"),
                ("password_confirmation", "new password"),
            ]),
        )
        .await;

    assert_eq!(forged.status(), 403);
    assert!(
        verify_password(
            "old password",
            Some(&reload(&db, &admin).await.password_hash)
        )
        .await
    );
}

/// A link resets nothing once its account may no longer enter the panel.
#[tokio::test]
async fn a_deactivated_account_cannot_use_its_link() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    request_link(&router, "ada@example.com").await;
    let token = token(&link(&mails(&mail, 1).await[0]));
    AdminUser::filter(AdminUser::fields().id().eq(admin.id))
        .update()
        .active(false)
        .exec(&mut db.clone())
        .await
        .unwrap();

    assert_eq!(choose(&router, &token, "new password").await.status(), 404);
    assert!(
        verify_password(
            "old password",
            Some(&reload(&db, &admin).await.password_hash)
        )
        .await
    );
}

/// A password changed on the profile page ends the account's reset link.
#[tokio::test]
async fn a_changed_password_ends_the_reset_link() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = mount_with_mail(
        db.clone(),
        panel(
            Auth::password()
                .profile(PasswordAuth)
                .password_reset(reset()),
        ),
        &mail,
    )
    .expect("panel builds");
    request_link(&router, "ada@example.com").await;
    let token = token(&link(&mails(&mail, 1).await[0]));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let saved = client
        .submit(
            "/admin/profile",
            &form_body(&[
                ("name", "Ada"),
                ("email", "ada@example.com"),
                ("password", "profile password"),
                ("password_confirmation", "profile password"),
                ("current_password", "old password"),
            ]),
        )
        .await;

    assert_eq!(saved.status(), 303);
    assert!(tokens(&db).await.is_empty());
    assert_eq!(choose(&router, &token, "new password").await.status(), 404);
    assert!(
        verify_password(
            "profile password",
            Some(&reload(&db, &admin).await.password_hash)
        )
        .await
    );
}

/// A link works only on the panel that mailed it, and a reset ends the account's sessions and
/// links on every panel that signs in the same user type.
#[tokio::test]
async fn a_link_belongs_to_its_panel_and_a_reset_ends_the_others() {
    let db = db().await;
    let admin = ada(&db).await;
    let mail = MemoryTransport::new();
    let router = mount_panels(
        db.clone(),
        [
            panel(Auth::password().password_reset(reset())),
            Panel::new("staff")
                .auth(Auth::password().password_reset(reset()))
                .resource::<NoteResource>(),
        ],
        &mail,
    )
    .expect("panels build");
    for prefix in ["admin", "staff"] {
        tablo::auth::mint_session(&mut db.clone(), prefix, &admin)
            .await
            .unwrap();
    }
    TestClient::new(&router)
        .submit(
            "/staff/forgot-password",
            &form_body(&[("email", "ada@example.com")]),
        )
        .await;
    let staff = token(&link(&mails(&mail, 1).await[0]));
    request_link(&router, "ada@example.com").await;
    let admin_token = token(&link(&mails(&mail, 2).await[1]));

    let elsewhere = TestClient::new(&router)
        .submit(
            "/staff/reset-password",
            &form_body(&[
                ("token", admin_token.as_str()),
                ("password", "new password"),
                ("password_confirmation", "new password"),
            ]),
        )
        .await;
    assert_eq!(elsewhere.status(), 404);
    assert_ne!(staff, admin_token);

    assert_eq!(
        choose(&router, &admin_token, "new password").await.status(),
        303
    );
    assert!(
        AuthSession::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(tokens(&db).await.is_empty());
}

/// A user type the panel's authenticator does not load.
struct Visitor;

impl PanelUser for Visitor {
    fn user_id(&self) -> String {
        String::new()
    }

    fn display_name(&self) -> &str {
        ""
    }
}

/// Resets visitors, whom the shipped authenticator never loads.
struct VisitorReset;

impl PasswordResetter for VisitorReset {
    type User = Visitor;

    async fn find_by_email(&self, _cx: &Cx, _email: &str) -> topcoat::Result<Option<Visitor>> {
        unreachable!("mounting refuses the resetter")
    }

    fn email<'u>(&self, _user: &'u Visitor) -> &'u str {
        unreachable!("mounting refuses the resetter")
    }

    async fn reset_password(
        &self,
        _cx: &Cx,
        _user_id: &str,
        _password: &str,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        unreachable!("mounting refuses the resetter")
    }
}

/// Mounting refuses a reset it cannot serve.
#[tokio::test]
async fn mounting_refuses_a_reset_the_panel_cannot_serve() {
    let mail = MemoryTransport::new();
    let bad_url = PasswordReset::new(
        PasswordAuth,
        Mailbox::new("no-reply@example.com").unwrap(),
        "admin.example.com",
    );
    let errors = refusal(mount_with_mail(
        db().await,
        panel(Auth::password().password_reset(bad_url)),
        &mail,
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::PasswordReset(ResetFault::BaseUrl(
            "admin.example.com".to_string()
        ))]
    );

    let errors = refusal(tablo::testing::mount(
        db().await,
        panel(Auth::password().password_reset(reset())),
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::PasswordReset(ResetFault::MissingMail)]
    );

    let errors = refusal(mount_with_mail(
        db().await,
        panel(Auth::disabled().password_reset(reset())),
        &mail,
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::PasswordReset(
            ResetFault::AuthDisabled
        )]
    );

    let visitors = PasswordReset::new(
        VisitorReset,
        Mailbox::new("no-reply@example.com").unwrap(),
        "https://admin.example.com",
    );
    let errors = refusal(mount_with_mail(
        db().await,
        panel(Auth::password().password_reset(visitors)),
        &mail,
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::PasswordReset(ResetFault::UserType {
            resetter: std::any::type_name::<Visitor>(),
        })]
    );

    let without_tokens = test_db(toasty::models!(Note, AdminUser, AuthSession)).await;
    let errors = refusal(mount_with_mail(
        without_tokens,
        panel(Auth::password().password_reset(reset())),
        &mail,
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::MissingAuthModels {
            models: vec!["PasswordResetToken"]
        }]
    );
}
