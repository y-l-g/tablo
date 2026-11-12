//! The Policy, auth, tenancy chapter's snippets for an app's own user table signing in by password.

use tablo::{
    AuthMail, EmailVerification, Membership, PanelUser, PasswordAccount, PasswordAuth,
    PasswordReset, prelude::*,
};
use topcoat::{Result, context::Cx};

// ANCHOR: policy-password-account
#[derive(Debug, Clone, toasty::Model)]
pub struct Member {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
    #[unique]
    pub email: String, // stored lowercase
    pub password_hash: String,
    pub email_verified_at: Option<jiff::Timestamp>,
    pub active: bool,
}

pub struct SignedMember {
    pub member: Member,
    pub workspaces: Vec<Membership>,
}

impl PanelUser for SignedMember {
    fn user_id(&self) -> String {
        self.member.id.to_string() // the key, as its `Display` spells it
    }
    fn display_name(&self) -> &str {
        &self.member.name
    }
    fn email(&self) -> Option<&str> {
        Some(&self.member.email)
    }
    fn can_access_panel(&self, _panel: &tablo::PanelId) -> bool {
        true
    }
    fn tenants(&self) -> &[Membership] {
        &self.workspaces
    }
}

impl PasswordAccount for Member {
    type User = SignedMember;

    fn name() -> Lens<Self, String> {
        lens!(Member.name)
    }
    fn email() -> Lens<Self, String> {
        lens!(Member.email)
    }
    fn password_hash() -> Lens<Self, String> {
        lens!(Member.password_hash)
    }
    fn email_verified_at() -> Lens<Self, Option<jiff::Timestamp>> {
        lens!(Member.email_verified_at)
    }
    fn account(user: &SignedMember) -> &Member {
        &user.member
    }
    async fn sign_in(
        self,
        _cx: &Cx,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Option<SignedMember>> {
        if !self.active {
            return Ok(None); // its login fails and its sessions end
        }
        let workspaces = member_workspaces(ex, &self).await?;
        Ok(Some(SignedMember {
            member: self,
            workspaces,
        }))
    }
}
// ANCHOR_END: policy-password-account

pub async fn member_workspaces(
    _ex: &mut dyn toasty::Executor,
    _member: &Member,
) -> Result<Vec<Membership>> {
    todo!()
}

// ANCHOR: policy-password-account-panel
pub fn member_panel(mail: AuthMail) -> Panel {
    let members = PasswordAuth::<Member>::default();
    Panel::new("app").auth(
        Auth::custom(members)
            .profile(members)
            .mail(mail)
            .password_reset(PasswordReset::new(members))
            .email_verification(EmailVerification::new(members)),
    )
}
// ANCHOR_END: policy-password-account-panel
