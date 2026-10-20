use super::*;
use crate::{ComputedColumn, display::Currency, lens, test_support::User};

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
        .include_unscoped(Pet::fields().owner())
        .include_unscoped(Pet::fields().vet())
        .include_unscoped(Pet::fields().owner());
    assert_eq!(
        column.includes().len(),
        2,
        "two relations, one repeated: the column keeps each once"
    );
}

/// A date column's text is the stored spelling of every date type, a null spells empty, and
/// `TextColumn` still spells the stored value. The day its cell shows is the stored value's own,
/// never one a local time zone moves.
#[test]
fn a_date_column_texts_the_stored_value_and_names_its_day() {
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
    assert_eq!(text(&DateColumn::new(lens!(Event.day))), "2026-10-14");
    assert_eq!(
        text(&DateColumn::new(lens!(Event.at))),
        "2026-10-14T09:30:00"
    );
    assert_eq!(
        text(&DateColumn::new(lens!(Event.seen))),
        "2026-10-14T23:30:00Z"
    );
    assert_eq!(text(&DateColumn::new(lens!(Event.closed))), "");
    assert_eq!(text(&TextColumn::new(lens!(Event.day))), "2026-10-14");

    let day = Some(jiff::civil::date(2026, 10, 14));
    assert_eq!(row.day.calendar_date(&jiff::tz::TimeZone::UTC), day);
    assert_eq!(row.at.calendar_date(&jiff::tz::TimeZone::UTC), day);
    assert_eq!(row.seen.calendar_date(&jiff::tz::TimeZone::UTC), day);
    assert_eq!(row.closed.calendar_date(&jiff::tz::TimeZone::UTC), None);
    assert_eq!(
        Column::column_width(&DateColumn::new(lens!(Event.day))),
        ColumnWidth::Auto
    );
}

#[derive(Debug, Clone, toasty::Model)]
struct Invoice {
    #[key]
    #[auto]
    id: uuid::Uuid,
    number: String,
    customer: String,
    total: i64,
}

fn invoice() -> Invoice {
    Invoice {
        id: uuid::Uuid::nil(),
        number: "INV-7".to_string(),
        customer: "Ada".to_string(),
        total: 123_456,
    }
}

/// Money and numbers read formatted, in the cell and in the export, and align to the end.
#[test]
fn amounts_format_and_align_to_the_end() {
    let cx = crate::test_support::cx();
    let money = TextColumn::new(lens!(Invoice.total)).money(Currency::USD.minor_units());
    assert_eq!(money.text(&cx, &invoice()), "$1,234.56");
    assert_eq!(money.alignment(), Alignment::End);

    let count = TextColumn::new(lens!(Invoice.total)).numeric(0);
    assert_eq!(count.text(&cx, &invoice()), "123,456");
    assert_eq!(count.alignment(), Alignment::End);

    assert_eq!(
        TextColumn::new(lens!(Invoice.number)).alignment(),
        Alignment::Start
    );
}

/// The description is never part of the value the export writes; the integration suite's
/// `columns` shows it on the cell.
#[test]
fn a_description_stays_out_of_the_text() {
    let cx = crate::test_support::cx();
    let column = TextColumn::new(lens!(Invoice.number))
        .description(|invoice: &Invoice| invoice.customer.clone());
    assert_eq!(column.text(&cx, &invoice()), "INV-7");
}

/// A color reaches a `style` declaration only when nothing in it can end the declaration or open
/// another.
#[test]
fn a_swatch_color_is_plain() {
    for plain in ["#3b82f6", "#FFF", "oklch(0.6 0.1 200 / 50%)", "rgb(1 2 3)"] {
        assert!(is_css_color(plain), "{plain}");
    }
    for hostile in [
        "red; background-image: url(x)",
        "#12345",
        "rgb(1 2 3); color: red",
        "rgb(url(x))",
        "oklch(0.6 0.1 200",
    ] {
        assert!(!is_css_color(hostile), "{hostile}");
    }
}
