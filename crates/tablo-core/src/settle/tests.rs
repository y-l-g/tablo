use std::time::Duration;

use tokio::sync::oneshot;

use super::AfterResponse;

/// `settled` waits for work still running, and returns once it ends.
#[tokio::test]
async fn settled_waits_for_running_work() {
    let after = AfterResponse::default();
    let (finish, finished) = oneshot::channel::<()>();
    after.run(async move {
        let _ = finished.await;
    });
    let waiting = tokio::time::timeout(Duration::from_millis(50), after.settled()).await;
    assert!(waiting.is_err(), "the work is still running");
    finish.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), after.settled())
        .await
        .expect("the work ended");
}

/// Work that panics still ends, so a shutdown does not wait on it forever.
#[tokio::test]
async fn panicking_work_still_settles() {
    let after = AfterResponse::default();
    after.run(async { panic!("the mail server answered nonsense") });
    tokio::time::timeout(Duration::from_secs(5), after.settled())
        .await
        .expect("the panicked work ended");
}
