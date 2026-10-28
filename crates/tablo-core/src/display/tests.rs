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
    // More decimals than an i128 has digits: the amount is a fraction of one unit.
    assert_eq!(
        Currency::new("$").decimals(40).minor_units().format(-5_i64),
        format!("-$0.{}5", "0".repeat(39))
    );
}

/// A decimal spells its digits exactly, rounding half to even only when it holds more decimals
/// than the spelling shows.
#[test]
fn a_decimal_spells_exactly_and_rounds_half_to_even() {
    let decimal = |mantissa, scale| Number::Decimal { mantissa, scale };
    assert_eq!(format_number(decimal(123_450, 2), 2), "1,234.50");
    assert_eq!(format_number(decimal(12_345, 1), 3), "1,234.500");
    assert_eq!(format_number(decimal(125, 2), 1), "1.2");
    assert_eq!(format_number(decimal(135, 2), 1), "1.4");
    assert_eq!(format_number(decimal(1251, 3), 1), "1.3");
    assert_eq!(format_number(decimal(999_995, 3), 2), "1,000.00");
    assert_eq!(format_number(decimal(-5, 3), 2), "0.00", "no negative zero");
    assert_eq!(format_number(decimal(-51, 4), 2), "-0.01");
    assert_eq!(format_number(decimal(6, 1), 0), "1");
    assert_eq!(format_number(decimal(5, 40), 2), "0.00");
    assert_eq!(format_number(decimal(i128::MAX, u32::MAX), 2), "0.00");
    assert_eq!(
        format_number(decimal(i128::MIN, 0), 0),
        "-170,141,183,460,469,231,731,687,303,715,884,105,728"
    );
}

/// A `Numeric` holding a [`Number`] as given: the money spelling of a decimal without a decimal
/// crate.
struct Amount(Number);

impl Numeric for Amount {
    fn number(&self) -> Option<Number> {
        Some(self.0)
    }
}

#[test]
fn minor_units_divide_a_decimal_exactly() {
    let cents = |mantissa, scale| Amount(Number::Decimal { mantissa, scale });
    assert_eq!(
        Currency::USD.minor_units().format(cents(123_456, 0)),
        "$1,234.56"
    );
    assert_eq!(Currency::USD.minor_units().format(cents(-55, 1)), "-$0.06");
}

proptest::proptest! {
    /// A decimal spelled with at least its own decimals reads back as its exact value, and one
    /// spelled with fewer lands within half a unit of the last digit shown.
    #[test]
    fn a_decimal_spelling_is_within_half_a_unit(
        mantissa in proptest::num::i64::ANY,
        scale in 0u32..12,
        decimals in 0u8..12,
    ) {
        let spelled = format_number(
            Number::Decimal { mantissa: mantissa.into(), scale },
            decimals,
        );
        let shown: i128 = spelled.replace([',', '.'], "").parse().expect("digits");
        let mantissa = i128::from(mantissa);
        let decimals = u32::from(decimals);
        if decimals >= scale {
            proptest::prop_assert_eq!(shown, mantissa * 10i128.pow(decimals - scale));
        } else {
            let unit = 10i128.pow(scale - decimals);
            let error = (shown * unit - mantissa).abs();
            proptest::prop_assert!(error * 2 <= unit, "{} from {}", spelled, mantissa);
        }
    }
}

#[cfg(feature = "rust_decimal")]
#[test]
fn a_rust_decimal_reads_as_its_digits() {
    let value: rust_decimal::Decimal = "-1234.5".parse().unwrap();
    assert_eq!(
        value.number(),
        Some(Number::Decimal {
            mantissa: -12_345,
            scale: 1
        })
    );
    assert_eq!(Currency::USD.format(value), "-$1,234.50");
}

#[cfg(feature = "bigdecimal")]
#[test]
fn a_big_decimal_reads_as_at_most_38_digits() {
    let big = |value: &str| value.parse::<bigdecimal::BigDecimal>().unwrap().number();
    assert_eq!(
        big("1E+5"),
        Some(Number::Decimal {
            mantissa: 100_000,
            scale: 0
        })
    );
    assert_eq!(
        big("0.1234567890123456789012345678901234567851"),
        Some(Number::Decimal {
            mantissa: 12_345_678_901_234_567_890_123_456_789_012_345_679,
            scale: 38
        }),
        "rounded to 38 digits"
    );
    assert_eq!(big("1E+50"), Some(Number::Float(1e50)));
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
