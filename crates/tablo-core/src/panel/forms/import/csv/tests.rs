use proptest::prelude::*;

use super::{Unterminated, records};

/// Writes `records` as a spreadsheet does: a cell holding a delimiter, a quote or a line break is
/// quoted, its quotes doubled, and each record ends with `newline`.
fn write(records: &[Vec<String>], delimiter: char, newline: &str) -> String {
    let mut out = String::new();
    for record in records {
        let cells: Vec<String> = record
            .iter()
            .map(|cell| {
                if cell.contains([',', ';', '"', '\n', '\r']) {
                    format!("\"{}\"", cell.replace('"', "\"\""))
                } else {
                    cell.clone()
                }
            })
            .collect();
        out.push_str(&cells.join(&delimiter.to_string()));
        out.push_str(newline);
    }
    out
}

fn cell() -> impl Strategy<Value = String> {
    "[a-z ,;\"\r\n=é]{0,6}"
}

proptest! {
    /// A file read back holds the records written, whatever the cells hold and however the
    /// records end, when the first record shows the delimiter.
    #[test]
    fn a_written_file_reads_back_as_its_records(
        first in prop::collection::vec(cell(), 2..5),
        rest in prop::collection::vec(prop::collection::vec(cell(), 1..5), 0..5),
        delimiter in prop::sample::select(vec![',', ';']),
        newline in prop::sample::select(vec!["\n", "\r\n", "\r"]),
    ) {
        let mut written = vec![first];
        written.extend(rest);
        prop_assert_eq!(records(&write(&written, delimiter, newline)), Ok(written));
    }

    /// A one-column file, which shows no delimiter, reads as a comma-separated one.
    #[test]
    fn a_one_column_file_reads_back(
        written in prop::collection::vec(prop::collection::vec(cell(), 1..2), 1..5),
    ) {
        prop_assert_eq!(records(&write(&written, ',', "\n")), Ok(written));
    }
}

#[test]
fn a_byte_order_mark_and_a_missing_last_line_break_change_nothing() {
    assert_eq!(
        records("\u{FEFF}name,email\nAda,ada@example.com"),
        Ok(vec![
            vec!["name".to_string(), "email".to_string()],
            vec!["Ada".to_string(), "ada@example.com".to_string()],
        ])
    );
}

#[test]
fn a_quote_inside_a_cell_is_a_character() {
    assert_eq!(
        records("a\"b,c"),
        Ok(vec![vec!["a\"b".to_string(), "c".to_string()]])
    );
}

#[test]
fn an_unclosed_quote_names_the_line_it_opens_on() {
    assert_eq!(
        records("name\nAda\n\"Grace\nHopper"),
        Err(Unterminated { line: 3 })
    );
}

#[test]
fn an_empty_file_holds_no_record() {
    assert_eq!(records(""), Ok(Vec::new()));
}
