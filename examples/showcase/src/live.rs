//! The live feed: a Topcoat shard whose WebSocket connection keeps every open
//! page current.

use std::sync::LazyLock;

use tablo_core::{NavigationItem, Page, Resource, db::db};
use topcoat::{
    Result,
    context::Cx,
    runtime::{connected, shard},
    view::{View, emit, live, view},
};

use crate::{app::UserResource, models::User};

/// Where the live feed lives.
pub const LIVE_PATH: &str = "/admin/live";

/// How many of the newest users the feed shows.
const FEED_ROWS: usize = 20;

/// The process-wide wake channel behind every open feed.
static BOARD: LazyLock<Board> = LazyLock::new(Board::default);

/// Wakes every connected feed after a committed write.
struct Board {
    changed: tokio::sync::broadcast::Sender<()>,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            changed: tokio::sync::broadcast::channel(16).0,
        }
    }
}

impl Board {
    fn notify(&self) {
        let _ = self.changed.send(());
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}

/// Wakes every connected feed the process holds.
pub(crate) fn notify() {
    BOARD.notify();
}

fn subscribe() -> tokio::sync::broadcast::Receiver<()> {
    BOARD.subscribe()
}

/// The panel page holding the live feed.
pub struct LiveActivityPage;

impl Page for LiveActivityPage {
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>().icon(tablo_ui::icons::ACTIVITY)
    }

    fn slug() -> String {
        "live".to_string()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! {
            cx =>
            tablo_ui::page(
                tablo_ui::page_header(
                    tablo_ui::page_title("Live activity")
                    tablo_ui::page_description(
                        "The newest users, re-read after every committed write and pushed over a WebSocket."
                    )
                )
                tablo_ui::page_content(
                    tablo_ui::card(
                        tablo_ui::card_header(tablo_ui::card_title("Newest users"))
                        tablo_ui::card_content(live_feed())
                    )
                )
            )
        })
    }
}

/// Streams the newest users to every open page.
#[shard]
async fn live_feed(cx: &Cx) -> Result<impl View> {
    // The shard runs the panel's guard itself.
    tablo_core::auth::guard(cx)?;
    Ok(live! {
        let mut changed = subscribe();
        loop {
            let users = newest_users(cx).await?;
            let empty = users.is_empty();
            let token = emit! {
                if empty {
                    <p class="text-sm text-muted-foreground" data-live-feed="">
                        "No users yet."
                    </p>
                } else {
                    <ul class="flex flex-col divide-y divide-border" data-live-feed="">
                        for user in users {
                            <li
                                class="flex items-center justify-between gap-4 py-2.5 text-sm"
                            >
                                <span class="font-medium text-foreground">
                                    (user.name)
                                </span>
                                <span class="truncate text-muted-foreground">
                                    (user.email)
                                </span>
                            </li>
                        }
                    </ul>
                }
            }?;
            if !connected(cx) {
                break Ok(token);
            }
            match changed.recv().await {
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break Ok(token),
                Ok(()) => {}
            }
        }
    })
}

/// The newest users the panel serves, newest first.
async fn newest_users(cx: &Cx) -> Result<Vec<User>> {
    let mut db = db(cx);
    let users = UserResource::query(cx)
        .order_by(User::fields().created_at().desc())
        .limit(FEED_ROWS)
        .exec(&mut db)
        .await?;
    Ok(users)
}

#[cfg(test)]
mod tests;
