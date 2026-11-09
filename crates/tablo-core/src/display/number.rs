//! The spellings of a [`Numeric`] value: a number with its thousands grouped, a percentage, and
//! an amount of a [`Currency`], in English or in the request's locale.

use super::{Number, Numeric};

impl Number {
    /// Spell `value` with `decimals` digits after the point and its thousands grouped, in
    /// English, or an empty string when it holds none.
    ///
    /// ```rust
    /// use tablo_core::Number;
    ///
    /// assert_eq!(Number::format(1234.5, 2), "1,234.50");
    /// assert_eq!(Number::format(-3, 0), "-3");
    /// ```
    pub fn format(value: impl Numeric, decimals: u8) -> String {
        value
            .number()
            .map(|number| NumberFormat::english().spell(number, decimals))
            .unwrap_or_default()
    }

    /// Spell `value` as [`format`](Self::format) does, with the separators of the request's
    /// locale: `1 234,50` in French.
    pub fn format_in(cx: &topcoat::context::Cx, value: impl Numeric, decimals: u8) -> String {
        value
            .number()
            .map(|number| NumberFormat::of(cx).spell(number, decimals))
            .unwrap_or_default()
    }

    /// Spell `value`, a percentage such as `12.5` for twelve and a half percent, with
    /// `decimals` digits, in English, or an empty string when it holds none.
    ///
    /// ```rust
    /// use tablo_core::Number;
    ///
    /// assert_eq!(Number::percent(12.5, 1), "12.5%");
    /// assert_eq!(Number::percent(-1250, 0), "-1,250%");
    /// ```
    pub fn percent(value: impl Numeric, decimals: u8) -> String {
        let number = Self::format(value, decimals);
        if number.is_empty() {
            return number;
        }
        format!("{number}%")
    }

    /// Spell `value` as [`percent`](Self::percent) does, in the request's locale: its
    /// separators, and its `tablo-percent` message, `12,5 %` in French.
    pub fn percent_in(cx: &topcoat::context::Cx, value: impl Numeric, decimals: u8) -> String {
        let number = Self::format_in(cx, value, decimals);
        if number.is_empty() {
            return number;
        }
        crate::t!(cx, "tablo-percent", number = number)
    }
}

/// How a money column spells its amounts: the symbol, where it goes, the decimals, and whether
/// the field stores minor units.
///
/// ```rust
/// use tablo_core::Currency;
///
/// assert_eq!(Currency::USD.format(1234.5), "$1,234.50");
/// assert_eq!(Currency::EUR.format(1234.5), "€1,234.50");
/// assert_eq!(
///     Currency::new("CHF").symbol_after().format(-3.0),
///     "-3.00 CHF"
/// );
/// assert_eq!(Currency::USD.minor_units().format(123_456), "$1,234.56");
/// ```
///
/// [`format`](Self::format) spells English; a column spells the request's locale through
/// [`format_in`](Self::format_in): its catalog's separators, `1 234,50`, and its side for the
/// symbol, `1 234,50 €` in French, unless [`symbol_before`](Self::symbol_before) or
/// [`symbol_after`](Self::symbol_after) fixes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Currency {
    symbol: &'static str,
    decimals: u8,
    placement: Placement,
    minor_units: bool,
}

/// Which side of the amount a [`Currency`] puts its symbol on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    /// The side the locale's `tablo-money` message puts it on; before in English.
    Locale,
    Before,
    After,
}

impl Currency {
    /// US dollars, `$1,234.50`.
    pub const USD: Self = Self::new("$");
    /// Euros, `€1,234.50`.
    pub const EUR: Self = Self::new("€");
    /// Pounds sterling, `£1,234.50`.
    pub const GBP: Self = Self::new("£");
    /// Yen, `¥1,235`: no minor unit.
    pub const JPY: Self = Self::new("¥").decimals(0);

    /// A currency spelled with `symbol` before the amount and two decimals.
    pub const fn new(symbol: &'static str) -> Self {
        Self {
            symbol,
            decimals: 2,
            placement: Placement::Locale,
            minor_units: false,
        }
    }

    /// Spell `decimals` digits after the point.
    #[must_use]
    pub const fn decimals(mut self, decimals: u8) -> Self {
        self.decimals = decimals;
        self
    }

    /// Put the symbol after the amount, separated by a space, in every locale.
    #[must_use]
    pub const fn symbol_after(mut self) -> Self {
        self.placement = Placement::After;
        self
    }

    /// Put the symbol before the amount in every locale, as English does.
    #[must_use]
    pub const fn symbol_before(mut self) -> Self {
        self.placement = Placement::Before;
        self
    }

    /// Read the field as minor units, cents for a two-decimal currency: `123456` is `1,234.56`.
    #[must_use]
    pub const fn minor_units(mut self) -> Self {
        self.minor_units = true;
        self
    }

    /// Spell `value` as an amount of this currency in English, or an empty string when it holds
    /// none.
    pub fn format(&self, value: impl Numeric) -> String {
        self.spell(value, &NumberFormat::english(), None)
    }

    /// Spell `value` as an amount of this currency in the request's locale, or an empty string
    /// when it holds none.
    pub fn format_in(&self, cx: &topcoat::context::Cx, value: impl Numeric) -> String {
        self.spell(value, &NumberFormat::of(cx), Some(cx))
    }

    /// Spell `value` with `format`'s separators, the symbol on the side `cx`'s locale puts it
    /// unless the currency fixes it.
    fn spell(
        &self,
        value: impl Numeric,
        format: &NumberFormat,
        cx: Option<&topcoat::context::Cx>,
    ) -> String {
        let Some(number) = value.number() else {
            return String::new();
        };
        let spelled = match (self.minor_units, number) {
            (true, Number::Int(minor)) => format_minor_units(minor, self.decimals),
            (true, Number::Float(minor)) => format_number(
                Number::Float(minor / 10f64.powi(self.decimals.into())),
                self.decimals,
            ),
            (true, Number::Decimal { mantissa, scale }) => format_number(
                Number::Decimal {
                    mantissa,
                    scale: scale.saturating_add(self.decimals.into()),
                },
                self.decimals,
            ),
            (false, number) => format_number(number, self.decimals),
        };
        let spelled = format.localize(&spelled);
        let (sign, digits) = match spelled.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("", spelled.as_str()),
        };
        match (self.placement, cx) {
            (Placement::After, _) => format!("{sign}{digits} {}", self.symbol),
            (Placement::Locale, Some(cx)) => crate::t!(
                cx,
                "tablo-money",
                sign = sign.to_owned(),
                symbol = self.symbol,
                amount = digits.to_owned()
            ),
            (Placement::Before | Placement::Locale, _) => {
                format!("{sign}{}{digits}", self.symbol)
            }
        }
    }
}

/// Spell `minor` units with `decimals` of them to the major unit, exactly: `-5` cents is `-0.05`.
fn format_minor_units(minor: i128, decimals: u8) -> String {
    // Past 38 decimals no `i128` reaches one major unit.
    let (whole, fraction) = match 10i128.checked_pow(decimals.into()) {
        Some(scale) => (minor.abs() / scale, minor.abs() % scale),
        None => (0, minor.abs()),
    };
    let whole = format_number(Number::Int(whole), 0);
    let sign = if minor < 0 { "-" } else { "" };
    if decimals == 0 {
        return format!("{sign}{whole}");
    }
    format!(
        "{sign}{whole}.{fraction:0width$}",
        width = usize::from(decimals)
    )
}

/// How a locale writes a number: its thousands separator and its decimal mark, from its catalog's
/// `tablo-number-group` and `tablo-number-decimal`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NumberFormat {
    pub(crate) group: String,
    pub(crate) decimal: String,
}

impl NumberFormat {
    /// `1,234.5`.
    pub(crate) fn english() -> Self {
        Self {
            group: ",".to_owned(),
            decimal: ".".to_owned(),
        }
    }

    /// The request's locale's.
    pub(crate) fn of(cx: &topcoat::context::Cx) -> Self {
        Self {
            group: crate::t!(cx, "tablo-number-group"),
            decimal: crate::t!(cx, "tablo-number-decimal"),
        }
    }

    /// `number` with `decimals` digits after the mark and its thousands grouped.
    pub(crate) fn spell(&self, number: Number, decimals: u8) -> String {
        self.localize(&format_number(number, decimals))
    }

    /// `english`, a number [`format_number`] spelled, in this locale's separators.
    fn localize(&self, english: &str) -> String {
        if *self == Self::english() {
            return english.to_owned();
        }
        let mut spelled = String::with_capacity(english.len() + 4);
        for char in english.chars() {
            match char {
                ',' => spelled.push_str(&self.group),
                '.' => spelled.push_str(&self.decimal),
                other => spelled.push(other),
            }
        }
        spelled
    }
}

/// Spell `number` with `decimals` digits after the point and its thousands grouped by `,`.
pub(crate) fn format_number(number: Number, decimals: u8) -> String {
    let fixed = match number {
        Number::Int(int) if decimals == 0 => int.to_string(),
        Number::Int(int) => format!("{int}.{}", "0".repeat(decimals.into())),
        Number::Float(float) => {
            let fixed = format!("{float:.*}", usize::from(decimals));
            // `-0.00` reads as zero.
            if fixed
                .trim_start_matches('-')
                .bytes()
                .all(|b| b == b'0' || b == b'.')
            {
                fixed.trim_start_matches('-').to_string()
            } else {
                fixed
            }
        }
        Number::Decimal { mantissa, scale } => fixed_decimal(mantissa, scale, decimals),
    };
    let (sign, unsigned) = match fixed.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", fixed.as_str()),
    };
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (unsigned, None),
    };
    let mut grouped = String::with_capacity(whole.len() + whole.len() / 3);
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

/// Spell `mantissa / 10^scale` with `decimals` digits after the point and no grouping, rounding
/// half to even. The digits are worked as text: a scale far past the mantissa's length drops
/// every digit without spelling the zeros between.
fn fixed_decimal(mantissa: i128, scale: u32, decimals: u8) -> String {
    let digits = mantissa.unsigned_abs().to_string();
    let decimals = u32::from(decimals);
    // The value times `10^decimals`, as digits.
    let mut units = if scale <= decimals {
        let zeros = (decimals - scale) as usize;
        format!("{digits}{}", "0".repeat(zeros))
    } else {
        // `scale - decimals` digits to drop: at most the length, else the value rounds to zero.
        let dropped = usize::try_from(scale - decimals)
            .ok()
            .filter(|&dropped| dropped <= digits.len());
        match dropped {
            Some(dropped) => {
                let (kept, rest) = digits.split_at(digits.len() - dropped);
                let mut kept = if kept.is_empty() { "0" } else { kept }.to_string();
                let odd = kept.bytes().last().is_some_and(|digit| digit % 2 == 1);
                let rounds_up = match rest.as_bytes() {
                    [first, tail @ ..] => {
                        *first > b'5'
                            || (*first == b'5' && (tail.iter().any(|&digit| digit != b'0') || odd))
                    }
                    [] => false,
                };
                if rounds_up {
                    increment(&mut kept);
                }
                kept
            }
            None => "0".to_string(),
        }
    };
    let width = decimals as usize + 1;
    if units.len() < width {
        units.insert_str(0, &"0".repeat(width - units.len()));
    }
    let point = units.len() - decimals as usize;
    let negative = mantissa < 0 && units.bytes().any(|digit| digit != b'0');
    let sign = if negative { "-" } else { "" };
    if decimals == 0 {
        format!("{sign}{units}")
    } else {
        format!("{sign}{}.{}", &units[..point], &units[point..])
    }
}

/// Add one to the decimal digits `digits`, carrying into a new leading digit.
fn increment(digits: &mut String) {
    let mut bytes = std::mem::take(digits).into_bytes();
    let mut carry = true;
    for digit in bytes.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            carry = false;
            break;
        }
    }
    if carry {
        bytes.insert(0, b'1');
    }
    *digits = String::from_utf8(bytes).expect("ASCII digits stay ASCII");
}
