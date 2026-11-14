//! The models Tablo ships, for auth and the bell: [`models!`](crate::models!), which registers
//! them beside the app's, and [`erase_user`], which deletes a user's rows from them.

use crate::{
    auth::{AdminUser, AuthFactor, AuthSession, AuthThrottle, AuthToken, ColumnPreference},
    notification::StoredNotification,
    toasty_compat::model::AppSchema,
};

/// Register every model Tablo ships in `models`: [`AdminUser`], [`AuthSession`],
/// [`ColumnPreference`], [`AuthToken`], [`AuthFactor`], [`AuthThrottle`] and
/// [`StoredNotification`].
///
/// [`models!`](crate::models!) calls it; call it directly when the app builds its
/// [`ModelSet`](toasty::schema::ModelSet) by hand. A model the panel never reads costs an empty
/// table and nothing else.
///
/// Toasty names each table after its model, so an app with a model of one of these names, such
/// as its own `AdminUser` beside a custom authenticator, cannot register both: it lists the models
/// its panel reads in [`toasty::models!`] instead.
pub fn register_models(models: &mut toasty::schema::ModelSet) {
    <AdminUser as toasty::schema::Model>::register(models);
    <AuthSession as toasty::schema::Model>::register(models);
    <ColumnPreference as toasty::schema::Model>::register(models);
    <AuthToken as toasty::schema::Model>::register(models);
    <AuthFactor as toasty::schema::Model>::register(models);
    <AuthThrottle as toasty::schema::Model>::register(models);
    <StoredNotification as toasty::schema::Model>::register(models);
}

/// Deletes every row the tables Tablo ships hold for `user_id`, its
/// [`PanelUser::user_id`](crate::PanelUser::user_id), on every panel, through `ex`: its sessions,
/// its mailed links and pending sign-ins, its second factors and recovery codes, its column
/// choices and its notifications. Call it where the app deletes an account, inside the same
/// transaction, rather than deleting those tables by name: a table a later release ships joins it.
///
/// It reads a model only when `ex`'s schema registers it, so an app that registers the few its
/// panel reads calls it as is. It leaves three things to the app:
///
/// - the user record, an [`AdminUser`](crate::auth::AdminUser) row included, which is the app's to
///   delete;
/// - the [`AuthThrottle`](crate::auth::AuthThrottle) counts, which hold a hash of the login and the
///   address, never either in the clear, and end with their window;
/// - what the app's own tables and the audit log record of the user.
///
/// A page the user has open keeps what it shows; its next request is anonymous.
///
/// ```rust
/// # async fn delete_account(db: &mut toasty::Db, user_id: &str) -> toasty::Result<()> {
/// use toasty::Executor as _;
///
/// let mut tx = db.transaction().await?;
/// tablo_core::erase_user(&mut tx, user_id).await?;
/// // ... delete the app's own rows for the user, then the user.
/// tx.commit().await
/// # }
/// ```
///
/// # Errors
///
/// The driver's error when a delete fails; the transaction then holds whatever ran before it.
pub async fn erase_user(ex: &mut dyn toasty::Executor, user_id: &str) -> toasty::Result<()> {
    let schema = AppSchema::of_executor(ex);
    let id = user_id.to_owned();
    if schema.registers("AuthSession") {
        AuthSession::filter(AuthSession::fields().user_id().eq(id.clone()))
            .delete()
            .exec(&mut *ex)
            .await?;
    }
    if schema.registers("AuthToken") {
        AuthToken::filter(AuthToken::fields().user_id().eq(id.clone()))
            .delete()
            .exec(&mut *ex)
            .await?;
    }
    if schema.registers("AuthFactor") {
        AuthFactor::filter(AuthFactor::fields().user_id().eq(id.clone()))
            .delete()
            .exec(&mut *ex)
            .await?;
    }
    if schema.registers("ColumnPreference") {
        ColumnPreference::filter(ColumnPreference::fields().user_id().eq(id.clone()))
            .delete()
            .exec(&mut *ex)
            .await?;
    }
    if schema.registers("StoredNotification") {
        StoredNotification::filter(StoredNotification::fields().user_id().eq(id))
            .delete()
            .exec(&mut *ex)
            .await?;
    }
    Ok(())
}

/// [`toasty::models!`] with the models Tablo ships appended, so a panel's `Db` holds every table
/// auth reads.
///
/// It takes what `toasty::models!` takes: the app's models, and `crate::*` or `some_crate::*` for
/// every model a crate declares. A `Db` built from it never fails the mount with
/// `MissingAuthModels`. An app with a model named as one of Tablo's lists them in
/// `toasty::models!` instead: see [`register_models`](crate::register_models).
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key]
/// #     #[auto]
/// #     id: uuid::Uuid,
/// # }
/// # async fn connect() -> toasty::Result<toasty::Db> {
/// toasty::Db::builder()
///     .models(tablo_core::models!(Post))
///     .connect("sqlite::memory:")
///     .await
/// # }
/// ```
#[macro_export]
macro_rules! models {
    ($($models:tt)*) => {{
        let mut models = ::toasty::models!($($models)*);
        $crate::register_models(&mut models);
        models
    }};
}

#[cfg(test)]
mod tests;
