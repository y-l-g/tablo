//! The profile page: the shipped `PasswordAuth` editing an `AdminUser`, the current password a
//! credential change asks for, the sessions a credential change ends, the profile throttle, and
//! the mistakes mounting refuses.

use std::time::Duration;

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, Field, FieldError, FormInput, FormValues, LoginThrottle, NoForm,
    NotificationStatus, PasswordAuth, ProfileEditor, ProfileFault, ProfileUpdate, ReadOnly,
    Resource, ResourceDef, RouterBuilderPanelExt, Schema, Table, TextColumn,
    auth::{AdminUser, AuthSession, create_admin},
    lens, parse_scalar,
    testing::{find_href_with, form_body, session_cookie_value},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{TestClient, body_string, field_error, flash, mount, refusal, test_db};

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
    test_db(toasty::models!(Note, AdminUser, AuthSession)).await
}

fn router(db: Db, auth: Auth) -> Router {
    mount(
        db,
        tablo::Panel::new("admin")
            .auth(auth)
            .resource::<NoteResource>(),
    )
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

fn profile(pairs: &[(&str, &str)]) -> String {
    let mut fields = vec![("name", "Ada"), ("email", "ada@example.com")];
    for (key, value) in pairs {
        fields.retain(|(kept, _)| kept != key);
        fields.push((key, value));
    }
    form_body(&fields)
}

/// The user menu links the profile page, which shows the account's name and email and none of
/// its password.
#[tokio::test]
async fn the_profile_page_shows_the_signed_in_account() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let home = body_string(client.get("/admin/notes").await).await;
    assert!(find_href_with(&home, "/admin/profile").is_some(), "{home}");

    let page = client.get("/admin/profile").await;
    assert_eq!(page.status(), 200);
    let html = body_string(page).await;
    assert_eq!(
        tablo::testing::input_value(&html, "name").as_deref(),
        Some("Ada")
    );
    assert_eq!(
        tablo::testing::input_value(&html, "email").as_deref(),
        Some("ada@example.com")
    );
    assert_eq!(
        tablo::testing::input_value(&html, "password").unwrap_or_default(),
        ""
    );
}

/// A new name saves without the current password, and every session stays signed in.
#[tokio::test]
async fn renaming_keeps_the_sessions() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;
    let elsewhere = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let response = client
        .submit("/admin/profile", &profile(&[("name", "Ada Lovelace")]))
        .await;

    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/profile");
    assert_eq!(flash(&response).status, NotificationStatus::Success);
    assert!(session_cookie_value(&response).is_none());
    assert_eq!(reload(&db, &admin).await.display_name, "Ada Lovelace");
    assert_eq!(client.get("/admin/notes").await.status(), 200);
    assert_eq!(elsewhere.get("/admin/notes").await.status(), 200);
}

/// A new password needs the current one: without it, or with a wrong one, nothing is written.
#[tokio::test]
async fn a_new_password_needs_the_current_one() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;
    let new = [
        ("password", "new password"),
        ("password_confirmation", "new password"),
    ];

    for current in ["", "wrong password"] {
        let mut pairs = new.to_vec();
        pairs.push(("current_password", current));
        let response = client.submit("/admin/profile", &profile(&pairs)).await;
        let html = body_string(response).await;
        assert!(field_error(&html, "current_password").is_some(), "{html}");
        let stored = reload(&db, &admin).await;
        assert!(tablo::auth::verify_password("old password", Some(&stored.password_hash)).await);
    }
}

/// A changed password ends every session the old one opened, the saving browser's included, and
/// signs that browser in again with a new session.
#[tokio::test]
async fn a_changed_password_ends_the_other_sessions() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let saving = TestClient::new(&router).sign_in(&db, "admin", &admin).await;
    let elsewhere = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let response = saving
        .submit(
            "/admin/profile",
            &profile(&[
                ("password", "new password"),
                ("password_confirmation", "new password"),
                ("current_password", "old password"),
            ]),
        )
        .await;

    assert_eq!(response.status(), 303);
    let stored = reload(&db, &admin).await;
    assert!(tablo::auth::verify_password("new password", Some(&stored.password_hash)).await);
    assert_eq!(elsewhere.get("/admin/notes").await.status(), 307);
    assert_eq!(saving.get("/admin/notes").await.status(), 307);
    let renewed = saving.cookies(&tablo::testing::response_cookies(&response));
    assert_eq!(renewed.get("/admin/notes").await.status(), 200);
    let sessions = AuthSession::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(sessions.len(), 1);
}

/// A new email needs the current password, may not be another account's in any case, and is
/// stored lowercased.
#[tokio::test]
async fn a_new_email_is_checked() {
    let db = db().await;
    let admin = ada(&db).await;
    create_admin(&mut db.clone(), "grace@example.com", "secret", "Grace")
        .await
        .unwrap();
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    // Without the current password, a taken address reads like a free one.
    for email in ["new@example.com", "grace@example.com"] {
        let response = client
            .submit("/admin/profile", &profile(&[("email", email)]))
            .await;
        let html = body_string(response).await;
        assert!(field_error(&html, "current_password").is_some(), "{html}");
        assert!(field_error(&html, "email").is_none(), "{html}");
    }

    let response = client
        .submit(
            "/admin/profile",
            &profile(&[
                ("email", "GRACE@example.com"),
                ("current_password", "old password"),
            ]),
        )
        .await;
    let html = body_string(response).await;
    assert!(field_error(&html, "email").is_some(), "{html}");
    assert_eq!(reload(&db, &admin).await.email, "ada@example.com");

    let response = client
        .submit(
            "/admin/profile",
            &profile(&[
                ("email", "New@Example.com"),
                ("current_password", "old password"),
            ]),
        )
        .await;
    assert_eq!(response.status(), 303);
    assert_eq!(reload(&db, &admin).await.email, "new@example.com");
}

/// Past the profile limit a user's save is refused with 429 before the editor runs.
#[tokio::test]
async fn the_profile_throttle_refuses_a_user_past_its_limit() {
    let db = db().await;
    let admin = ada(&db).await;
    let auth = Auth::password()
        .profile(PasswordAuth)
        .profile_throttle(LoginThrottle::new(1, Duration::from_secs(60)));
    let router = router(db.clone(), auth);
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let first = client
        .submit("/admin/profile", &profile(&[("name", "One")]))
        .await;
    assert_eq!(first.status(), 303);
    let refused = client
        .submit("/admin/profile", &profile(&[("name", "Two")]))
        .await;

    assert_eq!(refused.status(), 429);
    assert_eq!(reload(&db, &admin).await.display_name, "One");
}

/// Signed out, the page sends the visitor to sign in and a save is refused.
#[tokio::test]
async fn the_profile_page_needs_a_session() {
    let db = db().await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router);

    assert_eq!(client.get("/admin/profile").await.status(), 307);
    let response = client.submit("/admin/profile", &profile(&[])).await;
    assert_eq!(response.status(), 401);
}

/// Without `Auth::profile`, the panel serves no profile page and the user menu links to none.
#[tokio::test]
async fn a_panel_without_a_profile_serves_none() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password());
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let home = body_string(client.get("/admin/notes").await).await;
    assert!(find_href_with(&home, "/admin/profile").is_none(), "{home}");
    assert_eq!(client.get("/admin/profile").await.status(), 404);
}

/// Mounting refuses a profile without auth.
#[tokio::test]
async fn mounting_refuses_a_profile_the_panel_cannot_serve() {
    let errors = refusal(mount(
        db().await,
        tablo::Panel::new("admin")
            .auth(Auth::disabled().profile(PasswordAuth))
            .resource::<NoteResource>(),
    ));
    let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::Profile(ProfileFault::AuthDisabled)]
    );
}

/// One back office and one reader portal, both signing in `AdminUser`s.
fn two_panels(db: Db) -> Router {
    Router::builder()
        .discover()
        .with(db)
        .panel(
            tablo::Panel::new("admin")
                .auth(Auth::password().profile(PasswordAuth))
                .resource::<NoteResource>(),
        )
        .expect("the admin panel mounts")
        .panel(
            tablo::Panel::new("portal")
                .auth(Auth::password())
                .resource::<NoteResource>(),
        )
        .expect("the portal mounts")
        .build()
}

/// A password changed on one panel ends the sessions it opened on another panel that signs the
/// same users in.
#[tokio::test]
async fn a_changed_password_ends_the_sessions_on_every_panel_of_the_user() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = two_panels(db.clone());
    let saving = TestClient::new(&router).sign_in(&db, "admin", &admin).await;
    let portal = TestClient::new(&router)
        .sign_in(&db, "portal", &admin)
        .await;
    assert_eq!(portal.get("/portal/notes").await.status(), 200);

    let response = saving
        .submit(
            "/admin/profile",
            &profile(&[
                ("password", "new password"),
                ("password_confirmation", "new password"),
                ("current_password", "old password"),
            ]),
        )
        .await;

    assert_eq!(response.status(), 303);
    assert_eq!(portal.get("/portal/notes").await.status(), 307);
}

/// A name the user edits, and an email the page shows but no save changes.
struct LockedEmail {
    name: String,
    email: String,
}

impl FormInput for LockedEmail {
    fn schema() -> Schema {
        Schema::new((
            Field::text_input::<String>("name"),
            Field::text_input::<String>("email").disabled_on_edit(),
        ))
    }

    fn parse(_cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>> {
        let name = parse_scalar::<String>("name", values, None);
        let email = parse_scalar::<String>("email", values, None);
        match (name, email) {
            (Ok(name), Ok(email)) => Ok(Self { name, email }),
            (name, email) => Err(name.err().into_iter().chain(email.err()).collect()),
        }
    }

    fn write(&self, out: &mut FormValues) {
        out.insert("name".to_string(), self.name.clone());
        out.insert("email".to_string(), self.email.clone());
    }
}

/// Saves whatever the input carries, trusting the page to keep the email.
struct LockedEmailProfile;

impl ProfileEditor for LockedEmailProfile {
    type Input = LockedEmail;
    type User = AdminUser;

    fn fill(&self, _cx: &Cx, admin: &AdminUser) -> LockedEmail {
        LockedEmail {
            name: admin.display_name.clone(),
            email: admin.email.clone(),
        }
    }

    async fn save(
        &self,
        _cx: &Cx,
        admin: &AdminUser,
        input: LockedEmail,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        AdminUser::filter(AdminUser::fields().id().eq(admin.id))
            .update()
            .display_name(input.name)
            .email(input.email)
            .exec(ex)
            .await?;
        Ok(ProfileUpdate::Details)
    }
}

/// A control the page renders disabled keeps the account's value, whatever a crafted POST says.
#[tokio::test]
async fn a_disabled_control_keeps_the_stored_value() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(LockedEmailProfile));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let response = client
        .submit(
            "/admin/profile",
            &form_body(&[("name", "Ada Lovelace"), ("email", "mallory@example.com")]),
        )
        .await;

    assert_eq!(response.status(), 303);
    let stored = reload(&db, &admin).await;
    assert_eq!(stored.display_name, "Ada Lovelace");
    assert_eq!(stored.email, "ada@example.com");
}

/// A name, and the instant a timestamp control edits to the minute.
struct Shift {
    name: String,
    starts_at: jiff::Timestamp,
}

/// The instant [`ShiftProfile`] fills, with seconds its control does not show.
const STARTS_AT: &str = "2026-03-04T05:06:07.5Z";

impl FormInput for Shift {
    fn schema() -> Schema {
        Schema::new((
            Field::text_input::<String>("name"),
            Field::text_input::<jiff::Timestamp>("starts_at"),
        ))
    }

    fn parse(_cx: &Cx, values: &FormValues) -> Result<Self, Vec<FieldError>> {
        let name = parse_scalar::<String>("name", values, None);
        let starts_at = parse_scalar::<jiff::Timestamp>("starts_at", values, None);
        match (name, starts_at) {
            (Ok(name), Ok(starts_at)) => Ok(Self { name, starts_at }),
            (name, starts_at) => Err(name.err().into_iter().chain(starts_at.err()).collect()),
        }
    }

    fn write(&self, out: &mut FormValues) {
        out.insert("name".to_string(), self.name.clone());
        out.insert("starts_at".to_string(), self.starts_at.to_string());
    }
}

/// Fills a fixed instant and records the one each save receives.
struct ShiftProfile(std::sync::Arc<std::sync::Mutex<Option<jiff::Timestamp>>>);

impl ProfileEditor for ShiftProfile {
    type Input = Shift;
    type User = AdminUser;

    fn fill(&self, _cx: &Cx, admin: &AdminUser) -> Shift {
        Shift {
            name: admin.display_name.clone(),
            starts_at: STARTS_AT.parse().unwrap(),
        }
    }

    async fn save(
        &self,
        _cx: &Cx,
        _admin: &AdminUser,
        input: Shift,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        *self.0.lock().unwrap() = Some(input.starts_at);
        Ok(ProfileUpdate::Details)
    }
}

/// A timestamp the user leaves as its control shows it keeps its seconds; a changed minute is
/// saved as posted.
#[tokio::test]
async fn an_untouched_timestamp_keeps_its_seconds() {
    let db = db().await;
    let admin = ada(&db).await;
    let saved = std::sync::Arc::default();
    let router = router(
        db.clone(),
        Auth::password().profile(ShiftProfile(std::sync::Arc::clone(&saved))),
    );
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    for (posted, expected) in [
        ("2026-03-04T05:06", STARTS_AT),
        ("2026-03-04T05:07", "2026-03-04T05:07:00Z"),
    ] {
        let response = client
            .submit(
                "/admin/profile",
                &form_body(&[("name", "Ada"), ("starts_at", posted)]),
            )
            .await;
        assert_eq!(response.status(), 303, "{posted}");
        assert_eq!(
            *saved.lock().unwrap(),
            Some(expected.parse::<jiff::Timestamp>().unwrap()),
            "{posted}"
        );
    }
}
