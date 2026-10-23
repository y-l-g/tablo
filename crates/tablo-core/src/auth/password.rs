//! The shipped credentials: the [`AdminUser`] model, Argon2id hashing, and the
//! [`PasswordAuth`] authenticator over them.

use std::sync::{Arc, LazyLock};

use jiff::Timestamp;
use tokio::sync::Semaphore;
use topcoat::context::Cx;
use uuid::Uuid;

use super::{
    Authenticator, EmailChange, EmailVerifier, PanelUser, PasswordResetter, Profile, ProfileEditor,
    ProfileUpdate, Registrar, SignUp, infrastructure_failure, profile::CURRENT_PASSWORD_FIELD,
    request_email_change,
};
use crate::FieldErrors;

/// A dummy Argon2id PHC string verified against when the account does not
/// exist, so unknown logins pay the same work as known ones (ADR-0013).
/// Generated with `Argon2::default()` parameters (`m=19456,t=2,p=1`).
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$h3oXdPBVwhcgZ1OTO/PuzQ$zLrHLgIkwhqu4ZlLTfSyB8mPuL6mAtaswv/eXJ5ADO8";

/// Shipped credential model with unique email, Argon2id PHC hash, display name,
/// and active flag; belongs to no tenant. [`PasswordAuth`] stores the email, and looks it up,
/// trimmed and lowercased, so one address holds one account whatever case it is typed in; a
/// resource that edits the email must store it the same way.
/// Its [`ModelColumns`](crate::ModelColumns) leave out the hash, so a resource over the panel's
/// admins lists them. A resource's edit form that changes an admin's email or password hash ends
/// that admin's sessions, pending sign-ins, reset links and pending email change in the same
/// transaction; a custom action that changes them does not, and calls
/// [`revoke_sessions_for_user`](super::revoke_sessions_for_user) itself; one that changes `active`
/// to `false` ends them at the admin's next request.
#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
pub struct AdminUser {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    /// Argon2id hash in PHC string format; never stores the plaintext.
    #[columns(skip)]
    pub password_hash: String,
    pub display_name: String,
    /// `false` denies login and panel access immediately.
    pub active: bool,
    /// When the account proved it reads `email`; `None` until it does, and again after the
    /// email changes. Read only by a panel with
    /// [`Auth::email_verification`](super::Auth::email_verification).
    pub email_verified_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

impl PanelUser for AdminUser {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn email(&self) -> Option<&str> {
        Some(&self.email)
    }

    fn can_access_panel(&self) -> bool {
        self.active
    }
}

/// Ends the sessions, pending sign-ins, reset links and pending email change of the
/// [`AdminUser`] a panel write changed, through `ex`, when the write changed its email or its
/// password hash, on every panel whose authenticator loads `AdminUser`s. An admin editing their
/// own account moves to a fresh session, as on the profile page. Any other model, and an edit
/// that leaves both alone, passes through.
pub(crate) async fn end_changed_credentials(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    before: &(dyn std::any::Any + Send + Sync),
    after: &(dyn std::any::Any + Send + Sync),
) -> topcoat::Result<()> {
    let (Some(before), Some(after)) = (
        before.downcast_ref::<AdminUser>(),
        after.downcast_ref::<AdminUser>(),
    ) else {
        return Ok(());
    };
    if before.email == after.email && before.password_hash == after.password_hash {
        return Ok(());
    }
    let Some(panel) = super::realm::current_realm(cx) else {
        return Ok(());
    };
    let user_type = std::any::TypeId::of::<AdminUser>();
    let panels = super::session::credential_panels(cx, user_type, panel);
    if panels.is_empty() {
        return Ok(());
    }
    let user_id = before.id.to_string();
    if let Some(signed) = super::signed(cx).filter(|signed| {
        signed.user.user_id() == user_id && panels.iter().any(|p| Arc::ptr_eq(p, &signed.realm))
    }) {
        let session = topcoat::session::start(cx).await?;
        return super::session::replace_sessions(cx, ex, signed, user_type, &session).await;
    }
    super::session::end_credentials(ex, &user_id, &panels, super::token::REVOCATION).await
}

/// Shipped default authenticator: Argon2id verification against [`AdminUser`].
///
/// An unknown email costs the same work as a wrong password, and every failure shows the same
/// message. It stores an email trimmed and lowercased and looks it up the same way, so
/// `Ada@Example.com` and `ada@example.com` are one account; a resource that edits an admin's
/// email stores it the same way. A row stored with capitals cannot sign in or reset its password:
/// list the accounts that differ only in case with `SELECT LOWER(TRIM(email)) FROM admin_users
/// GROUP BY 1 HAVING COUNT(*) > 1`, merge or delete them, then run
/// `UPDATE admin_users SET email = LOWER(TRIM(email))`. SQLite's `LOWER` folds ASCII letters
/// only, so lowercase any other capital by hand.
#[derive(Debug, Default, Clone, Copy)]
pub struct PasswordAuth;

impl Authenticator for PasswordAuth {
    type User = AdminUser;

    async fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> topcoat::Result<Option<AdminUser>> {
        let mut db = crate::db::db(cx);
        let user = AdminUser::filter(AdminUser::fields().email().eq(normalize_email(login)))
            .first()
            .exec(&mut db)
            .await
            .map_err(infrastructure_failure)?;
        let hash = user.as_ref().map(|user| user.password_hash.as_str());
        if !verify_password(password, hash).await {
            return Ok(None);
        }
        Ok(user)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<AdminUser>> {
        let Ok(id) = Uuid::parse_str(id) else {
            return Ok(None);
        };
        let mut db = crate::db::db(cx);
        let user = AdminUser::filter(AdminUser::fields().id().eq(id))
            .first()
            .exec(&mut db)
            .await
            .map_err(infrastructure_failure)?;
        // A deactivated account stops resolving, so its live sessions purge.
        Ok(user.filter(|user| user.active))
    }
}

/// Registers an [`AdminUser`] from a [`SignUp`], refusing a password
/// [`new_password_errors`](super::new_password_errors) refuses and an email another account
/// holds. Install it with [`Auth::registration`](super::Auth::registration).
///
/// Whether a new account may open the panel is a choice the constructor makes:
/// [`pending`](Self::pending) stores it inactive, for an administrator to approve by setting
/// `active`, and the sign-up lands on the login page; [`open`](Self::open) stores it active and
/// signs it in at once, so any visitor gets the panel's access.
///
/// ```rust
/// use tablo_core::{Auth, PasswordSignUp};
///
/// let auth = Auth::password().registration(PasswordSignUp::pending());
/// # let _ = auth;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PasswordSignUp {
    active: bool,
}

impl PasswordSignUp {
    /// Stores each new account inactive: it signs in once an administrator approves it.
    #[must_use]
    pub fn pending() -> Self {
        Self { active: false }
    }

    /// Stores each new account active: any visitor who signs up enters the panel. Open it only
    /// where any visitor may hold an account.
    #[must_use]
    pub fn open() -> Self {
        Self { active: true }
    }
}

impl Registrar for PasswordSignUp {
    type Input = SignUp;
    type User = AdminUser;

    async fn validate(
        &self,
        _cx: &Cx,
        sign_up: &SignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = sign_up.password_errors();
        let taken = AdminUser::filter(
            AdminUser::fields()
                .email()
                .eq(normalize_email(&sign_up.email)),
        )
        .first()
        .exec(ex)
        .await
        .map_err(infrastructure_failure)?;
        if taken.is_some() {
            errors.add("email", email_taken());
        }
        Ok(errors)
    }

    async fn register(
        &self,
        _cx: &Cx,
        sign_up: SignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<AdminUser> {
        insert_admin(
            ex,
            &sign_up.email,
            &sign_up.password,
            &sign_up.name,
            self.active,
            None,
        )
        .await
    }
}

/// Edits the signed-in [`AdminUser`]'s name, email and password through a [`Profile`]. Changing
/// the email or the password asks for the current password, and refuses a new password
/// [`new_password_errors`](super::new_password_errors) refuses and, once the current password
/// verifies, an email another account holds. A changed password is a
/// [`ProfileUpdate::Credentials`]. A new email goes through [`request_email_change`]: on a panel
/// with [`Auth::email_verification`](super::Auth::email_verification) it waits for its link,
/// elsewhere it is stored at once, as a credential change. Install it with
/// [`Auth::profile`](super::Auth::profile).
impl ProfileEditor for PasswordAuth {
    type Input = Profile;
    type User = AdminUser;

    fn fill(&self, _cx: &Cx, admin: &AdminUser) -> Profile {
        Profile {
            name: admin.display_name.clone(),
            email: admin.email.clone(),
            password: None,
            password_confirmation: String::new(),
            current_password: None,
        }
    }

    async fn validate(
        &self,
        _cx: &Cx,
        admin: &AdminUser,
        profile: &Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = profile.password_errors();
        let new_email = normalize_email(&profile.email) != admin.email;
        if new_email || profile.password.is_some() {
            match &profile.current_password {
                None => errors.add(
                    CURRENT_PASSWORD_FIELD,
                    crate::text!("tablo-profile-current-password-needed"),
                ),
                Some(current) if !verify_password(current, Some(&admin.password_hash)).await => {
                    errors.add(
                        CURRENT_PASSWORD_FIELD,
                        crate::text!("tablo-profile-current-password-wrong"),
                    );
                }
                Some(_) => {}
            }
        }
        // Only the account's owner learns whether another account holds an email: a stolen
        // session without the password must not probe addresses.
        if new_email && !errors.contains_key(CURRENT_PASSWORD_FIELD) {
            let taken = AdminUser::filter(
                AdminUser::fields()
                    .email()
                    .eq(normalize_email(&profile.email)),
            )
            .first()
            .exec(ex)
            .await
            .map_err(infrastructure_failure)?;
            if taken.is_some_and(|other| other.id != admin.id) {
                errors.add("email", email_taken());
            }
        }
        Ok(errors)
    }

    async fn save(
        &self,
        cx: &Cx,
        admin: &AdminUser,
        profile: Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        let email = normalize_email(&profile.email);
        let stored = email != admin.email
            && request_email_change(cx, &mut *ex, admin, &email).await? == EmailChange::Immediate;
        let credentials = stored || profile.password.is_some();
        let mut update = AdminUser::filter(AdminUser::fields().id().eq(admin.id))
            .update()
            .display_name(profile.name);
        if stored {
            update.set_email(email);
            update.set_email_verified_at(None);
        }
        if let Some(password) = &profile.password {
            update.set_password_hash(hash_password(password).await?);
        }
        update.exec(ex).await?;
        Ok(if credentials {
            ProfileUpdate::Credentials
        } else {
            ProfileUpdate::Details
        })
    }
}

/// Resets an [`AdminUser`]'s password, mailing the link to the email the account stores.
/// Install it with [`Auth::password_reset`](super::Auth::password_reset).
impl PasswordResetter for PasswordAuth {
    type User = AdminUser;

    async fn find_by_email(&self, cx: &Cx, email: &str) -> topcoat::Result<Option<AdminUser>> {
        let mut db = crate::db::db(cx);
        AdminUser::filter(AdminUser::fields().email().eq(normalize_email(email)))
            .first()
            .exec(&mut db)
            .await
            .map_err(infrastructure_failure)
    }

    async fn reset_password(
        &self,
        _cx: &Cx,
        user_id: &str,
        password: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let id = Uuid::parse_str(user_id).map_err(|_| topcoat::router::error::not_found())?;
        AdminUser::filter(AdminUser::fields().id().eq(id))
            .update()
            .password_hash(hash_password(password).await?)
            .exec(ex)
            .await?;
        Ok(())
    }
}

/// Verifies an [`AdminUser`]'s address, storing the one the link proved. Install it with
/// [`Auth::email_verification`](super::Auth::email_verification).
impl EmailVerifier for PasswordAuth {
    type User = AdminUser;

    fn is_verified(&self, admin: &AdminUser) -> bool {
        admin.email_verified_at.is_some()
    }

    async fn store_verified_email(
        &self,
        _cx: &Cx,
        user_id: &str,
        email: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let id = Uuid::parse_str(user_id).map_err(|_| topcoat::router::error::not_found())?;
        AdminUser::filter(AdminUser::fields().id().eq(id))
            .update()
            .email(normalize_email(email))
            .email_verified_at(Some(Timestamp::now()))
            .exec(ex)
            .await?;
        Ok(())
    }
}

/// Creates an active [`AdminUser`] that signs in with `email` and `password`, storing only the
/// password's Argon2id hash: the account a new app seeds to reach its panel. The email is stored
/// trimmed and lowercased, and verified: whoever seeds the account vouches for the address.
///
/// ```rust,no_run
/// # async fn seed(mut db: toasty::Db) -> topcoat::Result<()> {
/// tablo_core::auth::create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Hashing fails, or the insert does: an email another account holds is refused by its unique
/// index.
pub async fn create_admin(
    ex: &mut dyn toasty::Executor,
    email: &str,
    password: &str,
    display_name: &str,
) -> topcoat::Result<AdminUser> {
    insert_admin(
        ex,
        email,
        password,
        display_name,
        true,
        Some(Timestamp::now()),
    )
    .await
}

/// Inserts an [`AdminUser`], `active` or not, verified at `verified_at`.
async fn insert_admin(
    ex: &mut dyn toasty::Executor,
    email: &str,
    password: &str,
    display_name: &str,
    active: bool,
    verified_at: Option<Timestamp>,
) -> topcoat::Result<AdminUser> {
    let admin = toasty::create!(AdminUser {
        email: normalize_email(email),
        password_hash: hash_password(password).await?,
        display_name: display_name.to_string(),
        active,
        email_verified_at: verified_at,
        created_at: Timestamp::now(),
    })
    .exec(ex)
    .await?;
    Ok(admin)
}

/// `email` as [`AdminUser`] stores it: trimmed and lowercased.
fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Hashes a password with Argon2id into a PHC string for storage; never stores
/// the plaintext. Runs on Tokio's blocking pool, as [`verify_password`] does, one hash per core
/// at once.
///
/// # Errors
///
/// Argon2 refuses the password.
pub async fn hash_password(password: &str) -> topcoat::Result<String> {
    use argon2::password_hash::PasswordHasher;

    let password = password.to_string();
    off_executor(move || {
        argon2::Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(topcoat::Error::from)
    })
    .await
}

/// Verifies a password against an Argon2id PHC hash, or `None` for an unknown
/// account; unknown accounts verify against a dummy hash so response times do
/// not reveal which accounts exist.
///
/// Runs on Tokio's blocking pool: a verification takes tens of milliseconds of CPU, which on
/// an async worker would stall every request that worker serves. One verification or hash per
/// core runs at once, and the rest wait their turn.
///
/// ```rust,no_run
/// # use tablo_core::auth::verify_password;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[unique]
/// #     email: String,
/// #     password_hash: String,
/// # }
/// # async fn sign_in(mut db: toasty::Db, login: &str, password: &str)
/// # -> topcoat::Result<Option<Staff>> {
/// let staff = Staff::filter_by_email(login).first().exec(&mut db).await?;
/// if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())).await {
///     return Ok(None);
/// }
/// # Ok(staff)
/// # }
/// ```
#[must_use]
pub async fn verify_password(password: &str, hash: Option<&str>) -> bool {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};

    let known = hash.is_some();
    let password = password.to_string();
    let hash = hash.unwrap_or(DUMMY_PASSWORD_HASH).to_string();
    let verified = off_executor(move || match PasswordHash::new(&hash) {
        Ok(parsed) => argon2::Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        // No password matches a hash that does not parse: the account cannot sign in until an
        // operator resets it, so say why.
        Err(error) => {
            tracing::error!(error = %error, "a stored password hash does not parse");
            false
        }
    })
    .await;
    verified && known
}

/// How many Argon2 runs the process allows at once: one per core. Each run holds about 19 MiB,
/// so a flood of sign-ins under distinct logins, which no throttle count refuses, waits here
/// instead of filling Tokio's blocking pool and the memory with hashes.
static ARGON2_PERMITS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    Arc::new(Semaphore::new(cores))
});

/// Runs `work` on Tokio's blocking pool once one of [`ARGON2_PERMITS`] is free, re-raising its
/// panic here.
async fn off_executor<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    permitted(&ARGON2_PERMITS, work).await
}

/// Runs `work` on Tokio's blocking pool while holding one of `permits`. The work keeps its permit
/// until it ends, even when the request awaiting it is dropped.
pub(super) async fn permitted<T: Send + 'static>(
    permits: &Arc<Semaphore>,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let permit = Arc::clone(permits)
        .acquire_owned()
        .await
        .expect("the Argon2 semaphore is never closed");
    let work = move || {
        let value = work();
        drop(permit);
        value
    };
    match tokio::task::spawn_blocking(work).await {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// The refusal of an email another account holds.
fn email_taken() -> crate::Text {
    crate::text!(
        "tablo-field-taken",
        label = crate::text!("tablo-auth-email")
    )
}
