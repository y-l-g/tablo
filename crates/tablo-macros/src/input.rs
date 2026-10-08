//! `#[derive(ActionInput)]` declares the typed value an action asks for before it runs, and
//! `#[derive(RepeaterItem)]` one item of a repeater: both post each field under its own name.
//!
//! See `tablo-core`'s `resource::action` and `schema::repeater` modules for the contracts.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, Type, spanned::Spanned};

use crate::fields::{
    Derive, FormAttrs, assert_scalar, blank_answer, blank_option, form_attrs, last_segment,
};

/// Which trait the derive implements.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// `ActionInput`.
    Input,
    /// `RepeaterItem`, which also writes an item back as its row posts it.
    Item,
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::Input => "ActionInput",
            Self::Item => "RepeaterItem",
        }
    }

    fn derive(self) -> Derive {
        match self {
            Self::Input => Derive::Input,
            Self::Item => Derive::Item,
        }
    }
}

pub fn expand_tokens(input: DeriveInput, target: Target) -> TokenStream2 {
    match expand_checked(input, target) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

struct FieldSpec {
    ident: syn::Ident,
    ty: Type,
    attrs: FormAttrs,
}

fn expand_checked(input: DeriveInput, target: Target) -> syn::Result<TokenStream2> {
    let derive = target.name();
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            format!("#[derive({derive})] does not support generic or lifetime parameters"),
        ));
    }
    let shape = || {
        syn::Error::new_spanned(
            &input.ident,
            format!("#[derive({derive})] supports a struct with named fields"),
        )
    };
    let named = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) if !named.named.is_empty() => named,
            Fields::Named(_) => {
                let hint = match target {
                    Target::Input => ": an action that asks for nothing names `type Input = ();`",
                    Target::Item => "",
                };
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    format!("#[derive({derive})] needs at least one field{hint}"),
                ));
            }
            _ => return Err(shape()),
        },
        _ => return Err(shape()),
    };
    let fields = named
        .named
        .iter()
        .map(|field| {
            Ok(FieldSpec {
                ident: field.ident.clone().expect("named field"),
                ty: field.ty.clone(),
                attrs: input_attrs(field, target)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let krate = crate::tablo_core_path(&input.ident, derive)?;
    Ok(expand_struct(&krate, &input.ident, &fields, target))
}

/// The field's `#[form(..)]` keys, refusing a text input's on a `bool`, which renders a checkbox.
fn input_attrs(field: &syn::Field, target: Target) -> syn::Result<FormAttrs> {
    let attrs = form_attrs(field, target.derive())?;
    if last_segment(&field.ty).as_deref() == Some("bool") && attrs.options.is_none() {
        let text_only = [
            (attrs.multiline.is_some(), "`multiline`"),
            (attrs.placeholder.is_some(), "`placeholder`"),
        ];
        if let Some((_, key)) = text_only.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                &field.ty,
                format!("{key} applies to a text input, and a `bool` renders a checkbox"),
            ));
        }
    }
    Ok(attrs)
}

fn expand_struct(
    krate: &TokenStream2,
    ident: &syn::Ident,
    fields: &[FieldSpec],
    target: Target,
) -> TokenStream2 {
    let mut controls = Vec::new();
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for field in fields {
        let ty = &field.ty;
        let attrs = &field.attrs;
        let key = field.ident.to_string();
        let key = key.trim_start_matches("r#");
        let binding = format_ident!("__read_{}", field.ident);
        let is_bool = last_segment(ty).as_deref() == Some("bool");
        let mut control = match &attrs.options {
            Some(options) => {
                let options = options.as_ref().unwrap_or(ty);
                quote! {
                    #krate::__macro::Field::choice_input(#key)
                        .options(<#options as #krate::__macro::Options>::options())
                }
            }
            None if is_bool => quote! { #krate::__macro::Field::toggle_input(#key) },
            None => quote_spanned! {ty.span()=>
                #krate::__macro::Field::text_input::<#ty>(#key)
            },
        };
        if let Some(rows) = attrs.multiline {
            let rows = proc_macro2::Literal::u32_unsuffixed(rows);
            control = quote! { #control.multiline(#rows) };
        }
        if let Some(placeholder) = &attrs.placeholder {
            control = quote! { #control.placeholder(#placeholder) };
        }
        if let Some(label) = &attrs.label {
            control = quote! { #control.label(#label) };
        }
        let control = if blank_answer(ty, attrs).is_none() {
            quote! { #krate::__macro::required_input(#control) }
        } else {
            quote! { #krate::__macro::Field::from(#control) }
        };
        let assert = assert_scalar(krate, ty);
        controls.push(quote! {
            {
                #assert
                #control
            }
        });
        let name = &field.ident;
        writes.push(quote_spanned! {ty.span()=>
            out.insert(
                ::std::string::String::from(#key),
                #krate::__macro::FormScalar::to_form(&self.#name),
            );
        });
        let blank = blank_option(ty, attrs);
        reads.push(quote_spanned! {ty.span()=>
            let #binding = #krate::__macro::take_leaf(
                #krate::__macro::parse_scalar::<#ty>(#key, values, #blank),
                &mut errors,
            );
        });
    }
    let names: Vec<&syn::Ident> = fields.iter().map(|f| &f.ident).collect();
    let bindings: Vec<syn::Ident> = names
        .iter()
        .map(|name| format_ident!("__read_{}", name))
        .collect();
    let (trait_name, write) = match target {
        Target::Input => (quote! { ActionInput }, quote! {}),
        Target::Item => (
            quote! { RepeaterItem },
            quote! {
                fn write(
                    &self,
                    out: &mut ::std::collections::HashMap<
                        ::std::string::String,
                        ::std::string::String,
                    >,
                ) {
                    #(#writes)*
                }
            },
        ),
    };
    quote! {
        impl #krate::__macro::#trait_name for #ident {
            fn schema() -> #krate::__macro::Schema {
                #krate::__macro::Schema::empty()
                    #(.extend(#krate::__macro::IntoSchema::into_schema(#controls)))*
            }

            fn parse(
                cx: &#krate::__macro::Cx,
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) -> ::std::result::Result<Self, ::std::vec::Vec<#krate::__macro::FieldError>> {
                let _ = cx;
                let mut errors: ::std::vec::Vec<#krate::__macro::FieldError> = ::std::vec::Vec::new();
                #(#reads)*
                match (#(#bindings,)*) {
                    (#(::std::option::Option::Some(#bindings),)*) if errors.is_empty() => {
                        ::std::result::Result::Ok(Self { #(#names: #bindings),* })
                    }
                    _ => ::std::result::Result::Err(errors),
                }
            }

            #write
        }
    }
}

#[cfg(test)]
mod tests;
