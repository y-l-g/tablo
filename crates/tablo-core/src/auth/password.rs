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

/// Shipped credential model with unique email, Argon2id PHC hash, display name,
/// and active flag; belongs to no tenant.
#[derive(Debug, Clone, toasty::Model)]
pub struct AdminUser {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    /// Argon2id hash in PHC string format; never stores the plaintext.
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
