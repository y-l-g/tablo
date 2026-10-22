use super::*;

/// topcoat#441: the shard is served at the named path, so its endpoint is the
/// same in every build.
#[test]
fn live_toaster_endpoint_is_the_named_path() {
    use topcoat::router::Route as _;

    assert_eq!(live_toaster.path().as_str(), LIVE_TOASTER_PATH);
}
