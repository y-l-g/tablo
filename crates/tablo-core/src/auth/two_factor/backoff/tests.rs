use super::*;

/// The first wrong codes are free; past them each waits twice as long as the one before, up
/// to an hour.
#[test]
fn the_wait_doubles_past_the_free_failures_up_to_an_hour() {
    assert_eq!(wait_after(FREE_FAILURES - 1), Duration::ZERO);
    assert_eq!(wait_after(FREE_FAILURES), Duration::from_secs(30));
    assert_eq!(wait_after(FREE_FAILURES + 1), Duration::from_secs(60));
    assert_eq!(wait_after(FREE_FAILURES + 3), Duration::from_secs(240));
    assert_eq!(wait_after(FREE_FAILURES + 7), Duration::from_hours(1));
    assert_eq!(wait_after(i64::MAX), Duration::from_hours(1));
}
