//! The in-process channel that wakes a panel user's open bells when their stored notifications
//! change: it carries whose, never what, so a bell reads the change back through its own scoped
//! query.

use std::sync::Arc;

use tokio::sync::broadcast;

/// How many wakes a slow bell may fall behind by before it skips to the latest; a bell that
/// lags re-reads once, which is all a wake asks of it.
const CAPACITY: usize = 256;

/// Whose stored notifications changed: one panel user on one panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Wake {
    pub(crate) panel: String,
    pub(crate) user_id: String,
}

/// The router's channel of [`Wake`]s, shared by every panel it mounts. Cloning it is cheap.
#[derive(Clone)]
pub(crate) struct NotificationBus(broadcast::Sender<Arc<Wake>>);

impl NotificationBus {
    pub(crate) fn new() -> Self {
        Self(broadcast::channel(CAPACITY).0)
    }

    /// Wakes every bell `user_id` has open on `panel`.
    pub(crate) fn wake(&self, panel: &str, user_id: &str) {
        // No open bell is no one to wake.
        let _ = self.0.send(Arc::new(Wake {
            panel: panel.to_owned(),
            user_id: user_id.to_owned(),
        }));
    }

    /// A receiver for the wakes of `user_id` on `panel`. Subscribe before the read the wake
    /// refreshes, so a change between the read and the wait is not missed.
    pub(crate) fn subscribe(&self, panel: &str, user_id: &str) -> Subscription {
        Subscription {
            receiver: self.0.subscribe(),
            wake: Wake {
                panel: panel.to_owned(),
                user_id: user_id.to_owned(),
            },
        }
    }
}

/// One bell's wait for its user's wakes.
pub(crate) struct Subscription {
    receiver: broadcast::Receiver<Arc<Wake>>,
    wake: Wake,
}

impl Subscription {
    /// Waits for the next wake of this subscription's user. A bell that fell behind wakes at
    /// once; `false` means the bus is gone and no wake will come.
    pub(crate) async fn changed(&mut self) -> bool {
        loop {
            match self.receiver.recv().await {
                Ok(wake) if *wake == self.wake => return true,
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => return true,
                Err(broadcast::error::RecvError::Closed) => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wake reaches its own user's bell on its own panel, and no one else's.
    #[tokio::test]
    async fn a_wake_reaches_only_its_users_bell_on_its_panel() {
        let bus = NotificationBus::new();
        let mut mine = bus.subscribe("/admin", "1");
        bus.wake("/admin", "1");
        assert!(mine.changed().await, "the user's own wake reaches the bell");
        bus.wake("/admin", "2");
        bus.wake("/portal", "1");
        drop(bus);
        assert!(
            !mine.changed().await,
            "another user's wake, or the user's on another panel, does not"
        );
    }
}
