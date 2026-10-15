use super::*;

#[test]
fn thousands_group_from_the_point() {
    assert_eq!(format_number(Number::Int(0), 0), "0");
    assert_eq!(format_number(Number::Int(999), 0), "999");
    assert_eq!(format_number(Number::Int(1000), 0), "1,000");
    assert_eq!(format_number(Number::Int(-1_234_567), 0), "-1,234,567");
    assert_eq!(format_number(Number::Int(42), 2), "42.00");
    assert_eq!(format_number(Number::Float(1234.5), 1), "1,234.5");
    assert_eq!(
        format_number(Number::Float(-999_999.996), 2),
        "-1,000,000.00"
    );
}

#[test]
fn a_negative_zero_reads_as_zero() {
    assert_eq!(format_number(Number::Float(-0.001), 2), "0.00");
}

#[test]
fn money_puts_the_sign_before_the_symbol() {
    assert_eq!(Currency::USD.format(-1234.5_f64), "-$1,234.50");
    assert_eq!(Currency::EUR.symbol_after().format(-2_i32), "-2.00 €");
    assert_eq!(Currency::JPY.format(1234.6_f64), "¥1,235");
}

#[test]
fn minor_units_divide_exactly() {
    assert_eq!(Currency::USD.minor_units().format(123_456_i64), "$1,234.56");
    assert_eq!(Currency::USD.minor_units().format(-5_i64), "-$0.05");
    assert_eq!(Currency::JPY.minor_units().format(1500_i64), "¥1,500");
    // Past f64's exact range: an integer amount never goes through a float.
    assert_eq!(
        Currency::USD
            .minor_units()
            .format(9_007_199_254_740_993_i64),
        "$90,071,992,547,409.93"
    );
    assert_eq!(Currency::USD.minor_units().format(250.0_f64), "$2.50");
}

#[test]
fn none_spells_nothing() {
    assert_eq!(Currency::USD.format(None::<f64>), "");
    assert_eq!(Currency::USD.format(Some(3_u8)), "$3.00");
}

#[test]
fn option_colors_parse_by_name() {
    assert_eq!(Color::from_name("success"), Some(Color::Success));
    assert_eq!(Color::from_name("danger"), Some(Color::Danger));
    assert_eq!(Color::from_name("red"), None);
}
