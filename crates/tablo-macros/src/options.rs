//! `#[derive(Options)]` — a unit-variant enum as a choice's options.
//!
//! Each variant's value is its `snake_case` name and its label that name in
//! sentence case; `#[option(value = "..", label = "..")]` overrides either, and
//! `#[option(color = "..")]` gives it the color a badge shows it in.
//! The derive implements `Options` and `FormScalar`, spelling a variant as
//! its value and reading it as its label, and gives the enum `value()`,
//! `label()` and `from_value()`.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields};

use crate::fields::{reject_generics, sentence_case};

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(&input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

struct Choice {
    ident: syn::Ident,
    value: String,
    label: String,
    /// The `Color` variant `color = ".."` names.
    color: Option<syn::Ident>,
}

/// The names `#[option(color = "..")]` takes, and the `Color` variant each names.
const COLORS: &[(&str, &str)] = &[
    ("gray", "Gray"),
    ("primary", "Primary"),
    ("success", "Success"),
    ("warning", "Warning"),
    ("danger", "Danger"),
    ("info", "Info"),
];

fn expand_checked(input: &DeriveInput) -> syn::Result<TokenStream2> {
    reject_generics(
        input,
        "#[derive(Options)] does not support generic or lifetime parameters",
    )?;
    let Data::Enum(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(Options)] supports an enum of unit variants",
        ));
    };
    if data.variants.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(Options)] needs at least one variant",
        ));
    }
    let mut choices = Vec::with_capacity(data.variants.len());
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "#[derive(Options)] supports unit variants only: an option is one stored value",
            ));
        }
        let name = variant.ident.to_string();
        let mut value = snake_case(&name);
        let mut label = sentence_case(&value);
        let mut color = None;
        for attr in &variant.attrs {
            if !attr.path().is_ident("option") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("value") {
                    value = meta.value()?.parse::<syn::LitStr>()?.value();
                } else if meta.path.is_ident("label") {
                    label = meta.value()?.parse::<syn::LitStr>()?.value();
                } else if meta.path.is_ident("color") {
                    let name = meta.value()?.parse::<syn::LitStr>()?;
                    let Some((_, variant)) = COLORS.iter().find(|(n, _)| *n == name.value()) else {
                        let names: Vec<&str> = COLORS.iter().map(|(n, _)| *n).collect();
                        return Err(syn::Error::new_spanned(
                            &name,
                            format!("unknown color: expected one of {}", names.join(", ")),
                        ));
                    };
                    color = Some(syn::Ident::new(variant, name.span()));
                } else {
                    return Err(meta.error(
                        "unknown `#[option(..)]` key: expected `value = \"…\"`, `label = \"…\"` \
                         or `color = \"…\"`",
                    ));
                }
                Ok(())
            })?;
        }
        // A form posts the value trimmed, and a blank one as no choice: neither could select it.
        if value.trim().is_empty() || value.trim() != value {
            return Err(syn::Error::new_spanned(
                variant,
                format!(
                    "`{}` stores \"{value}\": an option's value needs a character other than a \
                     space, and no space at either end",
                    variant.ident
                ),
            ));
        }
        if let Some(earlier) = choices.iter().find(|c: &&Choice| c.value == value) {
            return Err(syn::Error::new_spanned(
                variant,
                format!(
                    "`{}` and `{}` both store \"{value}\": each option needs a distinct value",
                    earlier.ident, variant.ident
                ),
            ));
        }
        if let Some(earlier) = choices.iter().find(|c: &&Choice| c.label == label) {
            return Err(syn::Error::new_spanned(
                variant,
                format!(
                    "`{}` and `{}` both read as \"{label}\": a select, a column and a group \
                     header could not tell them apart",
                    earlier.ident, variant.ident
                ),
            ));
        }
        choices.push(Choice {
            ident: variant.ident.clone(),
            value,
            label,
            color,
        });
    }
    let krate = crate::tablo_core_path(&input.ident, "Options")?;
    let ident = &input.ident;
    let idents: Vec<&syn::Ident> = choices.iter().map(|c| &c.ident).collect();
    let values: Vec<&str> = choices.iter().map(|c| c.value.as_str()).collect();
    let labels: Vec<&str> = choices.iter().map(|c| c.label.as_str()).collect();
    // Only an enum that names a color overrides the trait's `None`.
    let color = choices.iter().any(|c| c.color.is_some()).then(|| {
        let colors = choices.iter().map(|c| match &c.color {
            Some(variant) => {
                quote! { ::std::option::Option::Some(#krate::__macro::Color::#variant) }
            }
            None => quote! { ::std::option::Option::None },
        });
        quote! {
            fn color(&self) -> ::std::option::Option<#krate::__macro::Color> {
                match self {
                    #(Self::#idents => #colors,)*
                }
            }
        }
    });
    Ok(quote! {
        impl #krate::__macro::Options for #ident {
            fn options() -> ::std::vec::Vec<(::std::string::String, ::std::string::String)> {
                ::std::vec![
                    #((
                        ::std::string::String::from(#values),
                        ::std::string::String::from(#labels),
                    )),*
                ]
            }
        }

        impl #krate::__macro::FormScalar for #ident {
            fn parse_form(
                value: &str,
            ) -> ::std::result::Result<Self, ::std::string::String> {
                Self::from_value(value).ok_or_else(|| {
                    ::std::format!("`{value}` is not a valid option")
                })
            }

            fn to_form(&self) -> ::std::string::String {
                ::std::string::String::from(self.value())
            }

            fn to_label(&self) -> ::std::string::String {
                ::std::string::String::from(self.label())
            }

            #color
        }

        impl #krate::__macro::NullableScalar for #ident {}

        impl #ident {
            /// The value this option posts, and a `String` column stores.
            pub const fn value(&self) -> &'static str {
                match self {
                    #(Self::#idents => #values,)*
                }
            }

            /// The label this option reads as.
            pub const fn label(&self) -> &'static str {
                match self {
                    #(Self::#idents => #labels,)*
                }
            }

            /// The option storing `value`, if any.
            pub fn from_value(value: &str) -> ::std::option::Option<Self> {
                match value {
                    #(#values => ::std::option::Option::Some(Self::#idents),)*
                    _ => ::std::option::Option::None,
                }
            }
        }
    })
}

/// `PublishedLate` → `published_late`.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.trim_start_matches("r#").chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests;
