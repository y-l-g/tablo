//! How a cell presents its value: its [`Color`], its [`Alignment`], and the number and money
//! spellings of a [`Numeric`] value.

/// A semantic color for a badge, a status dot or a cell's text.
///
/// Each maps to a theme token (`--success`, `--warning`, `--info`, `--destructive`,
/// `--primary`, `--muted-foreground`), so a theme that redeclares the token recolors every cell
/// that uses it.
///
/// ```rust
/// #[derive(tablo_core::Options)]
/// enum Status {
///     #[option(color = "warning")]
///     Pending,
///     #[option(color = "success")]
///     Paid,
///     #[option(color = "danger")]
///     Refunded,
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Color {
    /// A neutral, muted status.
    #[default]
    Gray,
    /// The theme's primary color.
    Primary,
    /// Done, paid, active.
    Success,
    /// Pending, expiring, low.
    Warning,
    /// Failed, refunded, overdue.
    Danger,
    /// Informational, in progress.
    Info,
}

impl Color {
    /// The color a `#[option(color = "…")]` attribute names.
    #[doc(hidden)]
    pub const fn from_name(name: &str) -> Option<Self> {
        Some(match name.as_bytes() {
            b"gray" => Self::Gray,
            b"primary" => Self::Primary,
            b"success" => Self::Success,
            b"warning" => Self::Warning,
            b"danger" => Self::Danger,
            b"info" => Self::Info,
            _ => return None,
        })
    }

    /// The class of a badge's status dot in this color.
    pub(crate) const fn dot_class(self) -> &'static str {
        match self {
            Self::Gray => "size-1.5 shrink-0 rounded-full bg-muted-foreground/60",
            Self::Primary => "size-1.5 shrink-0 rounded-full bg-primary",
            Self::Success => "size-1.5 shrink-0 rounded-full bg-success",
            Self::Warning => "size-1.5 shrink-0 rounded-full bg-warning",
            Self::Danger => "size-1.5 shrink-0 rounded-full bg-destructive",
            Self::Info => "size-1.5 shrink-0 rounded-full bg-info",
        }
    }

    /// The class of a cell's text in this color.
    pub(crate) const fn text_class(self) -> &'static str {
        match self {
            Self::Gray => "text-muted-foreground",
            Self::Primary => "text-primary",
            Self::Success => "text-success",
            Self::Warning => "text-warning",
            Self::Danger => "text-destructive",
            Self::Info => "text-info",
        }
    }
}

/// Where a column's header and cells align their content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Alignment {
    /// The reading direction's start, the default.
    #[default]
    Start,
    /// The middle of the column.
    Center,
    /// The reading direction's end: numbers and money, so their digits line up.
    End,
}

impl Alignment {
    /// The text-alignment class of this alignment, empty at the start.
    pub(crate) const fn class(self) -> &'static str {
        match self {
            Self::Start => "",
            Self::Center => "text-center",
            Self::End => "text-end",
        }
    }
}

/// A number as a cell reads it: an integer and a decimal stay exact, a float is rounded when
/// spelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    /// Any integer type, widened.
    Int(i128),
    /// `f32` or `f64`.
    Float(f64),
    /// `mantissa / 10^scale`, exactly: a decimal type's value. Spelled with fewer digits than
    /// `scale`, it rounds half to even, as a float's spelling does.
    Decimal {
        /// The digits, with the sign.
        mantissa: i128,
        /// How many of the digits follow the point.
        scale: u32,
    },
}

/// A field a [`TextColumn`](crate::TextColumn) can spell as a number or an amount of money:
/// every integer and float type, `rust_decimal::Decimal` and `bigdecimal::BigDecimal` with the
/// features of those names, and an `Option` of one, which reads as the table's dash when `None`.
pub trait Numeric {
    /// The value, `None` when it holds none.
    fn number(&self) -> Option<Number>;
}

impl<T: Numeric + ?Sized> Numeric for &T {
    fn number(&self) -> Option<Number> {
        (**self).number()
    }
}

macro_rules! numeric_ints {
    ($($ty:ty),+) => {
        $(
            impl Numeric for $ty {
                fn number(&self) -> Option<Number> {
                    Some(Number::Int(i128::from(*self)))
                }
            }
        )+
    };
}

numeric_ints!(i8, i16, i32, i64, u8, u16, u32, u64);

impl Numeric for f32 {
    fn number(&self) -> Option<Number> {
        Some(Number::Float(f64::from(*self)))
    }
}

impl Numeric for f64 {
    fn number(&self) -> Option<Number> {
        Some(Number::Float(*self))
    }
}

#[cfg(feature = "rust_decimal")]
impl Numeric for rust_decimal::Decimal {
    fn number(&self) -> Option<Number> {
        Some(Number::Decimal {
            mantissa: self.mantissa(),
            scale: self.scale(),
        })
    }
}

/// A value of more than 38 significant digits, which no `i128` mantissa holds, is rounded to 38,
/// half to even; one whose integer part alone outgrows them reads as a float.
#[cfg(feature = "bigdecimal")]
impl Numeric for bigdecimal::BigDecimal {
    fn number(&self) -> Option<Number> {
        use bigdecimal::{RoundingMode, ToPrimitive};

        const DIGITS: u64 = 38;
        let exact = || {
            let (mantissa, scale) = match self.digits().checked_sub(DIGITS) {
                Some(excess @ 1..) => {
                    let (_, scale) = self.as_bigint_and_scale();
                    let scale = scale.checked_sub(i64::try_from(excess).ok()?)?;
                    self.with_scale_round(scale, RoundingMode::HalfEven)
                        .into_bigint_and_scale()
                }
                _ => self.clone().into_bigint_and_scale(),
            };
            let mantissa = mantissa.to_i128()?;
            match u32::try_from(scale) {
                Ok(scale) => Some(Number::Decimal { mantissa, scale }),
                Err(_) if scale < 0 => {
                    let shift = 10i128.checked_pow(u32::try_from(scale.unsigned_abs()).ok()?)?;
                    Some(Number::Decimal {
                        mantissa: mantissa.checked_mul(shift)?,
                        scale: 0,
                    })
                }
                Err(_) => None,
            }
        };
        exact().or_else(|| {
            self.to_f64()
                .filter(|float| float.is_finite())
                .map(Number::Float)
        })
    }
}

impl<T: Numeric> Numeric for Option<T> {
    fn number(&self) -> Option<Number> {
        self.as_ref().and_then(Numeric::number)
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
/// The separators are `,` and `.`: a locale's own spelling is the column's
/// [`format`](crate::TextColumn::format).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Currency {
    symbol: &'static str,
    decimals: u8,
    symbol_after: bool,
    minor_units: bool,
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
            symbol_after: false,
            minor_units: false,
        }
    }

    /// Spell `decimals` digits after the point.
    #[must_use]
    pub const fn decimals(mut self, decimals: u8) -> Self {
        self.decimals = decimals;
        self
    }

    /// Put the symbol after the amount, separated by a space.
    #[must_use]
    pub const fn symbol_after(mut self) -> Self {
        self.symbol_after = true;
        self
    }

    /// Read the field as minor units, cents for a two-decimal currency: `123456` is `1,234.56`.
    #[must_use]
    pub const fn minor_units(mut self) -> Self {
        self.minor_units = true;
        self
    }

    /// Spell `value` as an amount of this currency, or an empty string when it holds none.
    pub fn format(&self, value: impl Numeric) -> String {
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
        let (sign, digits) = match spelled.strip_prefix('-') {
            Some(digits) => ("-", digits),
            None => ("", spelled.as_str()),
        };
        if self.symbol_after {
            format!("{sign}{digits} {}", self.symbol)
        } else {
            format!("{sign}{}{digits}", self.symbol)
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

#[cfg(test)]
mod tests;
