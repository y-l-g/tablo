use super::*;

const MINUTE: Duration = Duration::from_secs(60);

#[test]
fn a_login_past_its_limit_is_refused_until_the_window_ends() {
    let throttle = LoginThrottle::new(2, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("ada@example.com", start));
    assert!(throttle.attempt_at(" ADA@example.com ", start));
    assert!(
        !throttle.attempt_at("ada@example.com", start + Duration::from_secs(59)),
        "the third attempt in the window, however the login is spelled"
    );
    assert!(
        throttle.attempt_at("grace@example.com", start),
        "another login keeps its own count"
    );
    assert!(
        throttle.attempt_at("ada@example.com", start + MINUTE),
        "a new window starts a new count"
    );
}

#[test]
fn a_sign_in_clears_the_count() {
    let throttle = LoginThrottle::new(1, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("ada@example.com", start));
    throttle.clear("Ada@Example.com");
    assert!(throttle.attempt_at("ada@example.com", start));
}

#[test]
fn an_off_throttle_counts_nothing() {
    let throttle = LoginThrottle::off();
    let start = Instant::now();
    assert!((0..100).all(|_| throttle.attempt_at("ada@example.com", start)));
}

#[test]
fn a_full_map_drops_the_oldest_count() {
    let throttle = LoginThrottle::new(1, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("first", start));
    for n in 1..MAX_TRACKED {
        assert!(throttle.attempt_at(&n.to_string(), start + Duration::from_millis(1)));
    }
    assert!(throttle.attempt_at("one more", start + Duration::from_millis(2)));
    assert_eq!(throttle.counts.lock().unwrap().len(), MAX_TRACKED);
    assert!(
        throttle.attempt_at("first", start + Duration::from_millis(3)),
        "the oldest count was dropped"
    );
}
