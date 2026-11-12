//! `PasswordAuth` over an app's own model: a numeric key, a signed-in user that wraps the row, an
//! `#[update]` stamp, the hook refusing a profile change, a reset, a verification, an edit form
//! that ends the sessions it changes the credentials of, and the declaration error of a user id
//! that spells another key.

use jiff::Timestamp;
use tablo::{
    Auth, AuthMail, EmailVerification, FieldErrors, Lens, Membership, NoForm, PanelUser,
    PasswordAccount, PasswordAuth, PasswordReset, Profile, ProfileField, ReadOnly, Resource,
    ResourceDef, ResourcePage, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AuthSession, AuthToken, hash_password, normalize_email, verify_password},
    lens,
    testing::{form_body, response_cookies},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    mail::{Mailbox, Mailer, MemoryTransport},
    router::{Router, RouterBuilderDiscoverExt},
};

use crate::common::{
    TestClient, body_string, field_error,
    mail::{BASE_URL, link, mails, token},
    test_db,
};

/// An account of the app's own table: a numeric key, an `#[update]` stamp, and an id the signed-in
/// user spells as `id_prefix` says.
#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Customer {
    #[key]
    #[auto]
    id: u64,
    name: String,
    #[unique]
    email: String,
    password: String,
    email_verified_at: Option<Timestamp>,
    /// Signs in nowhere while `true`.
    banned: bool,
    /// Bars every change to the email on the profile page.
    locked: bool,
    #[update(Timestamp::now())]
    updated_at: Timestamp,
}

/// The signed-in customer, wrapped with what the app loads beside the row.
#[derive(Debug)]
struct SignedCustomer {
    customer: Customer,
    tenants: Vec<Membership>,
}

impl PanelUser for SignedCustomer {
    fn user_id(&self) -> String {
        // A name holding `Mallory` spells another key: the declaration error's test.
        if self.customer.name == "Mallory" {
            return format!("customer-{}", self.customer.id);
        }
        self.customer.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.customer.name
    }

    fn email(&self) -> Option<&str> {
        Some(&self.customer.email)
    }

    fn can_access_panel(&self, _panel: &str) -> bool {
        true
    }

    fn tenants(&self) -> &[Membership] {
        &self.tenants
    }
}

impl PasswordAccount for Customer {
    type User = SignedCustomer;

    fn name() -> Lens<Self, String> {
        lens!(Customer.name)
    }

    fn email() -> Lens<Self, String> {
        lens!(Customer.email)
    }

    fn password_hash() -> Lens<Self, String> {
        lens!(Customer.password)
    }

    fn email_verified_at() -> Lens<Self, Option<Timestamp>> {
        lens!(Customer.email_verified_at)
    }

    fn account(user: &SignedCustomer) -> &Customer {
        &user.customer
    }

    async fn sign_in(
        self,
        _cx: &Cx,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Option<SignedCustomer>> {
        if self.banned {
            return Ok(None);
        }
        Ok(Some(SignedCustomer {
            customer: self,
            tenants: Vec::new(),
        }))
    }

    fn profile_errors(user: &SignedCustomer, profile: &Profile) -> FieldErrors<ProfileField> {
        let mut errors = FieldErrors::new();
        if user.customer.locked && normalize_email(&profile.email) != user.customer.email {
            errors.add(ProfileField::Email, "locked");
        }
        errors
    }
}

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Note {
    #[key]
    #[auto]
    id: u64,
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

/// The back office's form over customers, which edits their sign-in details.
#[derive(tablo::RecordForm)]
#[form(model = Customer)]
struct CustomerForm {
    email: String,
    password: String,
}

struct CustomerResource;

impl Resource for CustomerResource {
    type Model = Customer;
    type Form = CustomerForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .without_page(ResourcePage::Create)
            .policy_fn(|_cx, _ability| true)
            .table(Table::new(TextColumn::new(lens!(Customer.email))))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Customer, Note, AuthSession, AuthToken)).await
}

fn router(db: Db) -> Router {
    router_with_mail(db, &MemoryTransport::new())
}

/// The customers' panel, with its profile page, password reset and email verification, mailing
/// through `mail`.
fn router_with_mail(db: Db, mail: &MemoryTransport) -> Router {
    let customers = PasswordAuth::<Customer>::default();
    let auth = Auth::custom(customers)
        .profile(customers)
        .mail(AuthMail::new(
            Mailbox::new("no-reply@example.com").unwrap(),
            BASE_URL,
        ))
        .password_reset(PasswordReset::new(customers))
        .email_verification(EmailVerification::new(customers));
    Router::builder()
        .discover()
        .with(db)
        .with(Mailer::new(mail.clone()))
        .panel(
            tablo::Panel::new("admin")
                .auth(auth)
                .resource::<NoteResource>()
                .resource::<CustomerResource>(),
        )
        .expect("panel builds")
        .build()
}

/// A customer stored as a sign-up would store them: the email lowercased, the password hashed.
async fn customer(db: &Db, name: &str, email: &str) -> Customer {
    toasty::create!(Customer {
        name: name.to_string(),
        email: email.to_string(),
        password: hash_password("old password").await.unwrap(),
        email_verified_at: Some(Timestamp::now()),
        banned: false,
        locked: false,
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn reload(db: &Db, customer: &Customer) -> Customer {
    Customer::filter_by_id(customer.id)
        .get(&mut db.clone())
        .await
        .unwrap()
}

/// A login typed in capitals signs in, and each later request loads the wrapped user back by its
/// numeric key.
#[tokio::test]
async fn a_customer_signs_in_by_email_and_stays_signed_in() {
    let db = db().await;
    customer(&db, "Ada", "ada@example.com").await;
    let router = router(db.clone());

    let (client, response) = TestClient::new(&router)
        .log_in("admin", " Ada@Example.COM ", "old password", "")
        .await;

    assert_eq!(response.status(), 303);
    assert_eq!(client.get("/admin/notes").await.status(), 200);
}

/// An account `sign_in` refuses cannot log in, and its open session ends at its next request.
#[tokio::test]
async fn a_refused_account_neither_logs_in_nor_keeps_its_session() {
    let db = db().await;
    let ada = customer(&db, "Ada", "ada@example.com").await;
    let router = router(db.clone());
    let (client, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;
    Customer::filter_by_id(ada.id)
        .update()
        .banned(true)
        .exec(&mut db.clone())
        .await
        .unwrap();

    assert_eq!(client.get("/admin/notes").await.status(), 307);
    assert!(sessions(&db).await.is_empty());
    let (_, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;
    assert_eq!(response.status(), 403);
    assert!(sessions(&db).await.is_empty());
}

async fn sessions(db: &Db) -> Vec<AuthSession> {
    AuthSession::all().exec(&mut db.clone()).await.unwrap()
}

/// A new password writes the app's own hash column and its `#[update]` stamp, and ends the other
/// sessions.
#[tokio::test]
async fn a_new_password_writes_the_apps_columns() {
    let db = db().await;
    let ada = customer(&db, "Ada", "ada@example.com").await;
    let router = router(db.clone());
    let (saving, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;
    let (elsewhere, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;

    let response = saving
        .submit(
            "/admin/profile",
            &form_body(&[
                ("name", "Ada Lovelace"),
                ("email", "ada@example.com"),
                ("password", "new password"),
                ("password_confirmation", "new password"),
                ("current_password", "old password"),
            ]),
        )
        .await;

    assert_eq!(response.status(), 303);
    let stored = reload(&db, &ada).await;
    assert_eq!(stored.name, "Ada Lovelace");
    assert!(verify_password("new password", Some(&stored.password)).await);
    assert!(stored.updated_at > ada.updated_at);
    assert_eq!(elsewhere.get("/admin/notes").await.status(), 307);
    let renewed = saving.cookies(&response_cookies(&response));
    assert_eq!(renewed.get("/admin/notes").await.status(), 200);
}

/// A field the account's hook refuses renders its error, and nothing is written.
#[tokio::test]
async fn the_hook_refuses_a_profile_change() {
    let db = db().await;
    let ada = customer(&db, "Ada", "ada@example.com").await;
    Customer::filter_by_id(ada.id)
        .update()
        .locked(true)
        .exec(&mut db.clone())
        .await
        .unwrap();
    let router = router(db.clone());
    let (client, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;

    let response = client
        .submit(
            "/admin/profile",
            &form_body(&[
                ("name", "Ada"),
                ("email", "ada@elsewhere.example"),
                ("current_password", "old password"),
            ]),
        )
        .await;

    let html = body_string(response).await;
    assert!(field_error(&html, "email").is_some(), "{html}");
    assert_eq!(reload(&db, &ada).await.email, "ada@example.com");
}

/// A signed-in user whose id spells another key than its row's is a declaration error, and no
/// session is written for it.
#[tokio::test]
async fn a_user_id_spelling_another_key_is_refused() {
    let db = db().await;
    customer(&db, "Mallory", "mallory@example.com").await;
    let router = router(db.clone());

    let (_, response) = TestClient::new(&router)
        .log_in("admin", "mallory@example.com", "old password", "")
        .await;

    assert_eq!(response.status(), 500);
    assert!(sessions(&db).await.is_empty());
}

/// A reset link writes the app's hash column and its stamp, and ends the account's sessions.
#[tokio::test]
async fn a_reset_link_sets_the_customers_password() {
    let db = db().await;
    let ada = customer(&db, "Ada", "ada@example.com").await;
    let mail = MemoryTransport::new();
    let router = router_with_mail(db.clone(), &mail);
    let (client, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;

    TestClient::new(&router)
        .submit(
            "/admin/forgot-password",
            &form_body(&[("email", "ADA@example.com")]),
        )
        .await;
    let link = link(&mails(&mail, 1).await[0]);
    let saved = TestClient::new(&router)
        .submit(
            "/admin/reset-password",
            &form_body(&[
                ("token", &token(&link)),
                ("password", "new password"),
                ("password_confirmation", "new password"),
            ]),
        )
        .await;

    assert_eq!(saved.status(), 303);
    let stored = reload(&db, &ada).await;
    assert!(verify_password("new password", Some(&stored.password)).await);
    assert!(stored.updated_at > ada.updated_at);
    assert_eq!(client.get("/admin/notes").await.status(), 307);
}

/// A verification link marks the app's own column, and stamps the row.
#[tokio::test]
async fn a_verification_link_marks_the_customer_verified() {
    let db = db().await;
    let ada = customer(&db, "Ada", "ada@example.com").await;
    Customer::filter_by_id(ada.id)
        .update()
        .email_verified_at(None)
        .exec(&mut db.clone())
        .await
        .unwrap();
    let ada = reload(&db, &ada).await;
    let mail = MemoryTransport::new();
    let router = router_with_mail(db.clone(), &mail);
    let (client, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;
    assert_eq!(client.get("/admin/notes").await.status(), 307);

    client.submit("/admin/verify-email", "").await;
    let link = link(&mails(&mail, 1).await[0]);
    TestClient::new(&router).get(&link).await;

    let stored = reload(&db, &ada).await;
    assert!(stored.email_verified_at.is_some());
    assert!(stored.updated_at > ada.updated_at);
    assert_eq!(client.get("/admin/notes").await.status(), 200);
}

/// The back office's edit form that changes a customer's password hash ends that customer's
/// sessions, and the editor keeps theirs; one that changes nothing of the kind keeps them.
#[tokio::test]
async fn an_edit_form_changing_a_customers_hash_ends_their_sessions() {
    let db = db().await;
    customer(&db, "Ada", "ada@example.com").await;
    let bob = customer(&db, "Bob", "bob@example.com").await;
    let router = router(db.clone());
    let (ada_client, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "old password", "")
        .await;
    let (bob_client, _) = TestClient::new(&router)
        .log_in("admin", "bob@example.com", "old password", "")
        .await;
    let edit = |password: String| {
        let ada_client = &ada_client;
        async move {
            ada_client
                .submit(
                    &format!("/admin/customers/{}/edit", bob.id),
                    &form_body(&[("email", "bob@example.com"), ("password", &password)]),
                )
                .await
        }
    };

    assert_eq!(edit(bob.password.clone()).await.status(), 303);
    assert_eq!(bob_client.get("/admin/notes").await.status(), 200);

    let rehashed = hash_password("new password").await.unwrap();
    assert_eq!(edit(rehashed).await.status(), 303);
    assert_eq!(bob_client.get("/admin/notes").await.status(), 307);
    assert_eq!(ada_client.get("/admin/notes").await.status(), 200);
}
