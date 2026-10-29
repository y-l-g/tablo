//! The tab bar above the toolbar: a link per [`ListTab`](crate::ListTab), the active one marked
//! `aria-current`, each badged tab with its count.

use tablo_ui::{BadgeVariant, badge};
use topcoat::{context::Cx, view::*};

use super::{super::TableSignals, live_link};
use crate::{t, table::TableState};

/// One tab, projected for rendering.
pub(super) struct TabView {
    key: String,
    label: String,
    /// Whether the list shows this tab.
    active: bool,
    /// The rows it holds, when it shows a badge.
    count: Option<u64>,
}

impl TabView {
    /// The tab `key`, read `label`, holding `count` rows when badged.
    pub(super) fn new(key: &str, label: String, active: bool, count: Option<u64>) -> Self {
        Self {
            key: key.to_string(),
            label,
            active,
            count,
        }
    }
}

/// Render the tab bar over `tabs`, the first the one a list without `?tab=` shows, each linking
/// the first page of its rows under `state`'s search, filters and sort. `None` without a tab.
pub(super) fn render_tabs<'a>(
    cx: &'a Cx,
    tabs: Vec<TabView>,
    state: &TableState,
    path: &str,
    signals: &TableSignals,
) -> Option<BoxView<'a>> {
    if tabs.is_empty() {
        return None;
    }
    let links: Vec<_> = tabs
        .into_iter()
        .enumerate()
        .map(|(i, tab)| {
            // The first tab spells no parameter, so its link is the list's canonical one.
            let url = state.with_tab(path, (i > 0).then_some(tab.key.as_str()));
            let link = live_link(cx, url, signals);
            (tab, link)
        })
        .collect();
    Some(
        view! {
            cx =>
            <nav
                aria-label=(t!(cx, "tablo-list-tabs"))
                class="overflow-x-auto border-b border-border px-4 sm:px-6"
            >
                <div class="-mb-px flex gap-4">
                    for (tab, link) in links {
                        <a
                            data-list-tab=(tab.key)
                            aria-current=(tab.active.then_some("page"))
                            class=(class!(
                                TAB_CLASS,
                                if tab.active { TAB_ACTIVE_CLASS } else { TAB_IDLE_CLASS },
                            ))
                            (link)
                        >
                            (tab.label)
                            if let Some(count) = tab.count {
                                badge(
                                    variant: BadgeVariant::Secondary,
                                    attrs: attributes! {
                                        data-list-tab-count=(count.to_string())
                                        class="tabular-nums"
                                    },
                                    (count.to_string())
                                )
                            }
                        </a>
                    }
                </div>
            </nav>
        }
        .boxed(),
    )
}

/// A tab's link: its label over the bar's bottom border, which the active tab's underline covers.
pub(crate) const TAB_CLASS: StaticClass = class!(
    "inline-flex shrink-0 items-center gap-2 border-b-2 py-3 text-sm font-medium",
    "whitespace-nowrap outline-none transition-colors focus-visible:text-foreground",
);

/// The tab the list shows.
pub(crate) const TAB_ACTIVE_CLASS: StaticClass = class!("border-primary text-foreground");

/// A tab the reader may switch to.
pub(crate) const TAB_IDLE_CLASS: StaticClass =
    class!("border-transparent text-muted-foreground hover:text-foreground");
