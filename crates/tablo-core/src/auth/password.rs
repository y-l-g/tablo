//! The shipped credentials: the [`AdminUser`] model, its sign-up, and Argon2id hashing.

use std::sync::{Arc, LazyLock};

use jiff::Timestamp;
use tokio::sync::Semaphore;
use topcoat::context::Cx;
use uuid::Uuid;

use super::{
    PanelUser, PasswordAccount, Registrar, SignUp,
    account::{email_taken, normalize_email},
    infrastructure_failure,
};
use crate::{FieldErrors, Lens};

/// A dummy Argon2id PHC string verified against when the account does not
/// exist, so unknown logins pay the same work as known ones (ADR-0013).
/// Generated with `Argon2::default()` parameters (`m=19456,t=2,p=1`).
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$h3oXdPBVwhcgZ1OTO/PuzQ$zLrHLgIkwhqu4ZlLTfSyB8mPuL6mAtaswv/eXJ5ADO8";

/// Shipped credential model with unique email, Argon2id PHC hash, display name,
/// and active flag; belongs to no tenant. [`PasswordAuth`](super::PasswordAuth) stores the email,
/// and looks it up, trimmed and lowercased, so one address holds one account whatever case it is
/// typed in; a resource that edits the email must store it the same way.
/// Its [`PanelModel`](crate::PanelModel) leave out the hash, so a resource over the panel's
/// admins lists them. A resource's edit form that changes an admin's email or password hash ends
/// that admin's sessions, pending sign-ins, reset links and pending email change in the same
/// transaction; a custom action that changes them does not, and calls
/// [`revoke_sessions_for_user`](super::revoke_sessions_for_user) itself; one that changes `active`
/// to `false` ends them at the admin's next request.
#[derive(Debug, Clone, toasty::Model, crate::PanelModel)]
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

    fn can_access_panel(&self, _panel: &crate::PanelId) -> bool {
        self.active
    }
}

/// Ends the sessions, pending sign-ins, reset links and pending email change of the user a panel
/// write changed the sign-in details of, through `ex`: the first mounted authenticator whose
/// [`changed_credentials`](super::Authenticator::changed_credentials) names one, on every panel
/// whose authenticator loads its type. A user editing their own account moves to a fresh session,
/// as on the profile page. A write no authenticator answers for passes through.
pub(crate) async fn end_changed_credentials(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    before: &(dyn std::any::Any + Send + Sync),
    after: &(dyn std::any::Any + Send + Sync),
) -> topcoat::Result<()> {
    let Some(panel) = super::realm::current_realm(cx) else {
        return Ok(());
    };
    let mut realms = super::realm::realms(cx);
    if realms.is_empty() {
        realms.push(Arc::clone(panel));
    }
    let Some((user_type, user_id)) = realms.iter().find_map(|realm| {
        let auth = realm.auth.authenticator()?;
        let user_id = auth.changed_credentials(before, after)?;
        Some((auth.user_type(), user_id))
    }) else {
        return Ok(());
    };
    let panels = super::session::credential_panels(cx, user_type, panel);
    if let Some(signed) = super::signed(cx).filter(|signed| {
        signed.user.user_id() == user_id && panels.iter().any(|p| Arc::ptr_eq(p, &signed.realm))
    }) {
        let session = super::session::start(cx).await?;
        return super::session::replace_sessions(cx, ex, signed, user_type, &session).await;
    }
    super::session::end_credentials(ex, &user_id, &panels, super::token::REVOCATION).await
}

/// The shipped account [`PasswordAuth`](super::PasswordAuth) signs in by default. An account
/// whose `active` is `false` signs in nowhere and gets no reset link.
impl PasswordAccount for AdminUser {
    type User = Self;

    fn name() -> Lens<Self, String> {
        crate::lens!(AdminUser.display_name)
    }

    fn email() -> Lens<Self, String> {
        crate::lens!(AdminUser.email)
    }

    fn password_hash() -> Lens<Self, String> {
        crate::lens!(AdminUser.password_hash)
    }

    fn email_verified_at() -> Lens<Self, Option<Timestamp>> {
        crate::lens!(AdminUser.email_verified_at)
    }

    fn account(user: &Self) -> &Self {
        user
    }

    async fn sign_in(
        self,
        _cx: &Cx,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<Option<Self>> {
        Ok(self.active.then_some(self))
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
            errors.add_field("email", email_taken());
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
