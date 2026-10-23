//! A panel write that changes an admin's email or password ends that admin's sessions in the
//! same transaction; an edit that leaves both alone keeps them.

use tablo::{
    Auth, Resource, ResourceDef, Table, TextColumn,
    auth::{AdminUser, AuthSession, create_admin, mint_session},
    lens,
};
use toasty::Db;

use crate::common::{TestClient, form_body, router_with, test_db};

#[derive(tablo::RecordForm)]
#[form(model = AdminUser)]
struct AdminForm {
    email: String,
    display_name: String,
}

struct AdminResource;

impl Resource for AdminResource {
    type Model = AdminUser;
    type Form = AdminForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("admins")
            .edit_only()
            .policy_fn(|_cx, _ability| true)
            .table(Table::new(TextColumn::new(lens!(AdminUser.email))))
    }
}

/// Ada edits Bob through the panel, Bob holding a session of his own; answers the sessions left.
async fn edit_bob(email: &str, name: &str) -> Vec<AuthSession> {
    let mut db: Db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let ada = create_admin(&mut db, "ada@example.com", "opensesame", "Ada")
        .await
        .expect("seed ada");
    let bob = create_admin(&mut db, "bob@example.com", "opensesame", "Bob")
        .await
        .expect("seed bob");
    mint_session(&mut db.clone(), "/admin", &bob)
        .await
        .expect("bob's session");
    let router = router_with::<AdminResource>(db.clone(), Auth::password());
    let client = TestClient::new(&router).sign_in(&db, "/admin", &ada).await;
    let response = client
        .submit(
            &format!("/admin/admins/{}/edit", bob.id),
            &form_body(&[("email", email), ("display_name", name)]),
        )
        .await;
    assert_eq!(response.status(), 303, "the edit saves");
    let sessions = AuthSession::all().exec(&mut db).await.expect("sessions");
    assert!(
        sessions
            .iter()
            .any(|session| session.user_id == ada.id.to_string()),
        "the editor keeps her session"
    );
    sessions
        .into_iter()
        .filter(|session| session.user_id == bob.id.to_string())
        .collect()
}

#[tokio::test]
async fn a_changed_email_ends_the_admins_sessions() {
    assert!(edit_bob("robert@example.com", "Bob").await.is_empty());
}

#[tokio::test]
async fn a_renamed_admin_keeps_their_sessions() {
    assert_eq!(edit_bob("bob@example.com", "Robert").await.len(), 1);
}
