use super::{format_value, runs};

fn point(index: usize, value: f64) -> String {
    format!("{index},{value}")
}

#[test]
fn a_line_breaks_where_a_value_is_missing_and_doubles_a_lone_point() {
    let values = [Some(1.0), Some(2.0), None, Some(3.0), None, None];
    assert_eq!(runs(&values, point), ["0,1 1,2", "3,3 3,3"]);
    assert_eq!(runs(&[None, None], point), Vec::<String>::new());
}

#[test]
fn a_whole_value_spells_no_decimals_and_any_other_two() {
    assert_eq!(format_value(1204.0), "1,204");
    assert_eq!(format_value(-3.0), "-3");
    assert_eq!(format_value(2.5), "2.50");
}
