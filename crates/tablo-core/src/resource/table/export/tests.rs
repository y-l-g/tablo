use super::*;
use crate::{resource::TextColumn, test_support::User};

#[test]
fn csv_row_defuses_formula_cells_per_owasp() {
    let csv_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    for payload in ["=1+1", "+1+1", "-1+1", "@SUM(1+1)", "|id", "%x", "  =cmd"] {
        let user = User {
            id: uuid::Uuid::nil(),
            name: payload.to_string(),
        };
        let body = csv_table.csv_row(&user);
        assert!(
            body.starts_with('\''),
            "formula payload {payload:?} must be defused with leading `'`, got {body:?}"
        );
    }
    // Plain values stay untouched; RFC4180 quoting still applies.
    let user = User {
        id: uuid::Uuid::nil(),
        name: "Ada, \"the\" first".to_string(),
    };
    let csv = csv_table.csv_row(&user);
    assert!(
        csv.contains("\"Ada, \"\"the\"\" first\""),
        "quoting broke: {csv:?}"
    );
}

#[test]
fn csv_row_defuses_cr_lf_led_formula_cells() {
    // Spreadsheets treat CR/LF- and tab-led payloads as formulas
    // even when the dangerous character does not start the raw cell, so
    // the defuse test skips leading whitespace/controls. CR/LF-led cells
    // are RFC4180-quoted (they carry a newline); a tab-led cell has no
    // quote/comma/newline and stays bare — either way the `'` leads the
    // defused content.
    let csv_table = Table::<User>::new(
        |u| u.id.to_string(),
        TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
    );
    for (payload, defused) in [
        ("\r=1+1", "'\r=1+1"),
        ("\n@cmd", "'\n@cmd"),
        ("\t+1+1", "'\t+1+1"),
        (" \r=HYPERLINK(1,2)", "' \r=HYPERLINK(1,2)"),
        // BOM/ZWSP are neither whitespace nor control: cover the
        // format-character gap explicitly.
        ("\u{FEFF}=1+1", "'\u{FEFF}=1+1"),
        ("\u{200B}@cmd", "'\u{200B}@cmd"),
    ] {
        let user = User {
            id: uuid::Uuid::nil(),
            name: payload.to_string(),
        };
        let csv = csv_table.csv_row(&user);
        assert!(
            csv.contains(defused),
            "CR/LF-led formula payload {payload:?} must be defused to {defused:?}, got {csv:?}"
        );
    }
}
