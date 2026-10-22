//! The models Tablo ships, for auth and the bell, and [`models!`](crate::models!), which registers
//! them beside the app's.

use crate::{
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, ColumnPreference},
    notification::StoredNotification,
};

/// Register every model Tablo ships in `models`: [`AdminUser`], [`AuthSession`],
/// [`ColumnPreference`], [`AuthToken`], [`AuthFactor`] and [`StoredNotification`].
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
    <StoredNotification as toasty::schema::Model>::register(models);
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
