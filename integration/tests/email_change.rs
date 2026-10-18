//! Email-change confirmation: a new address waits for its link, the link stores it and ends the
//! account's other sessions, and the link to the old address cancels the change.

use http::header::LOCATION;
use tablo::{
    Auth, AuthMail, EmailVerification, NoForm, Panel, PasswordAuth, ReadOnly, Resource,
    ResourceDef, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, AuthToken, create_admin},
    lens,
    testing::{SESSION_COOKIE, form_body, session_cookie_value},
};
use toasty::Db;
use topcoat::{
    mail::{Mail, MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{
    TestClient, flash,
    mail::{BASE_URL, link, mails},
    test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Invoice {
    #[key]
    #[auto]
    id: Uuid,
    number: String,
}

struct InvoiceResource;

impl Resource for InvoiceResource {
    type Model = Invoice;
    type Form = NoForm<Invoice>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Invoice.number))))
    }
}

/// A database holding Ada, verified at `ada@example.com`, with a second session on the panel.
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(Invoice, AdminUser, AuthSession, AuthToken)).await;
    let ada = create_admin(&mut db.clone(), "ada@example.com", "a password", "Ada")
        .await
        .unwrap();
    tablo::auth::mint_session(&mut db.clone(), "admin", &ada)
        .await
        .unwrap();
    (db, ada)
}

fn router(db: Db, mail: &MemoryTransport) -> Router {
    let auth = Auth::password()
        .mail(AuthMail::new(
            Mailbox::new("no-reply@example.com").unwrap(),
            BASE_URL,
        ))
        .profile(PasswordAuth)
        .email_verification(EmailVerification::new(PasswordAuth));
    Router::builder()
        .discover()
        .app_context(db)
        .mail(MailConfig::builder().transport(mail.clone()).build())
        .panel(Panel::new("admin").auth(auth).resource::<InvoiceResource>())
        .expect("panel builds")
        .build()
}

async fn reload(db: &Db, ada: &AdminUser) -> AdminUser {
    AdminUser::filter(AdminUser::fields().id().eq(ada.id))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("Ada exists")
}

async fn sessions(db: &Db) -> Vec<AuthSession> {
    AuthSession::all().exec(&mut db.clone()).await.unwrap()
}

/// Ada, signed in, asks to sign in as `ada@new.example` from now on; answers the client and the
/// mail to the new address, then the mail to the old one.
async fn ask_for_change<'r>(
    router: &'r Router,
    db: &Db,
    ada: &AdminUser,
    mail: &MemoryTransport,
) -> (TestClient<'r>, Mail, Mail) {
    let client = TestClient::new(router).sign_in(db, "admin", ada).await;
    let saved = client
        .submit(
            "/admin/profile",
            &form_body(&[
                ("name", "Ada"),
                ("email", "ada@new.example"),
                ("current_password", "a password"),
            ]),
        )
        .await;
    assert_eq!(saved.status(), 303);
    assert!(flash(&saved).description.is_some());
    let sent = mails(mail, 2).await;
    let to = |address: &str| {
        sent.iter()
            .find(|mail| mail.to()[0].address() == address)
            .cloned()
            .unwrap_or_else(|| panic!("a mail to {address}"))
    };
    (client, to("ada@new.example"), to("ada@example.com"))
}

/// The address stays until the link to the new one is opened; opening it stores the address,
/// verified, keeps the browser that opened it signed in and ends every other session.
#[tokio::test]
async fn a_new_email_waits_for_its_link_and_the_link_stores_it() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    let (client, to_new, _) = ask_for_change(&router, &db, &ada, &mail).await;
    assert_eq!(reload(&db, &ada).await.email, "ada@example.com");
    assert_eq!(sessions(&db).await.len(), 2);

    let confirm = link(&to_new);
    assert!(confirm.starts_with("/admin/email-change/confirm?token="));
    let confirmed = client.get(&confirm).await;
    assert_eq!(confirmed.headers().get(LOCATION).unwrap(), "/admin");

    let ada = reload(&db, &ada).await;
    assert_eq!(ada.email, "ada@new.example");
    assert!(ada.email_verified_at.is_some());
    let kept = sessions(&db).await;
    assert_eq!(kept.len(), 1, "only the confirming browser stays signed in");
    let renewed = session_cookie_value(&confirmed).expect("the browser gets a new session");
    let client = TestClient::new(&router).cookie(SESSION_COOKIE, &renewed);
    assert_eq!(client.get("/admin/invoices").await.status(), 200);
    assert_eq!(client.get(&confirm).await.status(), 404);
}

/// A link opened in a browser signed in as nobody stores the address and ends every session.
#[tokio::test]
async fn a_link_opened_signed_out_ends_every_session() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    let (_, to_new, _) = ask_for_change(&router, &db, &ada, &mail).await;

    let confirmed = TestClient::new(&router).get(&link(&to_new)).await;
    assert_eq!(confirmed.headers().get(LOCATION).unwrap(), "/admin/login");
    assert_eq!(reload(&db, &ada).await.email, "ada@new.example");
    assert!(sessions(&db).await.is_empty());
}

/// The link to the old address keeps it, ends every session and the pending change.
#[tokio::test]
async fn the_link_to_the_old_address_cancels_the_change_and_signs_everyone_out() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    let (client, to_new, to_old) = ask_for_change(&router, &db, &ada, &mail).await;

    let cancel = link(&to_old);
    assert!(cancel.starts_with("/admin/email-change/cancel?token="));
    let cancelled = TestClient::new(&router).get(&cancel).await;
    assert_eq!(cancelled.headers().get(LOCATION).unwrap(), "/admin/login");

    assert!(sessions(&db).await.is_empty());
    assert_eq!(client.get("/admin/invoices").await.status(), 307);
    assert_eq!(
        TestClient::new(&router).get(&link(&to_new)).await.status(),
        404
    );
    assert_eq!(reload(&db, &ada).await.email, "ada@example.com");
}
