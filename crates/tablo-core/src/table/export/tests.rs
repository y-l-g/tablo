use proptest::prelude::*;

use super::*;
use crate::{lens, table::TextColumn, test_support::User};

/// What a spreadsheet reads back from one RFC 4180 cell.
fn unquote(cell: &str) -> String {
    match cell
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    {
        Some(inner) => inner.replace("\"\"", "\""),
        None => cell.to_string(),
    }
}

/// Whether a spreadsheet would run `cell` as a formula: its first character past whitespace,
/// controls and zero-width format characters starts one, and it is more than a minus sign and
/// a number's digits and separators.
fn runs_as_formula(cell: &str) -> bool {
    let lead = cell.trim_start_matches(|c: char| {
        c.is_whitespace() || c.is_control() || matches!(c, '\u{FEFF}' | '\u{200B}')
    });
    let numeral = lead.strip_prefix('-').is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_digit() || ".,$€£¥% \u{A0}\u{202F}".contains(c))
            && rest.chars().any(|c| c.is_ascii_digit())
    });
    lead.chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '|' | '%'))
        && !numeral
}

/// A cell spreadsheets could misread: formula leads behind whitespace, controls and format
/// characters, with quotes, commas and line breaks RFC 4180 must quote.
fn cell() -> impl Strategy<Value = String> {
    prop_oneof![
        "[ \t\r\n\u{FEFF}\u{200B}]{0,3}[=+\\-@|%a\",]\\PC{0,12}",
        "\\PC{0,24}",
    ]
}

proptest! {
    /// Every exported cell reads back as its value, defused: nothing a spreadsheet runs, and the
    /// text unchanged after the `'` a defused cell leads with.
    #[test]
    fn an_exported_cell_reads_back_as_inert_text(value in cell()) {
        let read = unquote(&escape_csv(&value));
        prop_assert!(!runs_as_formula(&read), "{read:?} runs as a formula");
        if runs_as_formula(&value) {
            prop_assert_eq!(read, format!("'{value}"));
        } else {
            prop_assert_eq!(read, value);
        }
    }

    /// A cell holding a separator, a quote or a line break is quoted, so it stays one cell.
    #[test]
    fn a_cell_with_a_separator_stays_one_cell(value in cell()) {
        let cell = escape_csv(&value);
        if value.contains([',', '"', '\n', '\r']) {
            prop_assert!(cell.starts_with('"') && cell.ends_with('"'), "{cell:?}");
            prop_assert!(!cell[1..cell.len() - 1].replace("\"\"", "").contains('"'), "{cell:?}");
        }
    }
}

/// A row's cells go through the escaping, in column order, on one line.
#[test]
fn a_csv_row_escapes_each_cell() {
    let table = Table::<User>::new(TextColumn::new(lens!(User.name)));
    let user = User {
        id: uuid::Uuid::nil(),
        name: "=Ada, \"the\" first".to_string(),
    };
    assert_eq!(
        table.csv_row(&crate::test_support::cx(), &user),
        "\"'=Ada, \"\"the\"\" first\"\n"
    );
}

/// A negative number exports as itself, so a spreadsheet reads a number; a cell carrying anything
/// more is still defused.
#[test]
fn a_negative_number_exports_undefused() {
    for number in ["-12", "-1234.50", "-$1,234.50", "-12%", " -7"] {
        assert_eq!(unquote(&escape_csv(number)), number);
    }
    for formula in ["-1+1", "-A1", "-", "-$", "-2*3", "-1)"] {
        assert_eq!(unquote(&escape_csv(formula)), format!("'{formula}"));
    }
}
