#[test]
fn the_window_keeps_the_ends_and_the_neighbors() {
    use super::page_window;
    let pages = |page, last| {
        page_window(page, last)
            .into_iter()
            .map(|entry| entry.map_or_else(|| "…".to_string(), |n| n.to_string()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(pages(1, 1), "1");
    assert_eq!(pages(1, 3), "1 2 3");
    assert_eq!(pages(1, 10), "1 2 … 10");
    // A gap of one page shows the page rather than an ellipsis.
    assert_eq!(pages(4, 10), "1 2 3 4 5 … 10");
    assert_eq!(pages(6, 12), "1 … 5 6 7 … 12");
    assert_eq!(pages(12, 12), "1 … 11 12");
}
