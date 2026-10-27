//! Reads a page's widget grid, its stat tiles, its charts and its record tables: `[data-widget]`
//! names each widget the grid renders, `[data-stat]` marks each tile, whose first two paragraphs
//! hold its label and value, `[data-chart]` marks each chart, whose `[data-chart-data]` table
//! holds its values, and `[data-record-table]` marks each record table by its resource's slug.

use super::dom::{Row, attr_value, columns, elements, rows, strip_text, tags_where};

/// The names of the widgets a page's grid renders, in order: `RecentOrdersWidget` is
/// `recent-orders`.
pub fn widgets(html: &str) -> Vec<String> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-widget").is_some())
        .into_iter()
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-widget"))
        .collect()
}

/// One stat tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatTile {
    /// What the figure counts.
    pub label: String,
    /// The figure.
    pub value: String,
    /// Where the tile links, if it does.
    pub href: Option<String>,
}

/// The stat tiles `html` renders, in document order.
pub fn stats(html: &str) -> Vec<StatTile> {
    tags_where(html, |_, attrs| {
        attrs.iter().any(|(name, _)| name == "data-stat")
    })
    .into_iter()
    .map(|(tag, attrs, inner)| {
        let mut paragraphs = elements(&inner, "p")
            .into_iter()
            .map(|(_, _, p)| strip_text(&p));
        StatTile {
            label: paragraphs.next().unwrap_or_default(),
            value: paragraphs.next().unwrap_or_default(),
            href: (tag == "a").then(|| attr_value(&attrs, "href")).flatten(),
        }
    })
    .collect()
}

/// One chart, as its data table reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartData {
    /// How it draws: `line` or `bar`.
    pub kind: String,
    /// Its heading, if it has one.
    pub heading: Option<String>,
    /// The labels along its bottom.
    pub labels: Vec<String>,
    /// Its series, in the order the chart draws them.
    pub series: Vec<ChartSeries>,
}

/// One series of a [`ChartData`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartSeries {
    /// What the series measures.
    pub label: String,
    /// A value per label of the chart, as the chart spells it: empty where it has none.
    pub values: Vec<String>,
}

/// The charts `html` renders, in document order.
pub fn charts(html: &str) -> Vec<ChartData> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-chart").is_some())
        .into_iter()
        .map(|(_, attrs, inner)| {
            let table = tags_where(&inner, |tag, attrs| {
                tag == "table" && attrs.iter().any(|(name, _)| name == "data-chart-data")
            })
            .into_iter()
            .next()
            .map(|(_, _, table)| table)
            .unwrap_or_default();
            let text = |element: &str| -> Vec<String> {
                elements(&table, element)
                    .into_iter()
                    .map(|(_, _, inner)| strip_text(&inner))
                    .collect()
            };
            let head = elements(&table, "thead")
                .into_iter()
                .next()
                .map(|(_, _, head)| head)
                .unwrap_or_default();
            let series = elements(&table, "tbody")
                .into_iter()
                .next()
                .map(|(_, _, body)| elements(&body, "tr"))
                .unwrap_or_default()
                .into_iter()
                .map(|(_, _, row)| ChartSeries {
                    label: elements(&row, "th")
                        .into_iter()
                        .next()
                        .map(|(_, _, th)| strip_text(&th))
                        .unwrap_or_default(),
                    values: elements(&row, "td")
                        .into_iter()
                        .map(|(_, _, td)| strip_text(&td))
                        .collect(),
                })
                .collect();
            ChartData {
                kind: attr_value(&attrs, "data-chart").unwrap_or_default(),
                heading: text("caption").into_iter().next(),
                labels: elements(&head, "th")
                    .into_iter()
                    .map(|(_, _, th)| strip_text(&th))
                    .collect(),
                series,
            }
        })
        .collect()
}

/// One record table, as its markup reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordTableData {
    /// The slug of the resource it lists.
    pub resource: String,
    /// Its heading.
    pub heading: String,
    /// The names of the columns it shows, in order.
    pub columns: Vec<String>,
    /// Its rows, in order, as [`rows`](super::rows) reads a list's.
    pub rows: Vec<Row>,
    /// Where its link to the full list points.
    pub list: Option<String>,
}

/// The record tables `html` renders, in document order.
pub fn record_tables(html: &str) -> Vec<RecordTableData> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-record-table").is_some()
    })
    .into_iter()
    .map(|(_, attrs, inner)| RecordTableData {
        resource: attr_value(&attrs, "data-record-table").unwrap_or_default(),
        heading: elements(&inner, "h2")
            .into_iter()
            .next()
            .map(|(_, _, h2)| strip_text(&h2))
            .unwrap_or_default(),
        columns: columns(&inner),
        rows: rows(&inner),
        list: tags_where(&inner, |tag, attrs| {
            tag == "a"
                && attrs
                    .iter()
                    .any(|(name, _)| name == "data-record-table-list")
        })
        .into_iter()
        .next()
        .and_then(|(_, attrs, _)| attr_value(&attrs, "href")),
    })
    .collect()
}
