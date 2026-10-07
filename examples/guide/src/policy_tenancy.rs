//! The Policy, auth, tenancy chapter's snippets.

use tablo::{
    Membership, PanelUser,
    auth::{Authenticator, verify_password},
    prelude::*,
};
use topcoat::{Result, context::Cx};

use crate::models::Staff;

// ANCHOR: policy-editors-only
pub fn editors_only(cx: &Cx) -> bool {
    tablo::auth::user::<Staff>(cx).is_some_and(|staff| staff.editor)
}
// ANCHOR_END: policy-editors-only

// ANCHOR: policy-models
pub fn auth_models() {
    let _ = toasty::models!(
        crate::Book,
        tablo_core::auth::AdminUser,
        tablo_core::auth::AuthSession
    );
}
// ANCHOR_END: policy-models

// ANCHOR: policy-hash-password
pub async fn seed_admin(mut db: toasty::Db) -> Result<()> {
    // Stores the password's Argon2id hash, never the plaintext.
    tablo_core::auth::create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;
    Ok(())
}

/// Your own user table hashes with the same function.
pub fn staff_hash() -> Result<String> {
    tablo_core::auth::hash_password("secret")
}
// ANCHOR_END: policy-hash-password

// ANCHOR: policy-staff-auth
pub struct SignedStaff {
    pub staff: Staff, // your model
    pub workspaces: Vec<Membership>,
}

impl PanelUser for SignedStaff {
    fn user_id(&self) -> String {
        self.staff.id.to_string()
    }
    fn display_name(&self) -> &str {
        &self.staff.name
    }
    fn can_access_panel(&self) -> bool {
        self.staff.active
    }
    fn tenants(&self) -> &[Membership] {
        &self.workspaces
    }
}

pub struct StaffAuth;

impl Authenticator for StaffAuth {
    type User = SignedStaff;

    async fn verify(&self, cx: &Cx, login: &str, password: &str) -> Result<Option<SignedStaff>> {
        let staff = find_staff_by_email(cx, login).await?;
        if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())) {
            return Ok(None);
        }
        // … load the memberships and return Some(SignedStaff { .. })
        todo!()
    }

    async fn find_by_id(&self, _cx: &Cx, _id: &str) -> Result<Option<SignedStaff>> {
        // … the same user and memberships, by `user_id`
        todo!()
    }
}

pub async fn find_staff_by_email(_cx: &Cx, _login: &str) -> Result<Option<Staff>> {
    todo!()
}
// ANCHOR_END: policy-staff-auth

// ANCHOR: policy-custom-auth-body
pub fn staff_auth_panel() -> Panel {
    Panel::new("admin").auth(Auth::custom(StaffAuth))
}
// ANCHOR_END: policy-custom-auth-body

// ANCHOR: policy-auth-disabled-body
pub fn open_panel() -> Panel {
    Panel::new("admin").auth(Auth::disabled())
}
// ANCHOR_END: policy-auth-disabled-body
