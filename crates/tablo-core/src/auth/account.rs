//! [`PasswordAccount`], the user model the stock password sign-in reads, and [`PasswordAuth`],
//! the sign-in, profile, password reset and email verification over one.

use std::{any::Any, future::Future, marker::PhantomData};

use derive_where::derive_where;
use jiff::Timestamp;
use topcoat::{context::Cx, l10n::Locale};

use super::{
    AdminUser, Authenticator, EmailChange, PanelUser, Profile, ProfileEditor, ProfileField,
    ProfileUpdate, hash_password, infrastructure_failure, profile::CURRENT_PASSWORD_FIELD,
    request_email_change, verify_password,
};
use crate::{
    EmailVerifier, FieldErrors, FormInput, Lens, PasswordResetter,
    toasty_compat::{
        RowUpdate,
        pk::{canonical_key, pk_eq_expr, pk_filter, pk_text},
    },
};

/// A user model [`PasswordAuth`] signs in by email and password, and whose own account it edits,
/// resets and verifies.
///
/// The app names four fields of its model through [`lens!`](crate::lens!), and says what the
/// panel signs in: the model itself, or a value loaded beside it, such as the user with their
/// tenants. [`AdminUser`] is one.
///
/// ```rust
/// # use jiff::Timestamp;
/// # use topcoat::context::Cx;
/// use tablo_core::{Lens, Membership, PanelUser, PasswordAccount, lens};
///
/// #[derive(Debug, Clone, toasty::Model)]
/// struct User {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     name: String,
///     #[unique]
///     email: String,
///     password: String,
///     email_verified_at: Option<Timestamp>,
/// }
///
/// /// The user and the tenants they belong to.
/// struct SignedIn {
///     user: User,
///     tenants: Vec<Membership>,
/// }
///
/// impl PanelUser for SignedIn {
///     fn user_id(&self) -> String {
///         self.user.id.to_string()
///     }
///     fn display_name(&self) -> &str {
///         &self.user.name
///     }
///     fn email(&self) -> Option<&str> {
///         Some(&self.user.email)
///     }
///     fn can_access_panel(&self, _panel: &str) -> bool {
///         true
///     }
///     fn tenants(&self) -> &[Membership] {
///         &self.tenants
///     }
/// }
///
/// impl PasswordAccount for User {
///     type User = SignedIn;
///
///     fn name() -> Lens<Self, String> {
///         lens!(User.name)
///     }
///     fn email() -> Lens<Self, String> {
///         lens!(User.email)
///     }
///     fn password_hash() -> Lens<Self, String> {
///         lens!(User.password)
///     }
///     fn email_verified_at() -> Lens<Self, Option<Timestamp>> {
///         lens!(User.email_verified_at)
///     }
///     fn account(user: &SignedIn) -> &User {
///         &user.user
///     }
///     async fn sign_in(
///         self,
///         _cx: &Cx,
///         _ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<Option<SignedIn>> {
///         // Load the user's memberships through `_ex` here.
///         Ok(Some(SignedIn {
///             user: self,
///             tenants: Vec::new(),
///         }))
///     }
/// }
/// ```
///
/// [`PasswordAuth`] stores an email trimmed and lowercased, as
/// [`normalize_email`](super::normalize_email) spells it, and looks it up the same way, so a
/// resource or a sign-up that writes the email stores it so too. The model's primary key is one
/// field, and the `User`'s [`PanelUser::user_id`] spells it as its `Display` does, such as
/// `self.id.to_string()`: a sign-in whose id spells another key is refused as a declaration
/// error.
///
/// A panel's edit form over the model that changes an account's email or password hash ends its
/// sessions, pending sign-ins, reset links and pending email change on every panel that signs in
/// `Self::User`, as it does an [`AdminUser`]'s; a custom action calls
/// [`revoke_sessions_for_user`](super::revoke_sessions_for_user) itself.
pub trait PasswordAccount:
    toasty::schema::Model<
        UpdateQuery: From<toasty::stmt::Query<toasty::stmt::List<Self>>>
                         + toasty::stmt::IntoStatement<Returning = ()>,
    > + toasty::stmt::IntoExpr<Self>
    + Send
    + Sync
    + 'static
{
    /// What the panel signs in: `Self`, or a value holding it.
    type User: PanelUser;

    /// The display name, which the profile page edits.
    fn name() -> Lens<Self, String>;

    /// The address the account signs in with, unique across the table.
    fn email() -> Lens<Self, String>;

    /// The password's Argon2id hash in PHC string format, as
    /// [`hash_password`](super::hash_password) writes it.
    fn password_hash() -> Lens<Self, String>;

    /// When the account proved it reads its email; `None` until it does, and again after the
    /// email changes. Every email change writes it; only a panel with
    /// [`Auth::email_verification`](crate::Auth::email_verification) reads it.
    fn email_verified_at() -> Lens<Self, Option<Timestamp>>;

    /// The account `user` signs in as.
    fn account(user: &Self::User) -> &Self;

    /// The account as the panel signs it in, loaded through `ex`, or `None` when it no longer
    /// signs in, such as a deactivated account: its login fails as a wrong password does, its
    /// sessions end at their next request, and it gets no reset link.
    ///
    /// It runs on every request of a signed-in session, so a removed membership or a deactivation
    /// takes effect at once. It reads through `ex`, the connection the caller already holds.
    fn sign_in(
        self,
        cx: &Cx,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Refuses a profile change the app does not allow `user`, such as a new email on an account
    /// whose sign-in details are locked. Defaults to none. It runs before the current password is
    /// checked, and the page renders each error under its field's control. `profile.email` is as
    /// typed: compare it through [`normalize_email`](super::normalize_email), as the stored email
    /// is spelled.
    fn profile_errors(user: &Self::User, profile: &Profile) -> FieldErrors<ProfileField> {
        let _ = (user, profile);
        FieldErrors::new()
    }

    /// Stores `locale` as `user`'s, as [`Authenticator::store_locale`] describes. Defaults to
    /// storing nothing.
    fn store_locale(
        cx: &Cx,
        user: &Self::User,
        locale: &Locale,
    ) -> impl Future<Output = topcoat::Result<()>> + Send {
        let _ = (cx, user, locale);
        async { Ok(()) }
    }
}

/// The stock sign-in by email and password, over a [`PasswordAccount`]: the authenticator, and
/// the [`ProfileEditor`], [`PasswordResetter`] and [`EmailVerifier`] for its users.
///
/// `PasswordAuth` reads the shipped [`AdminUser`]; `PasswordAuth::<User>::default()` reads
/// the app's own `User` model.
///
/// ```rust
/// use tablo_core::{Auth, PasswordAuth};
///
/// let auth = Auth::custom(PasswordAuth::new()).profile(PasswordAuth::new());
/// # let _ = auth;
/// ```
///
/// An unknown email costs the same work as a wrong password, and every failure shows the same
/// message. It looks an email up trimmed and lowercased, so `Ada@Example.com` and
/// `ada@example.com` are one account. A row stored with capitals cannot sign in or reset its
/// password: list the accounts that differ only in case with `SELECT LOWER(TRIM(email)) FROM
/// users GROUP BY 1 HAVING COUNT(*) > 1`, merge or delete them, then run `UPDATE users SET email
/// = LOWER(TRIM(email))`. SQLite's `LOWER` folds ASCII letters only, so lowercase any other
/// capital by hand.
///
/// As the profile editor it edits the name, the email and the password through a [`Profile`].
/// Changing the email or the password asks for the current password, and refuses a new password
/// [`new_password_errors`](super::new_password_errors) refuses and, once the current password
/// verifies, an email another account holds. A changed password is a
/// [`ProfileUpdate::Credentials`]. A new email goes through [`request_email_change`]: on a panel
/// with [`Auth::email_verification`](crate::Auth::email_verification) it waits for its link,
/// elsewhere it is stored at once, as a credential change.
#[derive_where(Debug, Clone, Copy, Default)]
pub struct PasswordAuth<A = AdminUser> {
    #[derive_where(skip)]
    account: PhantomData<fn() -> A>,
}

impl PasswordAuth {
    /// The stock sign-in over the shipped [`AdminUser`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            account: PhantomData,
        }
    }
}

impl<A: PasswordAccount> Authenticator for PasswordAuth<A> {
    type User = A::User;

    async fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> topcoat::Result<Option<A::User>> {
        let mut db = crate::db::db(cx)?;
        let account = by_email::<A>(&mut db, login).await?;
        let hash = account
            .as_ref()
            .map(|account| A::password_hash().read(account).clone());
        if !verify_password(password, hash.as_deref()).await {
            return Ok(None);
        }
        match account {
            Some(account) => signed_in(account, cx, &mut db).await,
            None => Ok(None),
        }
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<A::User>> {
        let Some(filter) = pk_eq_expr::<A>(id) else {
            return Ok(None);
        };
        let mut db = crate::db::db(cx)?;
        let account = toasty::stmt::Query::<toasty::stmt::List<A>>::all()
            .filter(filter)
            .first()
            .exec(&mut db)
            .await
            .map_err(infrastructure_failure)?;
        match account {
            Some(account) => signed_in(account, cx, &mut db).await,
            None => Ok(None),
        }
    }

    /// Checks `password` against the account's own hash, whatever [`PanelUser::email`] answers.
    async fn confirm_password(
        &self,
        _cx: &Cx,
        user: &A::User,
        password: &str,
    ) -> topcoat::Result<bool> {
        let hash = A::password_hash().read(A::account(user));
        Ok(verify_password(password, Some(hash)).await)
    }

    async fn store_locale(&self, cx: &Cx, user: &A::User, locale: &Locale) -> topcoat::Result<()> {
        A::store_locale(cx, user, locale).await
    }

    /// The account's key when an edit changed its email or its password hash.
    fn changed_credentials(&self, before: &dyn Any, after: &dyn Any) -> Option<String> {
        let (before, after) = (before.downcast_ref::<A>()?, after.downcast_ref::<A>()?);
        let changed = A::email().read(before) != A::email().read(after)
            || A::password_hash().read(before) != A::password_hash().read(after);
        changed.then(|| pk_text(before))
    }
}

impl<A: PasswordAccount> ProfileEditor for PasswordAuth<A> {
    type Input = Profile;
    type User = A::User;

    fn fill(&self, _cx: &Cx, user: &A::User) -> Profile {
        let account = A::account(user);
        Profile {
            name: A::name().read(account).clone(),
            email: A::email().read(account).clone(),
            password: None,
            password_confirmation: String::new(),
            current_password: None,
        }
    }

    async fn validate(
        &self,
        _cx: &Cx,
        user: &A::User,
        profile: &Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<FieldErrors> {
        let account = A::account(user);
        let mut errors = A::profile_errors(user, profile).keyed(Profile::key);
        errors.extend(profile.password_errors());
        let email = normalize_email(&profile.email);
        let new_email = email != *A::email().read(account);
        if new_email || profile.password.is_some() {
            let hash = A::password_hash().read(account);
            match &profile.current_password {
                None => errors.add(
                    CURRENT_PASSWORD_FIELD,
                    crate::text!("tablo-profile-current-password-needed"),
                ),
                Some(current) if !verify_password(current, Some(hash)).await => {
                    errors.add(
                        CURRENT_PASSWORD_FIELD,
                        crate::text!("tablo-profile-current-password-wrong"),
                    );
                }
                Some(_) => {}
            }
        }
        let email_key = Profile::key(ProfileField::Email);
        // Only the account's owner learns whether another account holds an email: a stolen
        // session without the password must not probe addresses.
        if new_email
            && !errors.contains_key(CURRENT_PASSWORD_FIELD)
            && !errors.contains_key(email_key)
            && by_email::<A>(ex, &email)
                .await?
                .is_some_and(|other| pk_text(&other) != pk_text(account))
        {
            errors.add(email_key, email_taken());
        }
        Ok(errors)
    }

    async fn save(
        &self,
        cx: &Cx,
        user: &A::User,
        profile: Profile,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<ProfileUpdate> {
        let account = A::account(user);
        let email = normalize_email(&profile.email);
        let stored = email != *A::email().read(account)
            && request_email_change(cx, &mut *ex, user, &email).await? == EmailChange::Immediate;
        let mut update = RowUpdate::new::<A>(pk_filter(account));
        update.set(A::name().path(), profile.name);
        if stored {
            update.set(A::email().path(), email);
            update.set(A::email_verified_at().path(), None);
        }
        if let Some(password) = &profile.password {
            update.set(A::password_hash().path(), hash_password(password).await?);
        }
        update.exec(ex).await?;
        Ok(if stored || profile.password.is_some() {
            ProfileUpdate::Credentials
        } else {
            ProfileUpdate::Details
        })
    }
}

impl<A: PasswordAccount> PasswordResetter for PasswordAuth<A> {
    type User = A::User;

    async fn find_by_email(&self, cx: &Cx, email: &str) -> topcoat::Result<Option<A::User>> {
        let mut db = crate::db::db(cx)?;
        match by_email::<A>(&mut db, email).await? {
            Some(account) => signed_in(account, cx, &mut db).await,
            None => Ok(None),
        }
    }

    async fn reset_password(
        &self,
        _cx: &Cx,
        user_id: &str,
        password: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let mut update = RowUpdate::new::<A>(by_user_id::<A>(user_id)?);
        update.set(A::password_hash().path(), hash_password(password).await?);
        update.exec(ex).await?;
        Ok(())
    }
}

impl<A: PasswordAccount> EmailVerifier for PasswordAuth<A> {
    type User = A::User;

    fn is_verified(&self, user: &A::User) -> bool {
        A::email_verified_at().read(A::account(user)).is_some()
    }

    async fn store_verified_email(
        &self,
        _cx: &Cx,
        user_id: &str,
        email: &str,
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        let mut update = RowUpdate::new::<A>(by_user_id::<A>(user_id)?);
        update.set(A::email().path(), normalize_email(email));
        update.set(A::email_verified_at().path(), Some(Timestamp::now()));
        update.exec(ex).await?;
        Ok(())
    }
}

/// The account whose email, as [`normalize_email`] spells it, is `email`.
async fn by_email<A: PasswordAccount>(
    ex: &mut dyn toasty::Executor,
    email: &str,
) -> topcoat::Result<Option<A>> {
    toasty::stmt::Query::<toasty::stmt::List<A>>::all()
        .filter(A::email().path().clone().eq(normalize_email(email)))
        .first()
        .exec(ex)
        .await
        .map_err(infrastructure_failure)
}

/// The filter selecting the account `user_id` names; not found when it spells no key.
fn by_user_id<A: PasswordAccount>(user_id: &str) -> topcoat::Result<toasty::stmt::Expr<bool>> {
    pk_eq_expr::<A>(user_id).ok_or_else(|| topcoat::router::error::not_found().into())
}

/// `account` as the panel signs it in, refusing a user whose id spells another key than the
/// account's, or a key no later request can parse: the session would store an id no later request
/// finds.
async fn signed_in<A: PasswordAccount>(
    account: A,
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
) -> topcoat::Result<Option<A::User>> {
    let key = pk_text(&account);
    let user = account.sign_in(cx, ex).await?;
    if let Some(user) = &user
        && canonical_key::<A>(&user.user_id()).as_deref() != Some(key.as_str())
    {
        return Err(crate::error::declaration(format!(
            "`PanelUser::user_id` answers '{}' for the account whose key is '{key}': a \
             `PasswordAccount` has a single primitive key, and its user spells it as its \
             `Display` does",
            user.user_id()
        )));
    }
    Ok(user)
}

/// `email` as a [`PasswordAccount`] stores it: trimmed and lowercased.
///
/// ```rust
/// assert_eq!(
///     tablo_core::auth::normalize_email(" Ada@Example.COM "),
///     "ada@example.com"
/// );
/// ```
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// The refusal of an email another account holds.
pub(super) fn email_taken() -> crate::Text {
    crate::text!(
        "tablo-field-taken",
        label = crate::text!("tablo-auth-email")
    )
}
