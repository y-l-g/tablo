//! The Policy, auth, tenancy chapter's snippets.

use tablo::{
    Membership, PanelUser, PasswordAuth,
    auth::{
        Authenticator, Profile, ProfileEditor, ProfileUpdate, Registrar, hash_password,
        new_password_errors, verify_password,
    },
    prelude::*,
};
use topcoat::{Result, context::Cx};

use crate::{
    actions::Publish,
    models::{Post, Staff},
};

// ANCHOR: policy-editors-only
pub fn editors_only(cx: &Cx) -> bool {
    tablo::auth::user::<Staff>(cx).is_some_and(|staff| staff.editor)
}
// ANCHOR_END: policy-editors-only

// ANCHOR: policy-action
pub fn post_policy(cx: &Cx, ability: Ability<'_, Post>) -> bool {
    match ability {
        Ability::ViewAny | Ability::View(_) => true,
        // `Publish` is the action's type, so renaming its `NAME` keeps this arm matching.
        _ if ability.is_action::<Publish, _>() => editors_only(cx),
        _ => false,
    }
}
// ANCHOR_END: policy-action

// ANCHOR: policy-models
pub fn auth_models() {
    let _ = toasty::models!(
        crate::Book,
        tablo::auth::AdminUser,
        tablo::auth::AuthSession
    );
}
// ANCHOR_END: policy-models

// ANCHOR: policy-hash-password
pub async fn seed_admin(mut db: toasty::Db) -> Result<()> {
    // Stores the password's Argon2id hash, never the plaintext.
    tablo::auth::create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;
    Ok(())
}

/// Your own user table hashes with the same function.
pub fn staff_hash() -> Result<String> {
    tablo::auth::hash_password("secret")
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

// ANCHOR: policy-sign-up-password
pub fn open_sign_up_panel() -> Panel {
    Panel::new("admin").auth(Auth::password().registration(PasswordAuth))
}
// ANCHOR_END: policy-sign-up-password

// ANCHOR: policy-staff-sign-up
#[derive(FormInput)]
pub struct StaffSignUp {
    pub name: String,
    #[form(email)]
    pub email: String,
    #[form(password)]
    pub password: String,
    #[form(password, label = "Confirm password")]
    pub password_confirmation: String,
}

impl Registrar for StaffAuth {
    type Input = StaffSignUp;
    type User = SignedStaff;

    async fn validate(
        &self,
        _cx: &Cx,
        sign_up: &StaffSignUp,
        ex: &mut dyn toasty::Executor,
    ) -> Result<FieldErrors> {
        let mut errors = new_password_errors(&sign_up.password, &sign_up.password_confirmation);
        if email_taken(ex, &sign_up.email).await? {
            errors.add("email", "Email has already been taken");
        }
        Ok(errors)
    }

    async fn register(
        &self,
        _cx: &Cx,
        sign_up: StaffSignUp,
        ex: &mut dyn toasty::Executor,
    ) -> Result<SignedStaff> {
        let hash = hash_password(&sign_up.password)?;
        let staff = create_staff(ex, &sign_up.name, &sign_up.email, &hash).await?;
        Ok(SignedStaff {
            staff,
            workspaces: Vec::new(),
        })
    }
}
// ANCHOR_END: policy-staff-sign-up

pub async fn email_taken(_ex: &mut dyn toasty::Executor, _email: &str) -> Result<bool> {
    todo!()
}

pub async fn create_staff(
    _ex: &mut dyn toasty::Executor,
    _name: &str,
    _email: &str,
    _password_hash: &str,
) -> Result<Staff> {
    todo!()
}

// ANCHOR: policy-staff-sign-up-panel
pub fn staff_sign_up_panel() -> Panel {
    Panel::new("admin").auth(Auth::custom(StaffAuth).registration(StaffAuth))
}
// ANCHOR_END: policy-staff-sign-up-panel

// ANCHOR: policy-profile-password
pub fn profile_panel() -> Panel {
    Panel::new("admin").auth(Auth::password().profile(PasswordAuth))
}
// ANCHOR_END: policy-profile-password

// ANCHOR: policy-staff-profile
impl ProfileEditor for StaffAuth {
    type Input = Profile;
    type User = SignedStaff;

    fn fill(&self, _cx: &Cx, signed: &SignedStaff) -> Profile {
        Profile {
            name: signed.staff.name.clone(),
            email: signed.staff.email.clone(),
            password: None,
            password_confirmation: String::new(),
            current_password: None,
        }
    }

    async fn validate(
        &self,
        _cx: &Cx,
        signed: &SignedStaff,
        profile: &Profile,
        _ex: &mut dyn toasty::Executor,
    ) -> Result<FieldErrors> {
        let mut errors = profile.password_errors();
        let current = profile.current_password.as_deref().unwrap_or_default();
        let credentials = profile.password.is_some() || profile.email != signed.staff.email;
        if credentials && !verify_password(current, Some(&signed.staff.password_hash)) {
            errors.add("current_password", "The current password is incorrect");
        }
        Ok(errors)
    }

    async fn save(
        &self,
        _cx: &Cx,
        signed: &SignedStaff,
        profile: Profile,
        ex: &mut dyn toasty::Executor,
    ) -> Result<ProfileUpdate> {
        let update = if profile.password.is_some() || profile.email != signed.staff.email {
            ProfileUpdate::Credentials
        } else {
            ProfileUpdate::Details
        };
        let hash = profile.password.as_deref().map(hash_password).transpose()?;
        update_staff(ex, &signed.staff, &profile.name, &profile.email, hash).await?;
        Ok(update)
    }
}
// ANCHOR_END: policy-staff-profile

pub async fn update_staff(
    _ex: &mut dyn toasty::Executor,
    _staff: &Staff,
    _name: &str,
    _email: &str,
    _password_hash: Option<String>,
) -> Result<()> {
    todo!()
}

// ANCHOR: policy-auth-disabled-body
pub fn open_panel() -> Panel {
    Panel::new("admin").auth(Auth::disabled())
}
// ANCHOR_END: policy-auth-disabled-body
