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

    /// The name a `#[option(color = "…")]` attribute gives this color.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Primary => "primary",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
            Self::Info => "info",
        }
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

    /// The class of a table row tinted in this color.
    pub(crate) const fn row_class(self) -> &'static str {
        match self {
            Self::Gray => "bg-muted/50",
            Self::Primary => "bg-primary/5",
            Self::Success => "bg-success/10",
            Self::Warning => "bg-warning/10",
            Self::Danger => "bg-destructive/10",
            Self::Info => "bg-info/10",
        }
    }

    /// The classes that tint a button in this color, `None` for the neutral button. They are
    /// marked important to win over the outline button's own colors.
    pub(crate) const fn button_tone(self) -> Option<&'static str> {
        match self {
            Self::Gray => None,
            Self::Primary => Some("text-primary! hover:bg-primary/5! active:bg-primary/10!"),
            Self::Success => Some("text-success! hover:bg-success/5! active:bg-success/10!"),
            Self::Warning => Some("text-warning! hover:bg-warning/5! active:bg-warning/10!"),
            Self::Danger => {
                Some("text-destructive! hover:bg-destructive/5! active:bg-destructive/10!")
            }
            Self::Info => Some("text-info! hover:bg-info/5! active:bg-info/10!"),
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

/// How heavy a column's text renders.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontWeight {
    /// Lighter than the body text.
    Light,
    /// The body text's weight, the default.
    #[default]
    Normal,
    /// Between normal and semibold.
    Medium,
    /// Between medium and bold.
    Semibold,
    /// The heaviest.
    Bold,
}

impl FontWeight {
    /// The font-weight class of this weight, empty at the body text's.
    pub(crate) const fn class(self) -> &'static str {
        match self {
            Self::Light => "font-light",
            Self::Normal => "",
            Self::Medium => "font-medium",
            Self::Semibold => "font-semibold",
            Self::Bold => "font-bold",
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
///
/// Equality compares the representation, not the amount: `Decimal { mantissa: 1250, scale: 2 }`
/// differs from `Decimal { mantissa: 125, scale: 1 }`, and `Int(12)` from
/// `Decimal { mantissa: 12, scale: 0 }`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
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

mod number;

pub use number::Currency;
pub(crate) use number::NumberFormat;
#[cfg(test)]
use number::format_number;

#[cfg(test)]
mod tests;
