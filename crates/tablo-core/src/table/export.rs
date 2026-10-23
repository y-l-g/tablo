//! [`Table`] CSV export: streamed header/row fragments plus RFC4180 escaping.

use topcoat::context::Cx;

use super::Table;

impl<M> Table<M> {
    /// CSV header line for this table (labels, RFC4180 escaped, trailing
    /// newline included) — the first fragment of a streamed export.
    pub(crate) fn csv_header(&self, cx: &Cx) -> String
    where
        M: toasty::schema::Model,
    {
        let mut out = String::new();
        let headers: Vec<String> = self
            .columns
            .iter()
            .map(|c| escape_csv(&c.label().localize(cx)))
            .collect();
        out.push_str(&headers.join(","));
        out.push('\n');
        out
    }

    /// CSV line for one row (cells, RFC4180 escaped, trailing newline
    /// included) — one fragment of a streamed export. Formula cells are
    /// defused per OWASP (a leading `'` is prepended when the first
    /// non-whitespace/control character is `=`, `+`, `-`, `@`, `|` or `%`,
    /// including CR/LF- or tab-led variants) so a stored value like
    /// `=1+1` opens as text, not a live spreadsheet formula. A negative
    /// number such as `-$1,234.50` stays as it is, so it opens as a number.
    pub(crate) fn csv_row(&self, cx: &Cx, row: &M) -> String
    where
        M: toasty::schema::Model,
    {
        let mut out = String::new();
        let cells: Vec<String> = self
            .columns
            .iter()
            .map(|c| escape_csv(&c.text(cx, row)))
            .collect();
        out.push_str(&cells.join(","));
        out.push('\n');
        out
    }
}

fn defuse_formula(s: &str) -> String {
    // Spreadsheets run formulas led by CR/LF/tab too (OWASP CSV
    // injection): the dangerous payload can start mid-cell after
    // leading whitespace, controls, or zero-width format characters
    // (BOM/ZWSP are neither whitespace nor control), so the
    // first-char test skips them. The `'` lands on the original
    // cell, before the payload.
    let trimmed = s.trim_start_matches(|c: char| {
        c.is_whitespace() || c.is_control() || matches!(c, '\u{FEFF}' | '\u{200B}')
    });
    if let Some(first) = trimmed.chars().next()
        && matches!(first, '=' | '+' | '-' | '@' | '|' | '%')
        && !is_negative_number(trimmed)
    {
        return format!("'{s}");
    }
    s.to_string()
}

/// Whether `s` is a negative number as a column formats one (`-1234.5`, `-$1,234.50`, `-12%`):
/// a spreadsheet reads it as a number, not a formula, so a `'` would only turn it into text.
fn is_negative_number(s: &str) -> bool {
    s.strip_prefix('-').is_some_and(|rest| {
        rest.chars().any(|c| c.is_ascii_digit())
            && rest.chars().all(|c| {
                c.is_ascii_digit()
                    || matches!(
                        c,
                        '.' | ',' | ' ' | '\u{A0}' | '\u{202F}' | '$' | '€' | '£' | '¥' | '%'
                    )
            })
    })
}

fn escape_csv(s: &str) -> String {
    let s = defuse_formula(s);
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

#[cfg(test)]
mod tests;
