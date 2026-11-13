use super::*;

/// A row the next widget does not fit in grows its last widget over the columns it leaves empty.
#[test]
fn a_widget_alone_on_its_row_fills_it() {
    let (half, full) = (Span::Columns(1), Span::Full);
    assert_eq!(
        filled(2, &[full, half, full]),
        [full, full, full],
        "a half between two fulls takes the row"
    );
    assert_eq!(filled(2, &[half, half, half]), [half, half, full]);
    assert_eq!(
        filled(3, &[half, half, full, Span::Columns(2)]),
        [half, Span::Columns(2), full, full],
        "a row of three left one short grows its last widget by one"
    );
    assert_eq!(filled(2, &[]), []);
}
