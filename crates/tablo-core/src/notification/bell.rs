//! The bell a panel with [`Panel::notifications`](crate::Panel::notifications) shows in its
//! topbar: an unread count, a sheet listing the signed-in user's stored notifications, and the
//! procedures that mark and remove them.
//!
//! The shard asks for a connection, so once the page loads its two live regions keep updating
//! over Topcoat's runtime socket: each waits for a wake on the [`NotificationBus`], checks the
//! session again, and reads back through the [`Inbox`]. A tab with no socket shows what the page
//! loaded with.
//!
//! [`NotificationBus`]: crate::bus::NotificationBus

use jiff::Timestamp;
use tablo_ui::{
    ButtonSize, ButtonVariant, SheetSide, button, dialog_header, dialog_title, empty_state, icons,
    sheet, sheet_content,
};
use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::error::{not_found, unauthorized},
    runtime::{Event, Signal, connected, procedure, shard, signal},
    view::{BoxView, EmitToken, View, ViewExt, attributes, emit, live, view},
};
use uuid::Uuid;

use super::{StoredNotification, stored::Inbox};
use crate::{
    auth::current_realm,
    bus::{Subscription, bus},
};

/// How many notifications the sheet lists at first, and how many more each "Show more" adds.
const PAGE: usize = 20;

/// The most the sheet lists, however often "Show more" is pressed.
const MAX_SHOWN: usize = 200;

/// The id of the sheet's `<dialog>`, which the bell's trigger controls.
const SHEET_ID: &str = "tablo-notifications";

/// The bell: its trigger with the unread count, and the sheet it opens.
///
/// A named path stays stable across builds (topcoat#441), and staying under `/_topcoat/runtime`
/// keeps the panel's auth gate covering it.
#[shard("/_topcoat/runtime/shards/tablo-notifications")]
pub async fn notification_bell(cx: &Cx) -> Result<impl View> {
    render_bell(cx).await
}

async fn render_bell(cx: &Cx) -> Result<BoxView<'_>> {
    // The shard restates the panel gate: Topcoat serves it at its runtime path.
    crate::auth::guard(cx)?;
    let Some(inbox) = bell_inbox(cx) else {
        return Ok(().boxed());
    };
    let open = signal(cx, || false);
    let shown = signal(cx, || PAGE);
    // A tracked read: "Show more" renders the shard again, with more rows.
    let limit = shown.get().clamp(PAGE, MAX_SHOWN);
    // Subscribed before the read, so a send between the read and a region's first wait still
    // wakes it.
    let badge_wakes = bus(cx).subscribe(&inbox.panel, &inbox.user_id);
    let list_wakes = bus(cx).subscribe(&inbox.panel, &inbox.user_id);
    // The layout read the first page before the page's body started streaming; a region's
    // first emission then needs no connection, which that body may still hold.
    let snapshot = if limit == PAGE {
        first_page(cx).await.clone()
    } else {
        Snapshot::read(cx, &inbox, limit).await.ok()
    };
    // The read logged its failure: the page renders without its bell rather than failing.
    let Some(snapshot) = snapshot else {
        return Ok(().boxed());
    };
    Ok(view! {
        cx =>
        <div class="relative">
            button(
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Icon,
                attrs: attributes! {
                    type="button"
                    aria-label="Notifications"
                    aria-haspopup="dialog"
                    aria-controls=(SHEET_ID)
                    @click=$(|_e: Event| open.set(true))
                },
                icon(data: icons::BELL)
            )
            (unread_badge(cx, inbox.clone(), badge_wakes, snapshot.clone()))
        </div>
        sheet(
            open: $(open.get()),
            attrs: attributes! {
                id=(SHEET_ID)
                aria-label="Notifications"
                @keydown=$(|e: Event| {
                    if e.key == "Escape" {
                        open.set(false);
                    }
                })
                @click=$(|e: Event| {
                    if e.target.id == SHEET_ID {
                        open.set(false);
                    }
                })
            },
            sheet_content(
                side: SheetSide::Right,
                attrs: attributes! { class="gap-3" },
                dialog_header(
                    attrs: attributes! { class="flex-row items-center justify-between gap-2" },
                    dialog_title("Notifications")
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        attrs: attributes! {
                            type="button"
                            aria-label="Close notifications"
                            @click=$(|_e: Event| open.set(false))
                        },
                        icon(data: icons::X)
                    )
                )
                (notification_list(cx, inbox, list_wakes, snapshot, limit, shown))
            )
        )
    }
    .boxed())
}

/// The bell at the end of the topbar, when the request's panel shows one to a signed-in user.
pub(crate) fn topbar_bell(cx: &Cx) -> BoxView<'_> {
    if bell_inbox(cx).is_none() {
        return ().boxed();
    }
    view! { cx => notification_bell() }.boxed()
}

/// The signed-in user's inbox when the request's panel shows a bell.
fn bell_inbox(cx: &Cx) -> Option<Inbox> {
    current_realm(cx).filter(|realm| realm.notifications)?;
    Inbox::of(cx)
}

/// What the bell shows of an inbox at one moment.
#[derive(Clone)]
struct Snapshot {
    unread: u64,
    /// Up to the limit, newest first.
    rows: Vec<StoredNotification>,
    /// Whether rows past the limit remain for "Show more".
    more: bool,
}

impl Snapshot {
    async fn read(cx: &Cx, inbox: &Inbox, limit: usize) -> Result<Self> {
        let db = crate::db::db(cx);
        // One row past the limit says whether "Show more" has more to show.
        let mut rows = inbox.latest(&db, limit + 1).await?;
        let more = rows.len() > limit && limit < MAX_SHOWN;
        rows.truncate(limit);
        Ok(Self {
            unread: inbox.unread(&db).await?,
            rows,
            more,
        })
    }
}

/// The bell's first page for the request, read once: the layout calls it before it renders, as it
/// counts the sidebar's badges, and the shard reuses what it read. `None` without a bell, or when
/// the read failed, which logs.
#[topcoat::context::memoize]
async fn first_page(cx: &Cx) -> Option<Snapshot> {
    let inbox = bell_inbox(cx)?;
    Snapshot::read(cx, &inbox, PAGE).await.ok()
}

/// Reads the bell's first page ahead of the page's body, on a panel that shows a bell.
pub(crate) async fn prefetch(cx: &Cx) {
    let _ = first_page(cx).await;
}

/// Waits for the next change to `inbox` and checks the session again; `false` once the
/// connected render must stop, because the session ended or the bus is gone.
async fn next_change(cx: &Cx, wakes: &mut Subscription) -> Result<bool> {
    Ok(wakes.changed().await && crate::auth::still_signed_in(cx).await?)
}

/// The unread count over the trigger, and a toast for each notification that arrives while the
/// page is open.
fn unread_badge(cx: &Cx, inbox: Inbox, mut wakes: Subscription, initial: Snapshot) -> BoxView<'_> {
    live! {
        cx =>
        let db = crate::db::db(cx);
        // When the newest row the page already showed was sent: only a later one toasts, so
        // marking an older one unread does not.
        let mut seen = initial.rows.first().map(|row| row.created_at);
        let mut unread = initial.unread;
        let mut arrival: Option<StoredNotification> = None;
        loop {
            let badge = badge_view(cx, unread, arrival.clone()).await?;
            let token = emit! { (badge) }?;
            if !connected(cx) || !next_change(cx, &mut wakes).await? {
                break Ok::<EmitToken, topcoat::Error>(token);
            }
            unread = inbox.unread(&db).await?;
            let newest = inbox.latest(&db, 1).await?.pop();
            if let Some(newest) = newest.filter(|row| Some(row.created_at) > seen) {
                seen = Some(newest.created_at);
                // One already read elsewhere arrived, but has nothing to say.
                if newest.read_at.is_none() {
                    arrival = Some(newest);
                }
            }
        }
    }
    .boxed()
}

async fn badge_view(
    cx: &Cx,
    unread: u64,
    arrival: Option<StoredNotification>,
) -> Result<BoxView<'_>> {
    let label = match unread {
        0 => String::new(),
        1..=99 => unread.to_string(),
        _ => "99+".to_owned(),
    };
    let toast = match arrival {
        Some(row) => {
            let id = format!("tablo-arrival-{}", row.id);
            super::render_notification(cx, row.notification.0, attributes! { cx => id=(id) })
                .await?
        }
        None => ().boxed(),
    };
    Ok(view! {
        cx =>
        if unread > 0 {
            <span
                class="pointer-events-none absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-medium leading-none text-primary-foreground tabular-nums"
                data-unread=(unread.to_string())
            >
                (label)
                <span class="sr-only">" unread"</span>
            </span>
        }
        tablo_ui::toaster((toast))
    }
    .boxed())
}

/// The sheet's body: the bulk controls, then up to `limit` notifications, newest first.
fn notification_list(
    cx: &Cx,
    inbox: Inbox,
    mut wakes: Subscription,
    initial: Snapshot,
    limit: usize,
    shown: Signal<usize>,
) -> BoxView<'_> {
    live! {
        cx =>
        let mut snapshot = initial;
        loop {
            let Snapshot { unread, rows, more, .. } = snapshot;
            let list = list_view(cx, rows, unread, more, shown.clone()).await?;
            let token = emit! { (list) }?;
            if !connected(cx) || !next_change(cx, &mut wakes).await? {
                break Ok::<EmitToken, topcoat::Error>(token);
            }
            snapshot = Snapshot::read(cx, &inbox, limit).await?;
        }
    }
    .boxed()
}

async fn list_view(
    cx: &Cx,
    rows: Vec<StoredNotification>,
    unread: u64,
    more: bool,
    shown: Signal<usize>,
) -> Result<BoxView<'_>> {
    if rows.is_empty() {
        return Ok(view! {
            cx =>
            empty_state(title: "No notifications", detail: "You're all caught up.")
        }
        .boxed());
    }
    let now = Timestamp::now();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(item_view(cx, row, now).await?);
    }
    Ok(view! {
        cx =>
        <div class="flex items-center justify-between gap-2 text-sm">
            <span class="text-muted-foreground" data-unread-count=(unread.to_string())>
                if unread == 0 {
                    "All read"
                } else {
                    (unread.to_string())
                    " unread"
                }
            </span>
            <div class="flex gap-1">
                if unread > 0 {
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        attrs: attributes! {
                            type="button"
                            data-notifications="read-all"
                            @click=$(async |_e: Event| {
                                mark_all_notifications_read().await;
                            })
                        },
                        icon(data: icons::CHECK_CHECK)
                        "Mark all as read"
                    )
                }
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attributes! {
                        type="button"
                        data-notifications="clear"
                        @click=$(async |_e: Event| {
                            clear_notifications().await;
                        })
                    },
                    icon(data: icons::TRASH)
                    "Clear"
                )
            </div>
        </div>
        <ul class="flex flex-col gap-2" data-notifications="list">
            for item in items {
                (item)
            }
        </ul>
        if more {
            button(
                variant: ButtonVariant::Outline,
                size: ButtonSize::Sm,
                attrs: attributes! {
                    type="button"
                    class="self-center"
                    @click=$(|_e: Event| shown.set(shown.get() + PAGE))
                },
                "Show more"
            )
        }
    }
    .boxed())
}

async fn item_view(cx: &Cx, row: StoredNotification, now: Timestamp) -> Result<BoxView<'_>> {
    let id = row.id.to_string();
    let read = row.read_at.is_some();
    let when = ago(row.created_at, now, &crate::time_zone(cx));
    let stamp = row.created_at.to_string();
    let super::Notification {
        status,
        title,
        description,
        actions,
    } = row.notification.0;
    let (icon_data, icon_class) = super::status_icon(&status);
    let links: Vec<(String, String)> = actions
        .iter()
        .filter_map(|action| Some((action.label.clone(), action.safe_url()?.to_owned())))
        .collect();
    let surface = if read {
        "border-border bg-background"
    } else {
        "border-primary/30 bg-muted/50"
    };
    let toggle_label = if read {
        "Mark as unread"
    } else {
        "Mark as read"
    };
    let toggle_icon = if read { icons::MAIL } else { icons::CHECK };
    let toggle_id = id.clone();
    let remove_id = id.clone();
    Ok(view! {
        cx =>
        <li
            id=(format!("tablo-notification-{id}"))
            class=(format!("flex gap-3 rounded-lg border p-3 text-sm {surface}"))
            data-notification=(id)
            data-read=(if read { "true" } else { "false" })
            data-type=(status.as_str())
        >
            <div class="mt-0.5 shrink-0" aria-hidden="true">
                icon(data: icon_data, attrs: attributes! { class=(icon_class) })
            </div>
            <div class="flex min-w-0 flex-1 flex-col gap-1">
                <p class="font-medium leading-snug [overflow-wrap:anywhere]">(title)</p>
                if let Some(description) = description {
                    <p
                        class="leading-snug text-muted-foreground [overflow-wrap:anywhere]"
                    >
                        (description)
                    </p>
                }
                <time class="text-xs text-muted-foreground" datetime=(stamp)>
                    (when)
                </time>
                if !links.is_empty() {
                    <div class="flex flex-wrap gap-2 pt-1">
                        for (label, url) in links {
                            <a
                                href=(url)
                                class="text-sm font-medium text-primary underline-offset-4 hover:underline"
                            >
                                (label)
                            </a>
                        }
                    </div>
                }
            </div>
            <div class="flex shrink-0 flex-col gap-1">
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    attrs: attributes! {
                        type="button"
                        class="size-7"
                        aria-label=(toggle_label)
                        title=(toggle_label)
                        @click=$(async |_e: Event| {
                            mark_notification(toggle_id, !read).await;
                        })
                    },
                    icon(data: toggle_icon)
                )
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    attrs: attributes! {
                        type="button"
                        class="size-7"
                        aria-label="Remove"
                        title="Remove"
                        @click=$(async |_e: Event| {
                            remove_notification(remove_id).await;
                        })
                    },
                    icon(data: icons::X)
                )
            </div>
        </li>
    }
    .boxed())
}

/// How long ago `then` was, as the sheet prints it: "just now", "5 min ago", "3 h ago", then the
/// date in `zone`.
pub(crate) fn ago(then: Timestamp, now: Timestamp, zone: &jiff::tz::TimeZone) -> String {
    let seconds = now.duration_since(then).as_secs();
    match seconds {
        ..60 => "just now".to_owned(),
        60..3_600 => format!("{} min ago", seconds / 60),
        3_600..86_400 => format!("{} h ago", seconds / 3_600),
        86_400..604_800 => format!("{} d ago", seconds / 86_400),
        _ => then
            .to_zoned(zone.clone())
            .strftime("%b %-d, %Y")
            .to_string(),
    }
}

/// The signed-in user's inbox for a procedure, answering 401 without one and 404 where the
/// request's panel shows no bell.
fn procedure_inbox(cx: &Cx) -> Result<Inbox> {
    crate::auth::guard(cx)?;
    let inbox = Inbox::of(cx).ok_or_else(unauthorized)?;
    current_realm(cx)
        .filter(|realm| realm.notifications)
        .ok_or_else(not_found)?;
    Ok(inbox)
}

/// Wakes the user's bells, this one included, so each reads the change back.
fn changed(cx: &Cx, inbox: &Inbox) {
    bus(cx).wake(&inbox.panel, &inbox.user_id);
}

/// Parses the id a browser sent; one that is no id names no notification.
fn parse_id(id: &str) -> Result<Uuid> {
    Uuid::parse_str(id).map_err(|_| not_found().into())
}

/// Marks one of the user's notifications read, or unread with `read` false. An id the user's
/// inbox does not hold changes nothing.
#[procedure("/_topcoat/runtime/procedures/tablo-notifications/mark")]
pub async fn mark_notification(cx: &Cx, id: String, read: bool) -> Result<()> {
    let inbox = procedure_inbox(cx)?;
    let id = parse_id(&id)?;
    inbox.mark(&crate::db::db(cx), id, read).await?;
    changed(cx, &inbox);
    Ok(())
}

/// Removes one of the user's notifications. An id the user's inbox does not hold changes
/// nothing.
#[procedure("/_topcoat/runtime/procedures/tablo-notifications/remove")]
pub async fn remove_notification(cx: &Cx, id: String) -> Result<()> {
    let inbox = procedure_inbox(cx)?;
    let id = parse_id(&id)?;
    inbox.remove(&crate::db::db(cx), id).await?;
    changed(cx, &inbox);
    Ok(())
}

/// Marks every notification the bell lists read.
#[procedure("/_topcoat/runtime/procedures/tablo-notifications/read-all")]
pub async fn mark_all_notifications_read(cx: &Cx) -> Result<()> {
    let inbox = procedure_inbox(cx)?;
    inbox.mark_all_read(&crate::db::db(cx)).await?;
    changed(cx, &inbox);
    Ok(())
}

/// Removes every notification the bell lists.
#[procedure("/_topcoat/runtime/procedures/tablo-notifications/clear")]
pub async fn clear_notifications(cx: &Cx) -> Result<()> {
    let inbox = procedure_inbox(cx)?;
    inbox.clear(&crate::db::db(cx)).await?;
    changed(cx, &inbox);
    Ok(())
}
