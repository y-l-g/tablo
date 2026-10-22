//! The time zone a request shows and reads wall-clock times in.

use jiff::tz::TimeZone;
use topcoat::context::{Cx, try_app_context};

/// The time zone the request reads and shows wall-clock times in: the signed-in user's
/// [`time_zone`](crate::PanelUser::time_zone), else the request panel's
/// [`time_zone`](crate::Panel::time_zone), else UTC.
///
/// A timestamp's control shows its stored instant in this zone and reads the wall-clock time it
/// posts back in it, and a [`DateColumn`](crate::DateColumn) and a
/// [`DateFilter`](crate::DateFilter) read a timestamp's calendar day in it.
///
/// - `14:00` entered in Paris on 20 October stores `12:00:00Z`. A time the zone skips moves forward
///   by the gap, and a time it repeats takes the earlier instant; a value posted back untouched
///   keeps its stored instant, in a repeater's rows too. A value posted with an offset, such as
///   `2026-10-20T14:00:00-05:00`, names that instant and stores it in UTC. A `min` or `max` refusal
///   spells the bound as the control shows it.
/// - A civil date or date-time names its day whatever the zone, but a timestamp's day is the
///   viewer's: two users in different zones who open the same `?f.starts_at=2026-10-20` see
///   different rows.
/// - The stored value is the instant: an export and a [`TextColumn`](crate::TextColumn) spell it in
///   RFC 3339, in UTC.
/// - A custom control receives the stored RFC 3339 value and posts a value the panel does not
///   convert, so it posts an instant with an offset: a wall-clock time is refused.
/// - [`Schema::read`](crate::Schema::read) reads a timestamp's wall-clock time in this zone too;
///   [`TypedValue::parse_input`](crate::extend::TypedValue::parse_input) alone refuses one, since
///   it names no instant.
pub fn time_zone(cx: &Cx) -> TimeZone {
    try_app_context::<ZoneSource>(cx)
        .and_then(|ZoneSource(session)| session(cx))
        .unwrap_or(TimeZone::UTC)
}

/// Finds the zone of the request's user or panel; the first panel mounted on a router installs it
/// in the app context.
pub(crate) struct ZoneSource(pub(crate) fn(&Cx) -> Option<TimeZone>);
