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
pub fn time_zone(cx: &Cx) -> TimeZone {
    try_app_context::<ZoneSource>(cx)
        .and_then(|ZoneSource(session)| session(cx))
        .unwrap_or(TimeZone::UTC)
}

/// Finds the zone of the request's user or panel; the first panel mounted on a router installs it
/// in the app context.
pub(crate) struct ZoneSource(pub(crate) fn(&Cx) -> Option<TimeZone>);
