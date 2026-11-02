//! What a request's query string holds for a page's filters.

use jiff::civil::date;

use super::{PageFilter, read};

fn filters() -> Vec<PageFilter> {
    vec![
        PageFilter::select("channel", "Channel", ["web", "store"]),
        PageFilter::date_range("period", "Period"),
    ]
}

#[test]
fn a_select_holds_only_one_of_its_options() {
    assert_eq!(
        read(&filters(), "f.channel=web").choice("channel"),
        Some("web")
    );
    assert_eq!(read(&filters(), "f.channel=phone").choice("channel"), None);
    assert_eq!(read(&filters(), "f.channel=").choice("channel"), None);
    assert!(read(&filters(), "f.other=web").is_empty());
}

#[test]
fn a_date_range_holds_each_day_that_parses() {
    let held = read(
        &filters(),
        "f.period.from=2026-01-01&f.period.to=2026-01-31",
    );
    assert_eq!(
        held.date_range("period"),
        (Some(date(2026, 1, 1)), Some(date(2026, 1, 31)))
    );
    let open = read(&filters(), "f.period.from=2026-01-01&f.period.to=soon");
    assert_eq!(open.date_range("period"), (Some(date(2026, 1, 1)), None));
    assert!(read(&filters(), "f.period.to=soon").is_empty());
    assert_eq!(read(&filters(), "").date_range("period"), (None, None));
}

#[test]
fn a_key_is_a_query_word_without_a_dot() {
    assert!(PageFilter::select("by-channel_2", "Channel", ["web"]).has_valid_key());
    for key in ["", "period.from", "a b", "é"] {
        assert!(
            !PageFilter::date_range(key, "Period").has_valid_key(),
            "{key:?}"
        );
    }
}
