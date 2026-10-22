//! Each signed-in user's choice of the toggleable columns a resource's list shows: the
//! [`ColumnPreference`] rows.

use topcoat::context::Cx;

use super::signed;
use crate::{table::ColumnChoice, topcoat_compat::href};

/// Shipped record of the toggleable columns one user shows or hides in one resource's list, on
/// one panel.
///
/// The panel writes a row when the user applies a choice that departs from the columns' defaults,
/// and deletes it when the user resets to them. Register it with the app's models when a table
/// declares a [`toggleable`](crate::TextColumn::toggleable) or
/// [`hidden_by_default`](crate::TextColumn::hidden_by_default) column and auth is on.
#[derive(Debug, Clone, toasty::Model)]
pub struct ColumnPreference {
    /// The panel, the user and the resource, query-encoded: one row per user per list.
    #[key]
    pub key: String,
    /// [`PanelUser::user_id`](crate::PanelUser::user_id) of the user who chose; indexed so an app
    /// deleting a user can delete their rows, which the panel leaves behind.
    #[index]
    pub user_id: String,
    /// The prefix of the panel the list is on (e.g. `/admin`).
    pub panel: String,
    /// The slug of the resource whose list it is.
    pub resource: String,
    /// The columns the user hides or shows against their defaults, query-encoded. Text, since a
    /// few names outgrow the `VARCHAR(191)` MySQL gives a `String`.
    #[column(type = text)]
    pub columns: String,
}

/// The row key of `user_id`'s choice for the resource at `slug` on the panel at `panel`.
fn preference_key(panel: &str, user_id: &str, slug: &str) -> String {
    href::encode_query(&[
        ("panel".to_string(), panel),
        ("user".to_string(), user_id),
        ("resource".to_string(), slug),
    ])
}

/// The request's user's choice for the resource at `slug`: the defaults without a signed-in
/// user, and when the database cannot answer, since a list renders without the choice.
pub(crate) async fn load_choice(cx: &Cx, slug: &str) -> ColumnChoice {
    let Some(signed) = signed(cx) else {
        return ColumnChoice::default();
    };
    let key = preference_key(&signed.realm.prefix, &signed.user.user_id(), slug);
    let mut db = crate::db::db(cx);
    match ColumnPreference::filter(ColumnPreference::fields().key().eq(key))
        .first()
        .exec(&mut db)
        .await
    {
        Ok(row) => row.map_or_else(ColumnChoice::default, |row| {
            ColumnChoice::decode(&row.columns)
        }),
        Err(error) => {
            tracing::warn!(resource = slug, error = %error, "column preference load failed");
            ColumnChoice::default()
        }
    }
}

/// Stores `choice` as the request's user's for the resource at `slug`, replacing any earlier one;
/// a choice departing from no default leaves no row.
///
/// # Errors
///
/// Forbidden without a signed-in user, and the infrastructure failure when the database cannot
/// answer.
pub(crate) async fn save_choice(cx: &Cx, slug: &str, choice: &ColumnChoice) -> topcoat::Result<()> {
    let Some(signed) = signed(cx) else {
        return Err(topcoat::router::error::forbidden().into());
    };
    let user_id = signed.user.user_id();
    let panel = signed.realm.prefix.clone();
    let key = preference_key(&panel, &user_id, slug);
    let mut db = crate::db::db(cx);
    let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
    ColumnPreference::filter(ColumnPreference::fields().key().eq(key.clone()))
        .delete()
        .exec(&mut tx)
        .await
        .map_err(crate::error::unavailable)?;
    if !choice.is_default() {
        toasty::create!(ColumnPreference {
            key,
            user_id,
            panel,
            resource: slug.to_string(),
            columns: choice.encode(),
        })
        .exec(&mut tx)
        .await
        .map_err(crate::error::unavailable)?;
    }
    tx.commit().await.map_err(crate::error::unavailable)
}
