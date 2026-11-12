//! The Policy, auth, tenancy chapter's snippets.

use tablo::{
    AuthMail, Authenticator, EmailVerification, IdentityLinker, Impersonation, Membership, Oidc,
    OidcProvider, PanelUser, PasswordAuth, PasswordReset, PasswordSignUp, Profile, ProfileEditor,
    ProfileUpdate, ProviderIdentity, Registrar, RouterBuilderPanelExt, TenantEditor,
    TenantRegistrar, TenantRegistration, TwoFactor,
    auth::{hash_password, new_password_errors, verify_password},
    prelude::*,
};
use topcoat::{
    Result,
    context::Cx,
    cookie::Key,
    mail::{FileTransport, Mailbox, Mailer},
    router::Router,
};

use crate::{
    actions::Publish,
    models::{Post, PostStatus, Staff},
};

// ANCHOR: policy-editors-only
pub fn editors_only(cx: &Cx) -> bool {
    tablo::auth::user::<Staff>(cx).is_some_and(|staff| staff.editor)
}
// ANCHOR_END: policy-editors-only

// ANCHOR: policy-load
/// How many posts the request's workspace holds, counted once for the request.
pub struct PostCount(pub usize);

impl tablo::Load for PostCount {
    async fn load(cx: &Cx) -> Result<Self> {
        let Some(workspace) = tablo::tenant_id(cx) else {
            return Ok(Self(0));
        };
        let mut db = tablo::db::db(cx)?;
        let posts = Post::filter(Post::fields().tenant_id().eq(workspace))
            .exec(&mut db)
            .await?;
        Ok(Self(posts.len()))
    }
}

/// Offers Create while the workspace holds fewer than ten posts.
pub fn under_quota(cx: &Cx) -> bool {
    tablo::loaded::<PostCount>(cx).is_some_and(|count| count.0 < 10)
}

pub fn quota_panel() -> Panel {
    Panel::new("admin").load::<PostCount>()
}
// ANCHOR_END: policy-load

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

// ANCHOR: policy-abilities
pub fn post_abilities() -> Abilities<Post> {
    Abilities::<Post>::new()
        .view_any(|_cx| true)
        .view(|_cx, _post| true)
        .create(editors_only)
        .update(|cx, post| editors_only(cx) && post.status != PostStatus::Published)
        .run::<Publish, _>(|cx, _post| editors_only(cx))
    // No delete: `DeleteAny` and `Delete` are refused, and so is any action added later.
}
// ANCHOR_END: policy-abilities

// ANCHOR: policy-written-record
pub fn unfeatured_only(_cx: &Cx, ability: Ability<'_, Post>) -> bool {
    // Refuses an update, a delete, a restore, a force delete or a record action's run on a
    // featured post, and allows every other ability: a create or a header action writes no
    // existing record, so a policy that refuses those matches them by name.
    ability.written_record().is_none_or(|post| !post.featured)
}
// ANCHOR_END: policy-written-record

// ANCHOR: policy-stored-value
/// Refuses a write that publishes a post unless an editor makes it. Called from
/// `PostResource::before_commit`, it sees what the write stores, which the policy does not.
pub async fn only_editors_publish(cx: &Cx, change: &tablo::Change<Post>) -> Result<()> {
    let publishes = match change {
        tablo::Change::Create(post) => matches!(post.status, PostStatus::Published),
        tablo::Change::Update { before, after } => {
            !matches!(before.status, PostStatus::Published)
                && matches!(after.status, PostStatus::Published)
        }
        _ => false,
    };
    if publishes && !editors_only(cx) {
        return Err(topcoat::router::error::forbidden().into());
    }
    Ok(())
}
// ANCHOR_END: policy-stored-value

// ANCHOR: policy-models
pub fn auth_models() {
    // `Book`, beside `AdminUser`, `AuthSession`, `ColumnPreference`, `AuthToken` and `AuthFactor`.
    let _ = tablo::models!(crate::Book);
}
// ANCHOR_END: policy-models

// ANCHOR: policy-hash-password
pub async fn seed_admin(mut db: toasty::Db) -> Result<()> {
    // Stores the password's Argon2id hash, never the plaintext.
    tablo::auth::create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;
    Ok(())
}

/// Your own user table hashes with the same function.
pub async fn staff_hash() -> Result<String> {
    tablo::auth::hash_password("secret").await
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
    fn can_access_panel(&self, _panel: &tablo::PanelId) -> bool {
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
        if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())).await {
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
    Panel::new("admin").auth(Auth::password().registration(PasswordSignUp::pending()))
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
        let hash = hash_password(&sign_up.password).await?;
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
    Panel::new("admin").auth(Auth::password().profile(PasswordAuth::new()))
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
        ex: &mut dyn toasty::Executor,
    ) -> Result<FieldErrors> {
        let mut errors = profile.password_errors();
        let current = profile.current_password.as_deref().unwrap_or_default();
        let credentials = profile.password.is_some() || profile.email != signed.staff.email;
        if credentials && !verify_password(current, Some(&signed.staff.password_hash)).await {
            errors.add("current_password", "The current password is incorrect");
        } else if profile.email != signed.staff.email && email_taken(ex, &profile.email).await? {
            // Asked only once the password verifies: a stolen session learns no addresses.
            errors.add("email", "Email has already been taken");
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
        let hash = match &profile.password {
            Some(password) => Some(hash_password(password).await?),
            None => None,
        };
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

// ANCHOR: policy-password-reset
pub fn reset_router(db: toasty::Db) -> Result<Router> {
    let auth = Auth::password()
        .mail(AuthMail::new(
            Mailbox::named("Example", "no-reply@example.com")?,
            "https://admin.example.com",
        ))
        .password_reset(PasswordReset::new(PasswordAuth::new()));
    let mut builder = Router::builder()
        .with(db)
        // Before the panel, which checks for it. `SmtpTransport`, behind Topcoat's `mail-smtp`
        // feature, delivers in production; `FileTransport` writes `.eml` files instead.
        .with(Mailer::new(FileTransport::new("target/mail")));
    builder.panel(Panel::new("admin").auth(auth))?;
    Ok(builder.build())
}
// ANCHOR_END: policy-password-reset

// ANCHOR: policy-email-verification
pub fn verified_auth() -> Result<Auth> {
    Ok(Auth::password()
        .mail(AuthMail::new(
            Mailbox::named("Example", "no-reply@example.com")?,
            "https://admin.example.com",
        ))
        .registration(PasswordSignUp::open())
        .email_verification(EmailVerification::new(PasswordAuth::new())))
}
// ANCHOR_END: policy-email-verification

// ANCHOR: policy-two-factor
pub fn two_factor_router(db: toasty::Db, cookie_key: &[u8]) -> Result<Router> {
    let auth = Auth::password().two_factor(TwoFactor::new());
    let mut builder = Router::builder()
        .with(db)
        // Seals each authenticator secret. Load the same 64 random bytes or more at every start,
        // kept with the app's other secrets: a new key opens no secret sealed under the old one.
        .with(Key::from(cookie_key));
    builder.panel(Panel::new("admin").auth(auth))?;
    Ok(builder.build())
}
// ANCHOR_END: policy-two-factor

// ANCHOR: policy-oidc
impl IdentityLinker for StaffAuth {
    type User = SignedStaff;

    async fn sign_in(
        &self,
        cx: &Cx,
        identity: &ProviderIdentity,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Option<SignedStaff>> {
        // An identity linked before signs in as its staff member.
        if let Some(staff) = staff_linked_to(ex, &identity.issuer, &identity.subject).await? {
            return Ok(Some(staff));
        }
        // A new one links to the staff member holding the address, once Google verified it.
        let (Some(email), true) = (&identity.email, identity.email_verified) else {
            return Ok(None);
        };
        let Some(staff) = find_staff_by_email(cx, email).await? else {
            return Ok(None);
        };
        link_identity(ex, &staff, &identity.issuer, &identity.subject).await?;
        // … load the memberships and return Some(SignedStaff { .. })
        todo!()
    }
}

pub fn google_router(db: toasty::Db, cookie_key: &[u8]) -> Result<Router> {
    let google = OidcProvider::google(
        std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
        std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default(),
    );
    let auth = Auth::custom(StaffAuth)
        .oidc(Oidc::new("https://admin.example.com", StaffAuth).provider(google));
    let mut builder = Router::builder()
        .with(db)
        // Seals each pending sign-in while the browser is at Google.
        .with(Key::from(cookie_key));
    builder.panel(Panel::new("admin").auth(auth))?;
    Ok(builder.build())
}

/// The staff member a row of the app's own identity table links `subject` at `issuer` to.
pub async fn staff_linked_to(
    _ex: &mut dyn toasty::Executor,
    _issuer: &str,
    _subject: &str,
) -> Result<Option<SignedStaff>> {
    todo!()
}

/// Stores a row of the app's own identity table linking `subject` at `issuer` to `staff`.
pub async fn link_identity(
    _ex: &mut dyn toasty::Executor,
    _staff: &Staff,
    _issuer: &str,
    _subject: &str,
) -> Result<()> {
    todo!()
}
// ANCHOR_END: policy-oidc

// ANCHOR: policy-impersonation
pub fn impersonating_panel() -> Panel {
    // An editor may act as a colleague who is not one, for half an hour at most.
    let rule = Impersonation::new(|_cx, actor: &SignedStaff, target: &SignedStaff| {
        actor.staff.editor && !target.staff.editor
    })
    .lifetime(std::time::Duration::from_secs(30 * 60));
    Panel::new("admin").auth(Auth::custom(StaffAuth).impersonation(rule))
}
// ANCHOR_END: policy-impersonation

// ANCHOR: policy-tenant-registration
#[derive(FormInput)]
pub struct NewWorkspace {
    pub name: String,
}

impl TenantRegistrar for StaffAuth {
    type Input = NewWorkspace;
    type User = SignedStaff;

    async fn register(
        &self,
        _cx: &Cx,
        signed: &SignedStaff,
        workspace: NewWorkspace,
        ex: &mut dyn toasty::Executor,
    ) -> Result<Membership> {
        let id = create_workspace(ex, &workspace.name).await?;
        seat_staff(ex, signed.staff.id, id).await?;
        Ok(Membership::new(id.into(), workspace.name))
    }
}

pub fn workspace_panel() -> Panel {
    Panel::new("admin").auth(
        Auth::custom(StaffAuth)
            .registration(StaffAuth)
            .tenant_registration(TenantRegistration::new(StaffAuth).required())
            .tenant_profile(StaffAuth),
    )
}
// ANCHOR_END: policy-tenant-registration

// ANCHOR: policy-tenant-profile
impl TenantEditor for StaffAuth {
    type Input = NewWorkspace;
    type User = SignedStaff;

    fn may_edit(&self, _cx: &Cx, signed: &SignedStaff, _workspace: &Membership) -> bool {
        signed.staff.editor
    }

    async fn fill(
        &self,
        _cx: &Cx,
        _signed: &SignedStaff,
        workspace: &Membership,
    ) -> Result<NewWorkspace> {
        Ok(NewWorkspace {
            name: workspace.name.clone(),
        })
    }

    async fn save(
        &self,
        _cx: &Cx,
        _signed: &SignedStaff,
        workspace: &Membership,
        renamed: NewWorkspace,
        ex: &mut dyn toasty::Executor,
    ) -> Result<()> {
        rename_workspace(ex, workspace.tenant.get(), &renamed.name).await
    }
}
// ANCHOR_END: policy-tenant-profile

pub async fn rename_workspace(
    _ex: &mut dyn toasty::Executor,
    _workspace: uuid::Uuid,
    _name: &str,
) -> Result<()> {
    todo!()
}

pub async fn create_workspace(_ex: &mut dyn toasty::Executor, _name: &str) -> Result<uuid::Uuid> {
    todo!()
}

pub async fn seat_staff(
    _ex: &mut dyn toasty::Executor,
    _staff: uuid::Uuid,
    _workspace: uuid::Uuid,
) -> Result<()> {
    todo!()
}

// ANCHOR: policy-auth-disabled-body
pub fn open_panel() -> Panel {
    Panel::new("admin").auth(Auth::disabled())
}
// ANCHOR_END: policy-auth-disabled-body
