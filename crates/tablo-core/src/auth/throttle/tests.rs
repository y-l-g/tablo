use super::*;

const MINUTE: Duration = Duration::from_secs(60);
const HOME: Option<IpAddr> = Some(IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)));
const AWAY: Option<IpAddr> = Some(IpAddr::V4(std::net::Ipv4Addr::new(198, 51, 100, 7)));

#[test]
fn a_login_past_its_limit_is_refused_until_the_window_ends() {
    let throttle = LoginThrottle::new(2, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("ada@example.com", HOME, start));
    assert!(throttle.attempt_at(" ADA@example.com ", HOME, start));
    assert!(
        !throttle.attempt_at("ada@example.com", HOME, start + Duration::from_secs(59)),
        "the third attempt in the window, however the login is spelled"
    );
    assert!(
        throttle.attempt_at("grace@example.com", HOME, start),
        "another login keeps its own count"
    );
    assert!(
        throttle.attempt_at("ada@example.com", AWAY, start),
        "the login keeps its own count from another address"
    );
    assert!(
        throttle.attempt_at("ada@example.com", HOME, start + MINUTE),
        "a new window starts a new count"
    );
}

#[test]
fn a_sign_in_clears_the_count() {
    let throttle = LoginThrottle::new(1, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("ada@example.com", HOME, start));
    assert!(throttle.attempt_at("ada@example.com", AWAY, start));
    throttle.clear("Ada@Example.com", HOME);
    assert!(throttle.attempt_at("ada@example.com", HOME, start));
    assert!(
        !throttle.attempt_at("ada@example.com", AWAY, start),
        "a sign-in clears only its own address's count"
    );
}

#[test]
fn an_off_throttle_counts_nothing() {
    let throttle = LoginThrottle::off();
    let start = Instant::now();
    assert!((0..100).all(|_| throttle.attempt_at("ada@example.com", HOME, start)));
}

#[test]
fn a_full_map_drops_the_oldest_count() {
    let throttle = LoginThrottle::new(1, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("first", HOME, start));
    for n in 1..MAX_TRACKED {
        assert!(throttle.attempt_at(&n.to_string(), HOME, start + Duration::from_millis(1)));
    }
    assert!(throttle.attempt_at("one more", HOME, start + Duration::from_millis(2)));
    assert_eq!(throttle.counts.lock().unwrap().len(), MAX_TRACKED);
    assert!(
        throttle.attempt_at("first", HOME, start + Duration::from_millis(3)),
        "the oldest count was dropped"
    );
}

#[test]
fn a_full_map_drops_expired_counts_before_the_oldest() {
    let throttle = LoginThrottle::new(1, MINUTE);
    let start = Instant::now();
    assert!(throttle.attempt_at("expired", HOME, start));
    let later = start + MINUTE / 2;
    assert!(throttle.attempt_at("oldest live", HOME, later));
    for n in 2..MAX_TRACKED {
        assert!(throttle.attempt_at(&n.to_string(), HOME, later + Duration::from_millis(1)));
    }
    assert!(throttle.attempt_at("one more", HOME, start + MINUTE));
    assert!(
        !throttle.attempt_at("oldest live", HOME, start + MINUTE),
        "the expired count went first, so the oldest live one stays"
    );
}
