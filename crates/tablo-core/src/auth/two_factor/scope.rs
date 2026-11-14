//! Which panels share an account's factors: those whose authenticator loads the same user type,
//! the ones that already share its password. A factor set up on one then stands on each, so a
//! password alone sets up none on a sibling.

use topcoat::context::Cx;

use super::AuthFactor;
use crate::auth::{
    infrastructure_failure,
    realm::{Realm, current_realm, panel_prefix, realms},
};

/// The prefixes of the panels that share factors with `panel`, itself among them.
pub(super) fn sharing(cx: &Cx, panel: &Realm) -> Vec<String> {
    let user_type = |realm: &Realm| realm.auth.authenticator().map(|auth| auth.user_type());
    let mut prefixes = Vec::new();
    if let Some(own) = user_type(panel) {
        prefixes.extend(
            realms(cx)
                .iter()
                .filter(|realm| user_type(realm) == Some(own))
                .map(|realm| realm.prefix.clone()),
        );
    }
    if !prefixes.contains(&panel.prefix) {
        prefixes.push(panel.prefix.clone());
    }
    prefixes
}

/// The prefixes of the panels that share factors with the request's panel.
pub(super) fn here(cx: &Cx) -> Vec<String> {
    current_realm(cx).map_or_else(|| vec![panel_prefix(cx)], |panel| sharing(cx, panel))
}

/// Every factor `user_id` holds on one of `panels`, oldest first, read through `ex`.
pub(super) async fn factors(
    ex: &mut dyn toasty::Executor,
    panels: &[String],
    user_id: &str,
) -> topcoat::Result<Vec<AuthFactor>> {
    let mut factors = AuthFactor::filter(AuthFactor::fields().user_id().eq(user_id.to_string()))
        .exec(ex)
        .await
        .map_err(infrastructure_failure)?;
    factors.retain(|factor| panels.contains(&factor.panel));
    factors.sort_by_key(|factor| factor.created_at);
    Ok(factors)
}

/// Deletes those of the factors `user_id` holds on the request's panel and the panels sharing
/// with it that `doomed` picks, through `ex`.
pub(super) async fn remove(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    doomed: impl Fn(&AuthFactor) -> bool,
) -> topcoat::Result<()> {
    for factor in factors(&mut *ex, &here(cx), user_id).await? {
        if doomed(&factor) {
            AuthFactor::filter(AuthFactor::fields().id().eq(factor.id))
                .delete()
                .exec(&mut *ex)
                .await
                .map_err(infrastructure_failure)?;
        }
    }
    Ok(())
}
