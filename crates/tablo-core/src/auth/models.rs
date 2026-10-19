//! The models auth ships, and [`models!`](crate::models!), which registers them beside the app's.

use super::{AuthSession, ColumnPreference};

/// Register every model Tablo ships in `models`: [`AuthSession`], [`ColumnPreference`], the
/// `password` feature's `AdminUser` and the `mail` feature's `PasswordResetToken`.
///
/// [`models!`](crate::models!) calls it; call it directly when the app builds its
/// [`ModelSet`](toasty::schema::ModelSet) by hand. A model the panel never reads costs an empty
/// table and nothing else.
pub fn register_models(models: &mut toasty::schema::ModelSet) {
    <AuthSession as toasty::schema::Model>::register(models);
    <ColumnPreference as toasty::schema::Model>::register(models);
    #[cfg(feature = "password")]
    <super::AdminUser as toasty::schema::Model>::register(models);
    #[cfg(feature = "mail")]
    <super::PasswordResetToken as toasty::schema::Model>::register(models);
}

/// [`toasty::models!`] with the models Tablo ships appended, so a panel's `Db` holds every table
/// auth reads.
///
/// It takes what `toasty::models!` takes: the app's models, and `crate::*` or `some_crate::*` for
/// every model a crate declares. A `Db` built from it never fails the mount with
/// `MissingAuthModels`.
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
        $crate::auth::register_models(&mut models);
        models
    }};
}
