//! [`FormScalar`]: a type one form key reads and writes, and the blanket impls over
//! [`TypedValue`] and `Option`.

use jiff::tz::TimeZone;

use super::TypedValue;

/// A type one form key reads and writes: `String`, every [`TypedValue`] type,
/// every enum deriving [`Options`](crate::Options), and an `Option` of any of them.
///
/// The value the parse sees is trimmed and non-empty; an empty submission is the
/// record-form field's **blank answer** instead, and a field with none is required.
///
/// A text field binds a path of any `FormScalar` type
/// ([`Field::text`](crate::Field::text)), and the form derives read and write
/// every field that is not `#[form(embed)]` through it.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a form scalar",
    label = "a form field of this type has no text spelling",
    note = "a form scalar is `String`, a `TypedValue` type, an `Options` enum, or an `Option` of \
            one; implement `TypedValue` for an app type, derive `Options` for a unit enum, or \
            mark an `EmbeddedForm` value `#[form(embed)]`"
)]
pub trait FormScalar: Sized {
    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

    /// The control's `step` attribute, when its input type takes one.
    const STEP: Option<&'static str> = None;

    /// Whether the database orders the stored column as the values order: a [`TypedValue`]'s
    /// [`SORTABLE`](TypedValue::SORTABLE), and `true` for any other type.
    const SORTABLE: bool = true;

    /// The control's `value` for a stored value's form spelling, shown in the request's time zone
    /// `zone`: a [`TypedValue`]'s [`input_value`](TypedValue::input_value), and the spelling
    /// itself for any other type.
    fn input_value(stored: &str, zone: &TimeZone) -> String {
        let _ = zone;
        stored.to_string()
    }

    /// The form spelling of what the control posted, read in the request's time zone `zone`: a
    /// [`TypedValue`]'s [`form_value`](TypedValue::form_value), and the posted text itself for
    /// any other type.
    fn form_value(posted: &str, zone: &TimeZone) -> String {
        let _ = zone;
        posted.to_string()
    }

    /// Parse a trimmed, non-empty submission, or return the error message.
    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text>;

    /// The form spelling of a stored value: a [`TypedValue`]'s
    /// [`form_spelling`](TypedValue::form_spelling).
    fn to_form(&self) -> String;

    /// What a column cell and a group header read: the form spelling, or an
    /// `Options` enum's label.
    fn to_label(&self) -> crate::Text {
        self.to_form().into()
    }

    /// The color a badge shows the value in: an `Options` enum's `#[option(color = "…")]`, and
    /// `None` for any other type.
    fn color(&self) -> Option<crate::Color> {
        None
    }

    /// The icon a column shows the value with: an `Options` enum's `#[option(icon = …)]`, and
    /// `None` for any other type.
    fn icon(&self) -> Option<topcoat::icon::IconData> {
        None
    }

    /// How `self` orders against `other` for a text field's [`min`](crate::TextField::min) and
    /// [`max`](crate::TextField::max): a [`TypedValue`]'s [`order`](TypedValue::order), and
    /// `None` for any other type.
    fn order(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        None
    }

    /// The type a text field's comparison with another field orders by: `Self`, and `T` for an
    /// `Option<T>`, so an optional end date compares with a required start date.
    #[doc(hidden)]
    fn compared_as() -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl FormScalar for String {
    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text> {
        Ok(value.to_string())
    }

    fn to_form(&self) -> String {
        self.clone()
    }
}

impl<T: TypedValue> FormScalar for T {
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;
    const STEP: Option<&'static str> = T::STEP;
    const SORTABLE: bool = T::SORTABLE;

    fn input_value(stored: &str, zone: &TimeZone) -> String {
        T::input_value(stored, zone)
    }

    fn form_value(posted: &str, zone: &TimeZone) -> String {
        T::form_value(posted, zone)
    }

    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text> {
        T::parse_input(value).ok_or_else(|| {
            crate::text!(
                "tablo-field-not-a",
                value = value,
                kind = crate::Text::message(T::NOUN)
            )
        })
    }

    fn to_form(&self) -> String {
        self.form_spelling()
    }

    fn order(&self, other: &Self) -> Option<std::cmp::Ordering> {
        TypedValue::order(self, other)
    }
}

/// A form scalar whose `Option` is one too: every [`TypedValue`] type, and every enum deriving
/// [`Options`](crate::Options), which implements it.
#[doc(hidden)]
pub trait NullableScalar: FormScalar {}

impl<T: TypedValue> NullableScalar for T {}

impl<T: NullableScalar> FormScalar for Option<T> {
    const INPUT_TYPE: &'static str = T::INPUT_TYPE;
    const STEP: Option<&'static str> = T::STEP;
    const SORTABLE: bool = T::SORTABLE;

    fn input_value(stored: &str, zone: &TimeZone) -> String {
        T::input_value(stored, zone)
    }

    fn form_value(posted: &str, zone: &TimeZone) -> String {
        T::form_value(posted, zone)
    }

    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text> {
        T::parse_form(value).map(Some)
    }

    fn to_form(&self) -> String {
        self.as_ref().map(T::to_form).unwrap_or_default()
    }

    fn to_label(&self) -> crate::Text {
        self.as_ref().map(T::to_label).unwrap_or_default()
    }

    fn color(&self) -> Option<crate::Color> {
        self.as_ref().and_then(T::color)
    }

    fn icon(&self) -> Option<topcoat::icon::IconData> {
        self.as_ref().and_then(T::icon)
    }

    fn order(&self, other: &Self) -> Option<std::cmp::Ordering> {
        T::order(self.as_ref()?, other.as_ref()?)
    }

    fn compared_as() -> &'static str {
        T::compared_as()
    }
}

impl FormScalar for Option<String> {
    fn parse_form(value: &str) -> std::result::Result<Self, crate::Text> {
        Ok(Some(value.to_string()))
    }

    fn compared_as() -> &'static str {
        String::compared_as()
    }

    fn to_form(&self) -> String {
        self.clone().unwrap_or_default()
    }
}

/// Compiles only for a form scalar: the form derives call it, spanned on a
/// field's type, so a field of another type fails there.
#[doc(hidden)]
pub fn assert_form_scalar<T: FormScalar>() {}
