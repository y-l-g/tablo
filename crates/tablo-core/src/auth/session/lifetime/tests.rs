use std::time::Duration;

use jiff::Timestamp;

use super::{Sessions, Standing};
use crate::auth::AuthSession;

fn row(created_at: Timestamp, expires_at: Timestamp) -> AuthSession {
    AuthSession {
        token_hash: String::new(),
        user_id: String::new(),
        panel: "/admin".to_owned(),
        tenant: None,
        expires_at,
        created_at,
        impersonating: None,
        impersonating_until: None,
    }
}

const IDLE: Duration = Duration::from_mins(30);

fn idle() -> Sessions {
    Sessions::new()
        .lifetime(Duration::from_hours(10))
        .idle_timeout(IDLE)
}

/// A request soon after the last recorded one writes nothing: the end moves once per slack.
#[test]
fn an_end_within_the_slack_stays() {
    let now = Timestamp::now();
    let recorded = now - Duration::from_mins(2) + IDLE;

    let standing = idle().check(&row(now - Duration::from_hours(1), recorded), now);

    assert_eq!(standing, Standing::Live(None));
}

#[test]
fn an_end_past_the_slack_moves_to_the_idle_timeout() {
    let now = Timestamp::now();
    let recorded = now - Duration::from_mins(4) + IDLE;

    let standing = idle().check(&row(now - Duration::from_hours(1), recorded), now);

    assert_eq!(standing, Standing::Live(Some(now + IDLE)));
}

/// A row written before the timeout was set, ending far ahead, is pulled back to it.
#[test]
fn an_end_beyond_the_idle_timeout_is_pulled_back() {
    let now = Timestamp::now();

    let standing = idle().check(&row(now, now + Duration::from_hours(9)), now);

    assert_eq!(standing, Standing::Live(Some(now + IDLE)));
}

#[test]
fn a_lifetime_and_a_timeout_never_go_under_a_minute() {
    let sessions = Sessions::new()
        .lifetime(Duration::ZERO)
        .idle_timeout(Duration::ZERO);
    let now = Timestamp::now();

    assert_eq!(sessions.first_end(now), now + Duration::from_mins(1));
}
