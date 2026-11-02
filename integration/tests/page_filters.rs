//! A page's filters: the bar holding the request's values, the values its widgets read, and the
//! keys the mount refuses.

use tablo::{
    DeclarationErrorKind, Page, PageDef, PageFilter, Stat, Widget,
    auth::{AdminUser, AuthSession},
    page_filters,
    testing::{page_filter_bar, stats},
};
use topcoat::{Result, context::Cx, view::View};

use crate::common::{body_string, get, mount, panel, refusal, test_db};

/// Shows what the request's page filters hold.
struct Held;

impl Widget for Held {
    async fn render(cx: &Cx) -> Result<impl View> {
        let filters = page_filters(cx);
        let (from, to) = filters.date_range("period");
        let held = format!(
            "channel={} from={} to={}",
            filters.choice("channel").unwrap_or("-"),
            from.map(|day| day.to_string()).unwrap_or_default(),
            to.map(|day| day.to_string()).unwrap_or_default(),
        );
        Ok(tablo::stats(cx, vec![Stat::new("Held", held)]))
    }
}

struct Report;

impl Page for Report {
    fn declare() -> PageDef {
        PageDef::new()
            .filter(PageFilter::select(
                "channel",
                "Channel",
                [("web", "Web"), ("store", "Store")],
            ))
            .filter(PageFilter::date_range("period", "Period"))
            .widget::<Held>()
    }
}

/// What the page's widget read.
fn held(html: &str) -> String {
    stats(html)
        .into_iter()
        .find(|stat| stat.label == "Held")
        .expect("the widget renders")
        .value
}

fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

#[tokio::test]
async fn a_widget_reads_the_values_the_page_filters_hold() {
    let db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let router = mount(db, panel().page::<Report>()).expect("the panel mounts");
    let page = async |uri: &str| body_string(get(&router, uri).await).await;

    let html =
        page("/admin/report?pf.channel=web&pf.period.from=2026-01-01&pf.period.to=2026-01-31")
            .await;
    assert_eq!(held(&html), "channel=web from=2026-01-01 to=2026-01-31");
    let bar = page_filter_bar(&html).expect("the page renders its bar");
    assert_eq!(
        bar.submits,
        pairs(&[
            ("pf.channel", "web"),
            ("pf.period.from", "2026-01-01"),
            ("pf.period.to", "2026-01-31"),
        ])
    );
    assert_eq!(bar.reset.as_deref(), Some("/admin/report"));

    // A value no option offers, and a day that does not parse, read as none.
    let html = page("/admin/report?pf.channel=phone&pf.period.from=soon").await;
    assert_eq!(held(&html), "channel=- from= to=");
    assert_eq!(page_filter_bar(&html).expect("the bar renders").reset, None);
}

/// A table's `f.` filter is no page filter, and the bar and its reset link carry it with the
/// request's other parameters, so applying the bar keeps what a table on the page shows.
#[tokio::test]
async fn the_bar_keeps_the_request_s_other_parameters() {
    let db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let router = mount(db, panel().page::<Report>()).expect("the panel mounts");
    let html = body_string(
        get(
            &router,
            "/admin/report?f.channel=web&q=ada&pf.channel=store",
        )
        .await,
    )
    .await;
    assert_eq!(held(&html), "channel=store from= to=");
    let bar = page_filter_bar(&html).expect("the page renders its bar");
    assert_eq!(
        bar.submits,
        pairs(&[
            ("f.channel", "web"),
            ("q", "ada"),
            ("pf.channel", "store"),
            ("pf.period.from", ""),
            ("pf.period.to", ""),
        ])
    );
    assert_eq!(
        bar.reset.as_deref(),
        Some("/admin/report?f.channel=web&q=ada")
    );
}

#[tokio::test]
async fn two_filters_sharing_a_key_or_a_dotted_key_fail_the_mount() {
    struct Misdeclared;

    impl Page for Misdeclared {
        fn declare() -> PageDef {
            PageDef::new()
                .filter(PageFilter::select("channel", "Channel", ["web"]))
                .filter(PageFilter::date_range("channel", "Channel"))
                .filter(PageFilter::date_range("period.from", "Period"))
        }
    }

    let db = test_db(toasty::models!(AdminUser, AuthSession)).await;
    let errors = refusal(mount(db, panel().page::<Misdeclared>()));
    let keys: Vec<_> = errors
        .into_iter()
        .filter_map(|error| match error.kind {
            DeclarationErrorKind::MisdeclaredPageFilter { key } => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["channel", "period.from"]);
}
