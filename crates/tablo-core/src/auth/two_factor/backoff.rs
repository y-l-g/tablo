//! Wrong codes after a password: each one past the first few waits longer than the last, on the
//! account's confirmed factor, so guessing codes takes the password again and again and slows to
//! one an hour.

use std::time::Duration;

use jiff::Timestamp;

use super::{AuthFactor, factor_code};
use crate::auth::infrastructure_failure;

/// How many wrong codes in a row a sign-in takes before each further one waits, and before the
/// challenge ends: the user signs in with their password again after every this many.
pub(in crate::auth) const FREE_FAILURES: i64 = 5;

/// The wait after the first wrong code past [`FREE_FAILURES`]; each further one doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(30);

/// The longest wait between two codes.
const LONGEST_WAIT: Duration = Duration::from_hours(1);

/// How long the next code waits after `failures` wrong ones in a row.
fn wait_after(failures: i64) -> Duration {
    if failures < FREE_FAILURES {
        return Duration::ZERO;
    }
    let doublings = u32::try_from(failures - FREE_FAILURES).unwrap_or(u32::MAX);
    FIRST_WAIT
        .checked_mul(2u32.saturating_pow(doublings.min(31)))
        .map_or(LONGEST_WAIT, |wait| wait.min(LONGEST_WAIT))
}

/// How long `code` must wait before `factor` checks it: a factor's code waits out its
/// [`retry_at`](AuthFactor::retry_at), a recovery code never does.
pub(in crate::auth) fn wait(factor: &AuthFactor, code: &str, now: Timestamp) -> Option<Duration> {
    factor_code(code)?;
    let retry_at = factor.retry_at?;
    let left = now.duration_until(retry_at).unsigned_abs();
    (retry_at > now).then_some(left)
}

/// Records one more wrong code on `factor` through `ex`, and when the next one may come.
/// Answers the wrong codes in a row so far.
pub(in crate::auth) async fn refused(
    ex: &mut dyn toasty::Executor,
    factor: &AuthFactor,
    now: Timestamp,
) -> topcoat::Result<i64> {
    let failures = factor.failures.saturating_add(1);
    let wait = wait_after(failures);
    let retry_at = (!wait.is_zero())
        .then(|| now.checked_add(wait).ok())
        .flatten();
    AuthFactor::filter(AuthFactor::fields().id().eq(factor.id))
        .update()
        .failures(failures)
        .retry_at(retry_at)
        .exec(ex)
        .await
        .map_err(infrastructure_failure)?;
    Ok(failures)
}

/// Clears `factor`'s wrong codes through `ex`, after a right one.
pub(in crate::auth) async fn admitted(
    ex: &mut dyn toasty::Executor,
    factor: &AuthFactor,
) -> topcoat::Result<()> {
    if factor.failures == 0 && factor.retry_at.is_none() {
        return Ok(());
    }
    AuthFactor::filter(AuthFactor::fields().id().eq(factor.id))
        .update()
        .failures(0)
        .retry_at(None)
        .exec(ex)
        .await
        .map_err(infrastructure_failure)
}

#[cfg(test)]
mod tests;
