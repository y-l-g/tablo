//! The panel's global search: the box in the topbar, which lists the first results of every
//! searched resource as the user types, and the page at `{prefix}/-/search?q=` that lists more,
//! which Enter in the box opens.
//!
//! The box's results are a shard: once the page loads, each pause in typing renders them again for
//! the new term.

use futures_util::future::try_join_all;
use tablo_ui::{empty_state, input as ui_input, page, page_header, page_title};
use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    router::{Body, Route},
    runtime::{Event, shard, signal},
    view::{BoxView, View, ViewExt, attributes, view},
};

use super::state::{NavEntry, Searchable, current};
use crate::{
    navigation::{MENU_LINK_CLASS, runtime_link},
    panel::url::{row_edit_url, row_view_url},
    resource::{Opens, SearchHit},
    schema::clamp_query_term,
    topcoat_compat::{async_page, href::encode_query_value},
};

/// The path of the results page under a panel's prefix.
pub(crate) const SEARCH_SEGMENT: &str = "-/search";

/// The path of the results' shard, which every panel with a global search declares as its
/// [endpoint](super::Panel::endpoint), so the results answer for a panel without sign-in too.
pub(super) fn shard_path() -> String {
    search_results.path().as_str().to_owned()
}

/// How many results of each resource the box lists.
const BOX_LIMIT: usize = 5;

/// How many results of each resource the page lists.
const PAGE_LIMIT: usize = 25;

/// How many words of a term the search matches; the rest are ignored, which bounds the query.
const MAX_WORDS: usize = 8;

/// The id of the box's form, which its field submits after a pause.
const FORM_ID: &str = "tablo-search";

/// The magnifier inside the field's leading padding.
const ICON_CLASS: &str =
    "pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground";

/// One resource's results, headed by its plural label.
struct Group {
    label: String,
    hits: Vec<Hit>,
}

/// One result as the panel links it.
struct Hit {
    title: String,
    url: String,
    details: Vec<(String, String)>,
}

impl Hit {
    /// `hit` of the resource whose list is at `list`.
    fn of(hit: SearchHit, list: &str) -> Self {
        let url = match hit.opens {
            Opens::Detail => row_view_url(list, &hit.key),
            Opens::Edit => row_edit_url(list, &hit.key),
        };
        Self {
            title: hit.title,
            url,
            details: hit.details,
        }
    }
}

/// `searches` in the order the sidebar lists their resources, with the ungrouped entries first;
/// a resource the sidebar leaves out comes last.
pub(super) fn in_sidebar_order(mut searches: Vec<Searchable>, nav: &[NavEntry]) -> Vec<Searchable> {
    let items: Vec<_> = nav.iter().map(|entry| entry.item.clone()).collect();
    let paths: Vec<String> =
        super::shell::sidebar::sidebar_sections(super::shell::sidebar::in_sidebar_order(&items))
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .filter_map(|(_, item)| item.path().map(str::to_string))
            .collect();
    searches.sort_by_key(|search| {
        paths
            .iter()
            .position(|path| *path == search.list)
            .unwrap_or(usize::MAX)
    });
    searches
}

/// Whether the request's panel shows the box: it searches a resource the request may list.
fn searches_any(cx: &Cx) -> bool {
    current(cx).is_some_and(|panel| panel.searches.iter().any(|search| (search.visible)(cx)))
}

/// The distinct words of `term`, at most [`MAX_WORDS`].
fn words(term: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for word in clamp_query_term(term).split_whitespace() {
        if words.len() == MAX_WORDS {
            break;
        }
        if !words.iter().any(|seen| seen == word) {
            words.push(word.to_string());
        }
    }
    words
}

/// Up to `limit` results of each resource the request's panel searches for `term`, in the
/// panel's order; a resource with none is left out.
async fn run(cx: &Cx, term: &str, limit: usize) -> Result<Vec<Group>> {
    let Some(panel) = current(cx) else {
        return Ok(Vec::new());
    };
    let words = words(term);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let found = try_join_all(
        panel
            .searches
            .iter()
            .map(|search| (search.search)(cx, &words, limit)),
    )
    .await?;
    Ok(panel
        .searches
        .iter()
        .zip(found)
        .filter(|(_, hits)| !hits.is_empty())
        .map(|(search, hits)| Group {
            label: search.label.clone(),
            hits: hits
                .into_iter()
                .map(|hit| Hit::of(hit, &search.list))
                .collect(),
        })
        .collect())
}

/// The end of the topbar: the search box, then the notification bell.
pub(super) fn topbar_end(cx: &Cx) -> BoxView<'_> {
    view! {
        cx =>
        <div class="ml-auto flex items-center gap-2">
            (topbar_search(cx))
            (crate::notification::topbar_bell(cx))
        </div>
    }
    .boxed()
}

/// The box at the end of the topbar, when the request's panel shows one: a GET form to the
/// results page, whose field writes the term to a signal once the user pauses, and the
/// [`search_results`] shard over that term. Enter opens the results page, with or without
/// JavaScript.
fn topbar_search(cx: &Cx) -> BoxView<'_> {
    let Some(panel) = current(cx).filter(|_| searches_any(cx)) else {
        return ().boxed();
    };
    let prefix = panel.realm.prefix.clone();
    let action = format!("{prefix}/{SEARCH_SEGMENT}");
    let term = signal(&cx.keyed("tablo-search"), String::new);
    // Each handler captures its own copy of the form's id.
    let (form, field_form, enter_form) = (
        FORM_ID.to_string(),
        FORM_ID.to_string(),
        FORM_ID.to_string(),
    );
    view! {
        cx =>
        <div class="group/search relative w-40 sm:w-64" data-global-search="">
            // A pause in typing submits the form, which writes the term to the signal; Enter
            // submits it natively instead, which skips the handler and opens the results page.
            <form
                id=(FORM_ID)
                method="get"
                action=(action)
                role="search"
                @submit=$(|e: Event| {
                    e.prevent_default();
                    term.set(
                        raw!(
                            "cx.hydrate(String(new FormData(document.getElementById(String(${form}))).get('q') ?? ''))",
                            form.clone(),
                        ),
                    );
                })
            >
                <div class="relative">
                    icon(
                        data: tablo_ui::icons::SEARCH,
                        attrs: attributes! { class=(ICON_CLASS) }
                    )
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            name="q"
                            :value=$(term.get())
                            placeholder="Search…"
                            aria-label="Search"
                            autocomplete="off"
                            class="pl-8"
                            @input=$(|_e: Event| raw!(
                                    "((f) => { clearTimeout(f.tabloSearch); f.tabloSearch = setTimeout(() => f.requestSubmit(), 200) })(document.getElementById(String(${field_form})))",
                                    (),
                                ))
                            @keydown=$(|e: Event| {
                                if e.key == "Enter" {
                                    e.prevent_default();
                                    raw!(
                                        "document.getElementById(String(${enter_form})).submit()",
                                        (),
                                    )
                                } else if e.key == "Escape" {
                                    // Escape in a search field also clears it, which would
                                    // drop the term the box shows again on focus.
                                    e.prevent_default();
                                    raw!("document.activeElement?.blur()", ())
                                }
                            })
                        }
                    )
                </div>
            </form>
            search_results(prefix: prefix, term: $(term.get()))
        </div>
    }
    .boxed()
}

/// The first results of every resource the panel at `prefix` searches for `term`, and a link to
/// the results page; nothing for a blank term.
///
/// A named path stays stable across builds (topcoat#441), and staying under `/_topcoat/runtime`
/// keeps the panel's auth gate covering it.
#[shard("/_topcoat/runtime/shards/tablo-search")]
pub(crate) async fn search_results(cx: &Cx, prefix: String, term: String) -> Result<impl View> {
    render_results(cx, prefix, term).await
}

/// The results under the box: up to [`BOX_LIMIT`] of each resource for `term`, shown while focus
/// is in the box, and a link to the results page; nothing for a blank term.
///
/// Topcoat serves the shard at its runtime path, where the request runs as the panel that signed
/// its session in, or as the first declaring panel without sign-in. The shard lists results only
/// when that is the panel at `prefix`, whose page rendered it. Otherwise, as on the second of two
/// panels without sign-in, it lists none and links the results page alone, which the panel at
/// `prefix` serves.
async fn render_results(cx: &Cx, prefix: String, term: String) -> Result<BoxView<'_>> {
    crate::auth::guard(cx)?;
    let term = clamp_query_term(&term);
    if words(&term).is_empty() {
        return Ok(().boxed());
    }
    let here = current(cx).is_some_and(|panel| panel.realm.prefix == prefix) && searches_any(cx);
    let groups = if here {
        Some(run(cx, &term, BOX_LIMIT).await?)
    } else {
        None
    };
    let all = runtime_link(
        cx,
        &format!("{prefix}/{SEARCH_SEGMENT}?q={}", encode_query_value(&term)),
    );
    let found = groups.as_ref().is_some_and(|groups| !groups.is_empty());
    let listed = groups.is_some();
    let results = groups_view(cx, groups.unwrap_or_default());
    Ok(view! {
        cx =>
        // Shown while focus is in the box: a click inside it focuses the panel, which keeps it
        // open until the link it hit navigates.
        <div
            tabindex="-1"
            class="absolute right-0 top-full z-50 mt-1 hidden max-h-[70vh] w-[min(24rem,calc(100vw-2rem))] overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md outline-none group-focus-within/search:block"
            data-search-results=""
        >
            if found {
                (results)
            } else if listed {
                <p class="px-2 py-6 text-center text-sm text-muted-foreground">
                    "No results"
                </p>
            }
            <a (all) class=(MENU_LINK_CLASS) data-search-all="">
                icon(data: tablo_ui::icons::SEARCH)
                "See all results"
            </a>
        </div>
    }
    .boxed())
}

/// `GET {prefix}/-/search?q=`: up to [`PAGE_LIMIT`] results of each resource for the `q` term.
pub(crate) fn search_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let query = crate::table::request_query(cx);
        let raw = form_urlencoded::parse(query.as_bytes())
            .find(|(name, _)| name == "q")
            .map(|(_, value)| clamp_query_term(&value))
            .unwrap_or_default();
        let groups = run(cx, &raw, PAGE_LIMIT).await?;
        let searched = !words(&raw).is_empty();
        let found = !groups.is_empty();
        let detail = format!("Nothing matches “{raw}”.");
        let results = groups_view(cx, groups);
        let path = topcoat::router::request::uri(cx).path().to_string();
        Ok(view! {
            cx =>
            page(
                page_header(page_title("Search"))
                <form
                    method="get"
                    action=(path)
                    role="search"
                    class="relative w-full sm:w-96"
                >
                    icon(
                        data: tablo_ui::icons::SEARCH,
                        attrs: attributes! { class=(ICON_CLASS) }
                    )
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            name="q"
                            value=(raw)
                            placeholder="Search…"
                            aria-label="Search"
                            class="pl-8"
                        }
                    )
                </form>
                if found {
                    <div class="max-w-2xl rounded-xl border bg-card p-1 shadow-xs">
                        (results)
                    </div>
                } else if searched {
                    empty_state(title: "No results", detail: detail)
                }
            )
        })
    })
}

/// Each group under its heading, its results as links.
fn groups_view(cx: &Cx, groups: Vec<Group>) -> BoxView<'_> {
    let sections: Vec<BoxView<'_>> = groups
        .into_iter()
        .map(|group| {
            let hits: Vec<BoxView<'_>> = group
                .hits
                .into_iter()
                .map(|hit| hit_view(cx, hit))
                .collect();
            view! {
                cx =>
                <section class="py-1" data-search-group=(group.label.clone())>
                    <h2 class="px-2 py-1.5 text-xs font-medium text-muted-foreground">
                        (group.label)
                    </h2>
                    for hit in hits {
                        (hit)
                    }
                </section>
            }
            .boxed()
        })
        .collect();
    view! {
        cx =>
        for section in sections {
            (section)
        }
    }
    .boxed()
}

/// One result: its title over its details, linking the page it opens.
fn hit_view(cx: &Cx, hit: Hit) -> BoxView<'_> {
    let link = runtime_link(cx, &hit.url);
    view! {
        cx =>
        <a
            (link)
            class="flex flex-col gap-0.5 rounded-md px-2 py-1.5 text-sm outline-none hover:bg-foreground/5 focus-visible:bg-foreground/5"
            data-search-hit=""
        >
            <span class="truncate font-medium text-foreground">(hit.title)</span>
            if !hit.details.is_empty() {
                <dl class="flex flex-wrap gap-x-3 text-xs text-muted-foreground">
                    for (label, value) in hit.details {
                        <div class="flex min-w-0 gap-1">
                            <dt>
                                (label)
                                ":"
                            </dt>
                            <dd class="truncate">(value)</dd>
                        </div>
                    }
                </dl>
            }
        </a>
    }
    .boxed()
}

#[cfg(test)]
mod tests;
