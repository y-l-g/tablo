//! What a builder's `default` and `options` take.

use crate::{
    Text,
    values::{FormScalar, FormValue},
};

/// A value a field's `default` takes: any [`FormScalar`] or a `&str` or `&String` in the form
/// spelling, or an array or `Vec` of them for a multiple choice.
pub trait IntoFormValue {
    /// The value as its control posts it.
    fn into_form_value(self) -> FormValue;
}

impl<T: FormScalar> IntoFormValue for T {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.to_form())
    }
}

impl IntoFormValue for &str {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.to_string())
    }
}

impl IntoFormValue for &String {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.clone())
    }
}

impl<T: FormScalar> IntoFormValue for Vec<T> {
    fn into_form_value(self) -> FormValue {
        FormValue::List(self.iter().map(FormScalar::to_form).collect())
    }
}

impl IntoFormValue for Vec<&str> {
    fn into_form_value(self) -> FormValue {
        FormValue::List(self.into_iter().map(str::to_string).collect())
    }
}

impl<T: FormScalar, const N: usize> IntoFormValue for [T; N] {
    fn into_form_value(self) -> FormValue {
        Vec::from(self).into_form_value()
    }
}

impl<const N: usize> IntoFormValue for [&str; N] {
    fn into_form_value(self) -> FormValue {
        Vec::from(self).into_form_value()
    }
}

/// A choice's static options as `(value, label)` pairs, in display order. A value listed alone
/// is its own label; a label is a [`Text`], so a message translates it.
pub trait IntoOptions {
    fn into_options(self) -> Vec<(String, Text)>;
}

impl IntoOptions for Vec<String> {
    fn into_options(self) -> Vec<(String, Text)> {
        self.into_iter()
            .map(|value| (value.clone(), Text::from(value)))
            .collect()
    }
}

impl<const N: usize> IntoOptions for [&str; N] {
    fn into_options(self) -> Vec<(String, Text)> {
        self.into_iter()
            .map(|value| (value.to_string(), Text::from(value)))
            .collect()
    }
}

impl<L: Into<Text>> IntoOptions for Vec<(String, L)> {
    fn into_options(self) -> Vec<(String, Text)> {
        self.into_iter()
            .map(|(value, label)| (value, label.into()))
            .collect()
    }
}

impl<L: Into<Text>, const N: usize> IntoOptions for [(&str, L); N] {
    fn into_options(self) -> Vec<(String, Text)> {
        self.into_iter()
            .map(|(value, label)| (value.to_string(), label.into()))
            .collect()
    }
}
