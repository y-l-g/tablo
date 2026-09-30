use std::time::Duration;

use tablo_core::Committed;

use super::*;

/// A wake reaches a subscriber: a feed waiting on a board resumes when the
/// board is notified.
#[tokio::test]
async fn notify_wakes_a_subscriber() {
    let board = Board::default();
    let mut changed = board.subscribe();
    board.notify();

    tokio::time::timeout(Duration::from_secs(1), changed.recv())
        .await
        .expect("notify must wake a subscriber")
        .expect("the sender outlives the receiver");
}

/// The resource hook wakes the process-wide board, so a committed write
/// re-runs every connected feed.
#[tokio::test]
async fn a_committed_write_wakes_the_feed() {
    let mut changed = subscribe();
    let user = User {
        id: uuid::Uuid::new_v4(),
        name: "Ada".to_string(),
        email: "ada@example.com".to_string(),
        role: "admin".to_string(),
        active: true,
        age: 36,
        created_at: jiff::Timestamp::now(),
    };
    UserResource::after_commit(&Cx::default(), Committed::created(user))
        .await
        .expect("the hook notifies the board");

    tokio::time::timeout(Duration::from_secs(1), changed.recv())
        .await
        .expect("a committed write must wake the feed")
        .expect("the sender outlives the receiver");
}
