//! The words a panel renders, in the request's locale.
//!
//! A [`Text`] is either plain, rendering as written, or a message: an id that each locale's
//! catalog spells, with the arguments its placeables read. Tablo ships a catalog per locale it
//! translates, and an app adds its own with [`Panel::translations`](crate::Panel::translations),
//! whose messages override Tablo's. The request reads them in its [`locale`].
//!
//! The module sits below every module that renders a word, and reads the request's panel through
//! the [`LocaleSource`] the serving layer installs, as `values` reads the time zone.

mod catalog;
mod locale;
mod text;

pub(crate) use catalog::Catalog;
pub(crate) use locale::{AcceptedLocale, LocaleSource, addressed_to, catalog_and_index};
pub use locale::{Locale, locale};
pub use text::Text;

#[cfg(test)]
mod tests;
