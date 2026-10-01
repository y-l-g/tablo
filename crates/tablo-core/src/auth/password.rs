//! The shipped credentials: the [`AdminUser`] model, Argon2id hashing, and the
//! [`PasswordAuth`] authenticator over them.

use jiff::Timestamp;
use topcoat::context::Cx;
use uuid::Uuid;

use super::{Authenticator, PanelUser, infrastructure_failure};

/// A dummy Argon2id PHC string verified against when the account does not
/// exist, so unknown logins pay the same work as known ones (ADR-0013).
/// Generated with `Argon2::default()` parameters (`m=19456,t=2,p=1`).
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$h3oXdPBVwhcgZ1OTO/PuzQ$zLrHLgIkwhqu4ZlLTfSyB8mPuL6mAtaswv/eXJ5ADO8";

/// The shipped credential model (ADR-0013): unique email, Argon2id PHC hash,
/// display name, and an active flag. It belongs to no tenant; an app whose
/// users belong to tenants implements [`PanelUser`] for its own model.
///
/// Register it (plus [`AuthSession`](super::AuthSession)) in the app's `Db`
/// model list, seed one row, and the default [`PasswordAuth`] works with no
/// further wiring:
/// `toasty::models!(…, tablo_core::auth::AdminUser, tablo_core::auth::AuthSession)`.
#[derive(Debug, Clone, toasty::Model)]
pub struct AdminUser {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    /// Argon2id password hash in PHC string format.
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

/// The shipped default authenticator: Argon2id verification against
/// [`AdminUser`]. An inactive user does not authenticate.
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
        // A deactivated account stops resolving: its live sessions are
        // purged and the next request redirects to login (spec #127 US11).
        Ok(user.filter(|user| user.active))
    }
}

/// Hash a password with Argon2id into a PHC string (the shipped storage
/// format). Seeds and record fns call this; it never stores the plaintext.
///
/// # Errors
///
/// When Argon2 cannot hash, which the default parameters do not cause.
pub fn hash_password(password: &str) -> topcoat::Result<String> {
    use argon2::password_hash::PasswordHasher;

    argon2::Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(topcoat::Error::from)
}

/// Verify a password against the account's Argon2id PHC hash, or `None` for
/// an account that does not exist.
///
/// An unknown account is verified against a dummy hash and answers `false`,
/// so it costs the same work as a wrong password and response times do not
/// reveal which accounts exist. A malformed hash answers `false`. An
/// [`Authenticator`] over its own user table calls it with the hash it loaded:
///
/// ```ignore
/// let staff = Staff::filter_by_email(login).first().exec(&mut db).await?;
/// if !verify_password(password, staff.as_ref().map(|s| s.password_hash.as_str())) {
///     return Ok(None);
/// }
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
