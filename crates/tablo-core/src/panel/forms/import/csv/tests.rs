use proptest::prelude::*;

use super::{Record, Unread};

/// Bounds no test file reaches.
fn records(text: &str) -> Result<Vec<Record>, Unread> {
    super::records(text, usize::MAX, usize::MAX)
}

/// `written` as the reader returns it: numbered from 1, without its blank records.
fn numbered(written: Vec<Vec<String>>) -> Vec<Record> {
    written
        .into_iter()
        .enumerate()
        .map(|(index, cells)| (index + 1, cells))
        .filter(|(_, cells)| cells.iter().any(|cell| !cell.trim().is_empty()))
        .collect()
}

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
        prop_assert_eq!(
            records(&write(&written, delimiter, newline)),
            Ok(numbered(written))
        );
    }

    /// A one-column file, which shows no delimiter, reads as a comma-separated one.
    #[test]
    fn a_one_column_file_reads_back(
        written in prop::collection::vec(prop::collection::vec(cell(), 1..2), 1..5),
    ) {
        prop_assert_eq!(records(&write(&written, ',', "\n")), Ok(numbered(written)));
    }
}

#[test]
fn a_byte_order_mark_and_a_missing_last_line_break_change_nothing() {
    assert_eq!(
        records("\u{FEFF}name,email\nAda,ada@example.com"),
        Ok(vec![
            (1, vec!["name".to_string(), "email".to_string()]),
            (2, vec!["Ada".to_string(), "ada@example.com".to_string()]),
        ])
    );
}

#[test]
fn a_quote_inside_a_cell_is_a_character() {
    assert_eq!(
        records("a\"b,c"),
        Ok(vec![(1, vec!["a\"b".to_string(), "c".to_string()])])
    );
}

#[test]
fn an_unclosed_quote_names_the_line_it_opens_on() {
    assert_eq!(
        records("name\nAda\n\"Grace\nHopper"),
        Err(Unread::Unterminated { line: 3 })
    );
}

#[test]
fn an_empty_file_holds_no_record() {
    assert_eq!(records(""), Ok(Vec::new()));
}

/// A file of line breaks or delimiters holds no more memory than the bounds allow.
#[test]
fn the_reader_stops_at_its_bounds() {
    assert_eq!(super::records(&"\n".repeat(100_000), 2, 4), Ok(Vec::new()));
    assert_eq!(
        super::records("h\na\n\nb\nc\n", 3, 4),
        Err(Unread::TooManyRecords)
    );
    assert_eq!(
        super::records("h\n\n,,,,\n", 3, 4),
        Err(Unread::TooManyCells { record: 3 })
    );
    assert_eq!(
        super::records("h\n\n,,,\nx\n", 3, 4),
        Ok(vec![(1, vec!["h".to_string()]), (4, vec!["x".to_string()])])
    );
}

/// A quote inside a cell of the first record opens nothing, as the reader reads it.
#[test]
fn the_delimiter_reads_quotes_as_the_reader_does() {
    assert_eq!(
        records("Width 5\";name;x\n1;2;3"),
        Ok(vec![
            (
                1,
                vec!["Width 5\"".to_string(), "name".to_string(), "x".to_string()]
            ),
            (2, vec!["1".to_string(), "2".to_string(), "3".to_string()]),
        ])
    );
}
