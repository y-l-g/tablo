//! The shipped credentials: the [`AdminUser`] model, Argon2id hashing, and the
//! [`PasswordAuth`] authenticator over them.

use jiff::Timestamp;
use topcoat::context::Cx;
use uuid::Uuid;

use super::{
    Authenticator, PanelUser, Profile, ProfileEditor, ProfileUpdate, Registrar, SignUp,
    infrastructure_failure, profile::CURRENT_PASSWORD_FIELD,
};
use crate::FieldErrors;

/// A dummy Argon2id PHC string verified against when the account does not
/// exist, so unknown logins pay the same work as known ones (ADR-0013).
/// Generated with `Argon2::default()` parameters (`m=19456,t=2,p=1`).
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$h3oXdPBVwhcgZ1OTO/PuzQ$zLrHLgIkwhqu4ZlLTfSyB8mPuL6mAtaswv/eXJ5ADO8";

/// Shipped credential model with unique email, Argon2id PHC hash, display name,
/// and active flag; belongs to no tenant.
/// Its [`ModelColumns`](crate::ModelColumns) leave out the hash, so a resource over the panel's
/// admins lists them.
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
    pub created_at: Timestamp,
}

impl PanelUser for AdminUser {
    fn user_id(&self) -> String {
        self.id.to_string()
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn can_access_panel(&self) -> bool {
        self.active
    }
}

/// Shipped default authenticator: Argon2id verification against [`AdminUser`].
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
        let user = AdminUser::filter(AdminUser::fields().email().eq(login.to_string()))
            .first()
            .exec(&mut db)
            .await
            .map_err(infrastructure_failure)?;
        let hash = user.as_ref().map(|user| user.password_hash.as_str());
        if !verify_password(password, hash) {
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

/// Registers an active [`AdminUser`] from a [`SignUp`], refusing a password
/// [`new_password_errors`](super::new_password_errors) refuses and an email another account
/// holds. Install it with [`Auth::registration`](super::Auth::registration).
impl Registrar for PasswordAuth {
    type Input = SignUp;
    type User = AdminUser;

    async fn validate(
        &self,
        _cx: &Cx,
        sign_up: &SignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let mut errors = sign_up.password_errors();
        let taken = AdminUser::filter(AdminUser::fields().email().eq(sign_up.email.clone()))
            .first()
            .exec(ex)
            .await
            .map_err(infrastructure_failure)?;
        if taken.is_some() {
            errors.add("email", "Email has already been taken");
        }
        Ok(errors)
    }

    async fn register(
        &self,
        _cx: &Cx,
        sign_up: SignUp,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<AdminUser> {
        create_admin(ex, &sign_up.email, &sign_up.password, &sign_up.name).await
    }
}

/// Edits the signed-in [`AdminUser`]'s name, email and password through a [`Profile`]. Changing
/// the email or the password asks for the current password, and refuses a new password
/// [`new_password_errors`](super::new_password_errors) refuses and, once the current password
/// verifies, an email another account holds. A changed email or password is a
/// [`ProfileUpdate::Credentials`]. Install it with [`Auth::profile`](super::Auth::profile).
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
        let new_email = profile.email != admin.email;
        if new_email || profile.password.is_some() {
            match &profile.current_password {
                None => errors.add(
                    CURRENT_PASSWORD_FIELD,
                    "Enter your current password to change your email or password",
                ),
                Some(current) if !verify_password(current, Some(&admin.password_hash)) => {
                    errors.add(CURRENT_PASSWORD_FIELD, "The current password is incorrect");
                }
                Some(_) => {}
            }
        }
        // Only the account's owner learns whether another account holds an email: a stolen
        // session without the password must not probe addresses.
        if new_email && !errors.contains_key(CURRENT_PASSWORD_FIELD) {
            let taken = AdminUser::filter(AdminUser::fields().email().eq(profile.email.clone()))
                .first()
                .exec(ex)
                .await
                .map_err(infrastructure_failure)?;
            if taken.is_some_and(|other| other.id != admin.id) {
                errors.add("email", "Email has already been taken");
            }
        }
        Ok(errors)
    }

    async fn save(
        &self,
        _cx: &Cx,
        admin: &AdminUser,
        profile: Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        let credentials = profile.email != admin.email || profile.password.is_some();
        let mut update = AdminUser::filter(AdminUser::fields().id().eq(admin.id))
            .update()
            .display_name(profile.name)
            .email(profile.email);
        if let Some(password) = &profile.password {
            update.set_password_hash(hash_password(password)?);
        }
        update.exec(ex).await?;
        Ok(if credentials {
            ProfileUpdate::Credentials
        } else {
            ProfileUpdate::Details
        })
    }
}

/// Creates an active [`AdminUser`] that signs in with `email` and `password`, storing only the
/// password's Argon2id hash: the account a new app seeds to reach its panel.
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
    let admin = toasty::create!(AdminUser {
        email: email.to_string(),
        password_hash: hash_password(password)?,
        display_name: display_name.to_string(),
        active: true,
        created_at: Timestamp::now(),
    })
    .exec(ex)
    .await?;
    Ok(admin)
}

/// Hashes a password with Argon2id into a PHC string for storage; never stores
/// the plaintext.
pub fn hash_password(password: &str) -> topcoat::Result<String> {
    use argon2::password_hash::PasswordHasher;

    argon2::Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(topcoat::Error::from)
}

/// Verifies a password against an Argon2id PHC hash, or `None` for an unknown
/// account; unknown accounts verify against a dummy hash so response times do
/// not reveal which accounts exist.
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
/// if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())) {
///     return Ok(None);
/// }
/// # Ok(staff)
/// # }
/// ```
#[must_use]
pub fn verify_password(password: &str, hash: Option<&str>) -> bool {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};

    let Ok(parsed) = PasswordHash::new(hash.unwrap_or(DUMMY_PASSWORD_HASH)) else {
        return false;
    };
    let verified = argon2::Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok();
    verified && hash.is_some()
}
