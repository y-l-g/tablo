//! Reads an uploaded CSV file into its records: RFC 4180 quoting, a comma or a semicolon between
//! cells, and LF, CRLF or CR between records.

/// Why a file is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unread {
    /// A quoted cell the file never closes, opening on `line`, from 1.
    Unterminated { line: usize },
    /// The file holds more records than the reader keeps.
    TooManyRecords,
    /// Record `record`, from 1, holds more cells than the reader keeps.
    TooManyCells { record: usize },
}

/// A record the file holds: its number, from 1 and counting blank records, and its cells.
pub(super) type Record = (usize, Vec<String>);

/// Every record of `text` holding a cell that is not blank, in file order. A leading byte-order
/// mark is skipped, and a file that ends with a line break holds no empty record after it.
///
/// The reader stops at the first record past `max_records` or the first cell past `max_cells`
/// in one record, so a small file of line breaks or delimiters holds memory to those bounds.
///
/// The delimiter is the first record's: a semicolon when it holds one outside quotes and no
/// comma, as a spreadsheet in a locale with a decimal comma writes, else a comma. A quote opens a
/// quoted cell only at the cell's start; elsewhere it is a character of the cell.
pub(super) fn records(
    text: &str,
    max_records: usize,
    max_cells: usize,
) -> Result<Vec<Record>, Unread> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let delimiter = delimiter(text);
    let mut reader = Reader {
        records: Vec::new(),
        record: Vec::new(),
        number: 0,
        max_records,
        max_cells,
    };
    let mut cell = String::new();
    // Whether the cell has read anything, a quote included: only a fresh cell opens quotes.
    let mut fresh = true;
    let mut quoted = None;
    let mut line = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted.is_some() {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = None,
                _ => {
                    if c == '\n' || (c == '\r' && chars.peek() != Some(&'\n')) {
                        line += 1;
                    }
                    cell.push(c);
                }
            }
            continue;
        }
        match c {
            '"' if fresh => {
                quoted = Some(line);
                fresh = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                reader.cell(std::mem::take(&mut cell))?;
                reader.end()?;
                fresh = true;
                line += 1;
            }
            c if c == delimiter => {
                reader.cell(std::mem::take(&mut cell))?;
                fresh = true;
            }
            c => {
                cell.push(c);
                fresh = false;
            }
        }
    }
    if let Some(line) = quoted {
        return Err(Unread::Unterminated { line });
    }
    if !fresh || !reader.record.is_empty() {
        reader.cell(cell)?;
        reader.end()?;
    }
    Ok(reader.records)
}

/// The records read so far and the one being read.
struct Reader {
    records: Vec<Record>,
    record: Vec<String>,
    /// The number of records ended, blank ones included.
    number: usize,
    max_records: usize,
    max_cells: usize,
}

impl Reader {
    fn cell(&mut self, cell: String) -> Result<(), Unread> {
        if self.record.len() == self.max_cells {
            return Err(Unread::TooManyCells {
                record: self.number + 1,
            });
        }
        self.record.push(cell);
        Ok(())
    }

    /// Ends the record being read, keeping it unless its every cell is blank.
    fn end(&mut self) -> Result<(), Unread> {
        self.number += 1;
        if self.record.iter().all(|cell| cell.trim().is_empty()) {
            self.record.clear();
            return Ok(());
        }
        if self.records.len() == self.max_records {
            return Err(Unread::TooManyRecords);
        }
        self.records
            .push((self.number, std::mem::take(&mut self.record)));
        Ok(())
    }
}

/// The first record's delimiter, read outside its quoted cells as [`records`] reads them.
fn delimiter(text: &str) -> char {
    let mut quoted = false;
    let mut fresh = true;
    let mut semicolon = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' && chars.next_if_eq(&'"').is_none() {
                quoted = false;
            }
            continue;
        }
        match c {
            '"' if fresh => {
                quoted = true;
                fresh = false;
            }
            ',' => return ',',
            ';' => {
                semicolon = true;
                fresh = true;
            }
            '\n' | '\r' => break,
            _ => fresh = false,
        }
    }
    if semicolon { ';' } else { ',' }
}

#[cfg(test)]
mod tests;
