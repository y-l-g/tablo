//! A text field's declared rules: its length, its bounds and its pattern. Each renders as the
//! control's HTML attribute of the same name, and the server checks it again, since a browser's
//! check is only a convenience.

use std::cmp::Ordering;

use regex::Regex;

use crate::{DeclarationErrorKind, form::FormScalar};

/// How a field's type orders two submissions, `None` when it parses neither or has no order.
pub(super) type Order = fn(&str, &str) -> Option<Ordering>;

/// [`FormScalar::order`] over two submissions of `T`.
pub(super) fn order<T: FormScalar>(a: &str, b: &str) -> Option<Ordering> {
    let a = T::parse_form(a.trim()).ok()?;
    let b = T::parse_form(b.trim()).ok()?;
    a.order(&b)
}

/// The rules [`TextField`](crate::TextField) declares, all unset by default.
#[derive(Default)]
pub(crate) struct Rules {
    /// The fewest characters a value holds: `minlength`.
    pub(crate) min_length: Option<usize>,
    /// The most characters a value holds: `maxlength`.
    pub(crate) max_length: Option<usize>,
    /// The smallest value, inclusive, in its form spelling: `min`.
    pub(crate) min: Option<String>,
    /// The largest value, inclusive, in its form spelling: `max`.
    pub(crate) max: Option<String>,
    /// The expression a whole value matches: `pattern`.
    pub(crate) pattern: Option<Pattern>,
}

/// A declared pattern: its source, as the `pattern` attribute carries it, and the expression the
/// server matches.
pub(crate) struct Pattern {
    pub(crate) source: String,
    /// The source anchored to the whole value, as a browser anchors it, or why it does not
    /// compile.
    regex: Result<Regex, String>,
}

impl Pattern {
    /// Compiles `source` anchored to the whole value.
    pub(crate) fn new(source: String) -> Self {
        let regex = Regex::new(&format!("^(?:{source})$")).map_err(|error| error.to_string());
        Self { source, regex }
    }
}

impl Rules {
    /// What is wrong with the rules field `field` declares: a bound its type does not parse or
    /// does not order, a pattern that does not compile, or a range no value fits.
    pub(crate) fn declaration_errors(
        &self,
        field: &str,
        order: Order,
    ) -> Vec<DeclarationErrorKind> {
        let mut errors = Vec::new();
        for bound in [&self.min, &self.max].into_iter().flatten() {
            if order(bound, bound).is_none() {
                errors.push(DeclarationErrorKind::UnorderedBound {
                    field: field.to_string(),
                    bound: bound.clone(),
                });
            }
        }
        if let Some(Pattern {
            regex: Err(error), ..
        }) = &self.pattern
        {
            errors.push(DeclarationErrorKind::InvalidPattern {
                field: field.to_string(),
                error: error.clone(),
            });
        }
        let lengths_cross =
            matches!((self.min_length, self.max_length), (Some(min), Some(max)) if min > max);
        let bounds_cross = match (&self.min, &self.max) {
            (Some(min), Some(max)) => order(min, max) == Some(Ordering::Greater),
            _ => false,
        };
        if lengths_cross || bounds_cross {
            errors.push(DeclarationErrorKind::EmptyRange {
                field: field.to_string(),
            });
        }
        errors
    }

    /// The sentence refusing `value`, a trimmed, non-empty submission its type parses, for the
    /// first rule it breaks; `None` when it keeps them all.
    pub(crate) fn refusal(&self, label: &str, value: &str, order: Order) -> Option<String> {
        let length = value.chars().count();
        if let Some(min) = self.min_length
            && length < min
        {
            return Some(format!("{label} must be at least {min} characters"));
        }
        if let Some(max) = self.max_length
            && length > max
        {
            return Some(format!("{label} must be at most {max} characters"));
        }
        if let Some(min) = &self.min
            && order(value, min) == Some(Ordering::Less)
        {
            return Some(format!("{label} must be at least {min}"));
        }
        if let Some(max) = &self.max
            && order(value, max) == Some(Ordering::Greater)
        {
            return Some(format!("{label} must be at most {max}"));
        }
        if let Some(Pattern {
            regex: Ok(regex), ..
        }) = &self.pattern
            && !regex.is_match(value)
        {
            return Some(format!("{label} is not in the expected format"));
        }
        None
    }
}
