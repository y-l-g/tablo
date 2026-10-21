//! Reads an uploaded CSV file into its records: RFC 4180 quoting, a comma or a semicolon between
//! cells, and LF, CRLF or CR between records.

/// A quoted cell the file never closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unterminated {
    /// The line the cell opens on, from 1.
    pub(super) line: usize,
}

/// Every record of `text`, each as its cells, in file order. A leading byte-order mark is
/// skipped, and a file that ends with a line break holds no empty record after it.
///
/// The delimiter is the first record's: a semicolon when it holds one outside quotes and no
/// comma, as a spreadsheet in a locale with a decimal comma writes, else a comma. A quote opens a
/// quoted cell only at the cell's start; elsewhere it is a character of the cell.
pub(super) fn records(text: &str) -> Result<Vec<Vec<String>>, Unterminated> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let delimiter = delimiter(text);
    let mut records = Vec::new();
    let mut record = Vec::new();
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
                record.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut record));
                fresh = true;
                line += 1;
            }
            c if c == delimiter => {
                record.push(std::mem::take(&mut cell));
                fresh = true;
            }
            c => {
                cell.push(c);
                fresh = false;
            }
        }
    }
    if let Some(line) = quoted {
        return Err(Unterminated { line });
    }
    if !fresh || !record.is_empty() {
        record.push(cell);
        records.push(record);
    }
    Ok(records)
}

/// The first record's delimiter, read outside its quoted cells.
fn delimiter(text: &str) -> char {
    let mut quoted = false;
    let mut semicolon = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => return ',',
            ';' if !quoted => semicolon = true,
            '\n' | '\r' if !quoted => break,
            _ => {}
        }
    }
    if semicolon { ';' } else { ',' }
}

#[cfg(test)]
mod tests;
