//! The profile page: the shipped `PasswordAuth` editing an `AdminUser`, the current password a
//! credential change asks for, the sessions a credential change ends, the profile throttle, and
//! the mistakes mounting refuses.

use std::time::Duration;

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, LoginThrottle, NoForm, NotificationStatus, PanelUser, PasswordAuth,
    ProfileFault, ReadOnly, Resource, ResourceDef, Table, TextColumn,
    auth::{AdminUser, AuthSession, Profile, ProfileEditor, ProfileUpdate, create_admin},
    lens,
    testing::{find_href_with, form_body, session_cookie_value},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{TestClient, body_string, field_error, flash, memory_db, mount, refusal};

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
    memory_db(toasty::models!(Note, AdminUser, AuthSession)).await
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
async fn renaming_keeps_the_session() {
    let db = db().await;
    let admin = ada(&db).await;
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let response = client
        .submit("/admin/profile", &profile(&[("name", "Ada Lovelace")]))
        .await;

    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/profile");
    assert_eq!(flash(&response).status, NotificationStatus::Success);
    assert!(session_cookie_value(&response).is_none());
    assert_eq!(reload(&db, &admin).await.display_name, "Ada Lovelace");
    assert_eq!(client.get("/admin/notes").await.status(), 200);
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
        assert!(tablo::auth::verify_password(
            "old password",
            Some(&stored.password_hash)
        ));
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
    assert!(tablo::auth::verify_password(
        "new password",
        Some(&stored.password_hash)
    ));
    assert_eq!(elsewhere.get("/admin/notes").await.status(), 307);
    assert_eq!(saving.get("/admin/notes").await.status(), 307);
    let renewed = saving.cookies(&tablo::testing::response_cookies(&response));
    assert_eq!(renewed.get("/admin/notes").await.status(), 200);
    let sessions = AuthSession::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(sessions.len(), 1);
}

/// A new email needs the current password and may not be another account's.
#[tokio::test]
async fn a_new_email_is_checked() {
    let db = db().await;
    let admin = ada(&db).await;
    create_admin(&mut db.clone(), "grace@example.com", "secret", "Grace")
        .await
        .unwrap();
    let router = router(db.clone(), Auth::password().profile(PasswordAuth));
    let client = TestClient::new(&router).sign_in(&db, "admin", &admin).await;

    let response = client
        .submit("/admin/profile", &profile(&[("email", "new@example.com")]))
        .await;
    let html = body_string(response).await;
    assert!(field_error(&html, "current_password").is_some(), "{html}");

    let response = client
        .submit(
            "/admin/profile",
            &profile(&[
                ("email", "grace@example.com"),
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
                ("email", "new@example.com"),
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

/// Edits visitors, whom the shipped authenticator never loads.
struct VisitorProfile;

impl ProfileEditor for VisitorProfile {
    type Input = Profile;
    type User = Visitor;

    fn fill(&self, _cx: &Cx, _user: &Visitor) -> Profile {
        unreachable!("mounting refuses the editor")
    }

    async fn save(
        &self,
        _cx: &Cx,
        _user: &Visitor,
        _input: Profile,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        unreachable!("mounting refuses the editor")
    }
}

/// Mounting refuses an editor of another user type, and a profile without auth.
#[tokio::test]
async fn mounting_refuses_a_profile_the_panel_cannot_serve() {
    let cases: [(Auth, ProfileFault); 2] = [
        (
            Auth::password().profile(VisitorProfile),
            ProfileFault::UserType {
                editor: std::any::type_name::<Visitor>(),
            },
        ),
        (
            Auth::disabled().profile(PasswordAuth),
            ProfileFault::AuthDisabled,
        ),
    ];
    for (auth, fault) in cases {
        let errors = refusal(mount(
            db().await,
            tablo::Panel::new("admin")
                .auth(auth)
                .resource::<NoteResource>(),
        ));
        let kinds: Vec<_> = errors.iter().map(|error| error.kind.clone()).collect();
        assert_eq!(kinds, vec![DeclarationErrorKind::Profile(fault)]);
    }
}
