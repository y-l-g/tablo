use super::*;
use crate::{ComputedColumn, lens, test_support::User};

/// `%` and `_` in a search term are literal characters, not
/// wildcards, and the term is wrapped for a substring match.
#[test]
fn search_pattern_escapes_like_metacharacters() {
    assert_eq!(escape_like_pattern("Ada"), "%Ada%");
    assert_eq!(escape_like_pattern("100%"), "%100\\%%");
    assert_eq!(escape_like_pattern("a_b"), "%a\\_b%");
    assert_eq!(escape_like_pattern("back\\slash"), "%back\\\\slash%");
}

#[test]
fn text_column_renders_cells_via_typed_projection() {
    let plain = TextColumn::new(lens!(User.name));
    let decorated = TextColumn::new(lens!(User.name)).format(|name| format!("{name}!"));
    let row = User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    };
    assert_eq!(plain.text(&crate::test_support::cx(), &row), "Ada");
    assert_eq!(decorated.text(&crate::test_support::cx(), &row), "Ada!");
    assert_eq!(plain.name(), "name");
    assert_eq!(Column::label(&plain), "Name");
    let relabelled = TextColumn::new(lens!(User.name)).label("Full name");
    assert_eq!(Column::label(&relabelled), "Full name");
    assert_eq!(relabelled.name(), "name", "a label renames no column");
}

/// A column's includes accumulate across calls and keep each relation once, so
/// the table's union loads every relation the columns read, once.
#[test]
fn text_column_includes_accumulate_once_per_relation() {
    #[derive(Debug, toasty::Model, crate::ModelColumns, Clone)]
    struct Owner {
        #[key]
        #[auto]
        id: uuid::Uuid,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Pet {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[index]
        owner_id: uuid::Uuid,
        #[belongs_to(key = owner_id, references = id)]
        owner: toasty::Deferred<Owner>,
        #[index]
        vet_id: uuid::Uuid,
        #[belongs_to(key = vet_id, references = id)]
        vet: toasty::Deferred<Owner>,
    }

    let column = ComputedColumn::new("Owner", |p: &Pet| p.id.to_string())
        .include(Pet::fields().owner())
        .include(Pet::fields().vet())
        .include(Pet::fields().owner());
    assert_eq!(
        column.includes().len(),
        2,
        "two relations, one repeated: the column keeps each once"
    );
}

/// Every date type spells its calendar day, a null spells empty, and `TextColumn` still spells
/// the stored value.
#[test]
fn a_date_column_spells_the_day_of_every_date_type() {
    #[derive(Debug, Clone, toasty::Model)]
    struct Event {
        #[key]
        #[auto]
        id: uuid::Uuid,
        day: jiff::civil::Date,
        at: jiff::civil::DateTime,
        seen: jiff::Timestamp,
        closed: Option<jiff::civil::Date>,
    }

    let cx = crate::test_support::cx();
    let row = Event {
        id: uuid::Uuid::nil(),
        day: jiff::civil::date(2026, 10, 14),
        at: jiff::civil::date(2026, 10, 14).at(9, 30, 0, 0),
        // Late in the UTC day, so a local-time spelling moves it.
        seen: "2026-10-14T23:30:00Z".parse().expect("a timestamp"),
        closed: None,
    };
    let text = |column: &dyn Column<Event>| column.text(&cx, &row);
    assert_eq!(text(&DateColumn::new(lens!(Event.day))), "Oct 14, 2026");
    assert_eq!(text(&DateColumn::new(lens!(Event.at))), "Oct 14, 2026");
    assert_eq!(text(&DateColumn::new(lens!(Event.seen))), "Oct 14, 2026");
    assert_eq!(text(&DateColumn::new(lens!(Event.closed))), "");
    assert_eq!(text(&TextColumn::new(lens!(Event.day))), "2026-10-14");
    assert_eq!(
        Column::column_width(&DateColumn::new(lens!(Event.day))),
        ColumnWidth::Narrow
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, crate::Options)]
enum Stage {
    #[option(color = "success")]
    Paid,
    Open,
}

#[derive(Debug, Clone, toasty::Model)]
struct Invoice {
    #[key]
    #[auto]
    id: uuid::Uuid,
    number: String,
    customer: String,
    stage: Stage,
    total: i64,
    color: String,
    logo: String,
}

fn invoice() -> Invoice {
    Invoice {
        id: uuid::Uuid::nil(),
        number: "INV-7".to_string(),
        customer: "Ada".to_string(),
        stage: Stage::Paid,
        total: 123_456,
        color: "#3b82f6".to_string(),
        logo: "/uploads/logo.png".to_string(),
    }
}

/// A cell's HTML, as the table renders it.
async fn cell_html(column: &dyn Column<Invoice>, row: &Invoice) -> String {
    use crate::test_support::Html as _;
    let cx = crate::test_support::cx();
    Ok::<_, topcoat::Error>(column.cell(&cx, row))
        .html(&cx)
        .await
}

/// An `Options` variant's declared color reaches its badge's dot; a variant without one shows a
/// badge without a dot.
#[tokio::test]
async fn badge_dots_follow_the_option_color() {
    let column = TextColumn::new(lens!(Invoice.stage)).badge();
    let paid = cell_html(&column, &invoice()).await;
    assert!(
        paid.contains("bg-success"),
        "a paid badge has a success dot: {paid}"
    );
    assert!(paid.contains("Paid"), "the badge reads the label: {paid}");

    let open = Invoice {
        stage: Stage::Open,
        ..invoice()
    };
    let open = cell_html(&column, &open).await;
    assert!(!open.contains("rounded-full"), "no color, no dot: {open}");

    let overridden = TextColumn::new(lens!(Invoice.stage))
        .badge()
        .color(|_| Some(Color::Danger));
    let html = cell_html(&overridden, &invoice()).await;
    assert!(
        html.contains("bg-destructive"),
        "a declared color wins: {html}"
    );
}

/// Without a badge, only a declared color tints the text: an `Options` color alone does not.
#[tokio::test]
async fn only_a_declared_color_tints_text() {
    let plain = cell_html(&TextColumn::new(lens!(Invoice.stage)), &invoice()).await;
    assert!(!plain.contains("text-success"), "{plain}");
    let tinted = TextColumn::new(lens!(Invoice.stage)).color(|_| Some(Color::Success));
    let tinted = cell_html(&tinted, &invoice()).await;
    assert!(tinted.contains("text-success"), "{tinted}");
}

/// Money and numbers read formatted, in the cell and in the export, and align to the end.
#[tokio::test]
async fn amounts_format_and_align_to_the_end() {
    let cx = crate::test_support::cx();
    let money = TextColumn::new(lens!(Invoice.total)).money(Currency::USD.minor_units());
    assert_eq!(money.text(&cx, &invoice()), "$1,234.56");
    assert_eq!(money.alignment(), Alignment::End);
    assert!(cell_html(&money, &invoice()).await.contains("tabular-nums"));

    let count = TextColumn::new(lens!(Invoice.total)).numeric(0);
    assert_eq!(count.text(&cx, &invoice()), "123,456");
    assert_eq!(count.alignment(), Alignment::End);

    assert_eq!(
        TextColumn::new(lens!(Invoice.number)).alignment(),
        Alignment::Start
    );
}

/// The description is a second line in the cell, never part of the value the export writes.
#[tokio::test]
async fn description_is_a_second_line_only() {
    let cx = crate::test_support::cx();
    let column = TextColumn::new(lens!(Invoice.number))
        .description(|invoice: &Invoice| invoice.customer.clone());
    assert_eq!(column.text(&cx, &invoice()), "INV-7");
    let html = cell_html(&column, &invoice()).await;
    assert!(html.contains("INV-7") && html.contains("Ada"), "{html}");
}

/// A color column paints a swatch only for a value that cannot leave the `style` declaration.
#[tokio::test]
async fn color_swatch_accepts_only_plain_colors() {
    let column = ColorColumn::new(lens!(Invoice.color));
    let html = cell_html(&column, &invoice()).await;
    assert!(html.contains("background-color: #3b82f6"), "{html}");

    for hostile in [
        "red; background-image: url(x)",
        "#12345",
        "rgb(1 2 3); color: red",
        "rgb(url(x))",
        "oklch(0.6 0.1 200",
    ] {
        let row = Invoice {
            color: hostile.to_string(),
            ..invoice()
        };
        let html = cell_html(&column, &row).await;
        assert!(
            !html.contains("style="),
            "`{hostile}` must not reach a style: {html}"
        );
    }
    assert!(is_css_color("oklch(0.6 0.1 200 / 50%)"));
    assert!(is_css_color("#FFF"));
}

/// An image column shows a thumbnail for a linkable path, and the dash for an empty one.
#[tokio::test]
async fn image_thumbnail_needs_a_linkable_path() {
    let column = ImageColumn::new(lens!(Invoice.logo)).circular();
    let html = cell_html(&column, &invoice()).await;
    assert!(
        html.contains("<img") && html.contains("src=\"/uploads/logo.png\""),
        "{html}"
    );
    assert!(html.contains("rounded-full"), "{html}");

    for path in ["", "javascript:alert(1)", "//evil.example/x.png"] {
        let row = Invoice {
            logo: path.to_string(),
            ..invoice()
        };
        let html = cell_html(&column, &row).await;
        assert!(
            !html.contains("<img"),
            "`{path}` must not be an image: {html}"
        );
    }
}
