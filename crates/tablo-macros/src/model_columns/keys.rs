//! A field's `#[columns(..)]` keys: `skip`, and the presentation keys that call a builder of the
//! field's column or register its filter.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned as _;

/// The keys `#[columns(..)]` takes, for the unknown-key error.
const KEYS: &str =
    "`skip`, `label = \"..\"`, `badge`, `money = USD`, `hidden_by_default` or `filter`";

/// The `#[columns(..)]` keys of one field, each with the span its errors point at.
#[derive(Default)]
pub struct ColumnKeys {
    /// `skip`: no column shows the field.
    pub skip: Option<Span>,
    /// `label = ".."`: the column's label, and its filter's.
    label: Option<syn::LitStr>,
    /// `badge`: the value as a badge.
    badge: Option<Span>,
    /// `money = USD`: the number as an amount of the currency the expression names.
    money: Option<(Span, syn::Expr)>,
    /// `hidden_by_default`: a table column each reader may show, hidden at first.
    hidden_by_default: Option<Span>,
    /// `filter`: the filter the field's type picks.
    filter: Option<Span>,
}

impl ColumnKeys {
    /// The keys of `field`'s `#[columns(..)]` attributes, refusing an unknown or repeated key and
    /// `skip` beside another key.
    pub fn parse(field: &syn::Field) -> syn::Result<Self> {
        let mut keys = Self::default();
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("columns"))
        {
            attr.parse_nested_meta(|meta| {
                let span = meta.path.span();
                let Some(name) = meta.path.get_ident().map(ToString::to_string) else {
                    return Err(
                        meta.error(format!("unknown `#[columns(..)]` key: expected {KEYS}"))
                    );
                };
                let taken = match name.as_str() {
                    "skip" => keys.skip.replace(span).is_some(),
                    "label" => keys.label.replace(meta.value()?.parse()?).is_some(),
                    "badge" => keys.badge.replace(span).is_some(),
                    "money" => {
                        let currency = meta
                            .value()
                            .map_err(|_| meta.error("`money` names its currency: `money = USD`"))?;
                        keys.money.replace((span, currency.parse()?)).is_some()
                    }
                    "hidden_by_default" => keys.hidden_by_default.replace(span).is_some(),
                    "filter" => keys.filter.replace(span).is_some(),
                    _ => {
                        return Err(
                            meta.error(format!("unknown `#[columns(..)]` key: expected {KEYS}"))
                        );
                    }
                };
                if taken {
                    return Err(meta.error(format!("`{name}` is given twice")));
                }
                Ok(())
            })?;
        }
        if let (Some(skip), Some(other)) = (keys.skip, keys.presentation()) {
            let mut error = syn::Error::new(
                other,
                "`skip` leaves the field out of every column: drop this key or `skip`",
            );
            error.combine(syn::Error::new(skip, "the field is skipped here"));
            return Err(error);
        }
        Ok(keys)
    }

    /// The span of the first presentation key, if any.
    pub fn presentation(&self) -> Option<Span> {
        [
            self.label.as_ref().map(syn::LitStr::span),
            self.badge,
            self.money.as_ref().map(|(span, _)| *span),
            self.hidden_by_default,
            self.filter,
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// The builder calls the keys make on the field's `__column`, on the table or on the detail
    /// page, each spanned at its key.
    pub fn column_calls(&self, krate: &TokenStream2, table: bool) -> TokenStream2 {
        let pick = quote!(#krate::__macro::pick);
        let mut calls = TokenStream2::new();
        if let Some(label) = &self.label {
            calls.extend(quote_spanned! {label.span()=>
                let __column = #pick::Label::label(__column, ::core::convert::Into::into(#label));
            });
        }
        if let Some(span) = self.badge {
            calls.extend(quote_spanned! {span=>
                let __column = #pick::Badge::badge(__column);
            });
        }
        if let Some((span, currency)) = &self.money {
            let currency = currency_expr(krate, currency);
            calls.extend(quote_spanned! {*span=>
                let __column = #pick::Money::money(__column, #currency);
            });
        }
        if let Some(span) = self.hidden_by_default.filter(|_| table) {
            calls.extend(quote_spanned! {span=>
                let __column = #pick::HiddenByDefault::hidden_by_default(__column);
            });
        }
        calls
    }

    /// The filter `filter` registers on `__table` from the field's `__field`, labelled as the
    /// column is.
    pub fn filter(&self, krate: &TokenStream2) -> TokenStream2 {
        let Some(span) = self.filter else {
            return TokenStream2::new();
        };
        let pick = quote!(#krate::__macro::pick);
        let label = self.label.as_ref().map(|label| {
            quote_spanned! {label.span()=>
                let __filter = #pick::Label::label(__filter, ::core::convert::Into::into(#label));
            }
        });
        quote_spanned! {span=>
            let __filter = (&&&&__field).table_filter();
            #label
            let __table = #pick::PlaceFilter::place_filter(__filter, __table);
        }
    }
}

/// The `Currency` `money = ..` names: a bare name is one of `Currency`'s constants, as in
/// `money = EUR`; any other expression is a `Currency` itself.
fn currency_expr(krate: &TokenStream2, currency: &syn::Expr) -> TokenStream2 {
    match currency {
        syn::Expr::Path(path) if path.qself.is_none() && path.path.get_ident().is_some() => {
            let name = path.path.get_ident();
            quote_spanned!(currency.span()=> #krate::Currency::#name)
        }
        _ => quote!(#currency),
    }
}
