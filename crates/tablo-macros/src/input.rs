//! `#[derive(FormInput)]` declares a typed value a form posts under its own keys: what an action
//! asks for, what a sign-up reads, one item of a repeater. Each field posts under its own name.
//!
//! See `tablo-core`'s `form::input` module for the contract.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{DeriveInput, Type, spanned::Spanned};

use crate::fields::{
    Derive, FormAttrs, assert_scalar, blank_answer, blank_option, form_attrs, last_segment,
    named_fields, reject_generics,
};

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

struct FieldSpec {
    ident: syn::Ident,
    ty: Type,
    attrs: FormAttrs,
}

fn expand_checked(input: DeriveInput) -> syn::Result<TokenStream2> {
    let derive = "FormInput";
    reject_generics(
        &input,
        &format!("#[derive({derive})] does not support generic or lifetime parameters"),
    )?;
    let shape = format!("#[derive({derive})] supports a struct with named fields");
    let empty = format!("#[derive({derive})] needs at least one field: `()` asks for nothing");
    let named = named_fields(&input, &shape, Some(empty.as_str()))?;
    let fields = named
        .named
        .iter()
        .map(|field| {
            Ok(FieldSpec {
                ident: field.ident.clone().expect("named field"),
                ty: field.ty.clone(),
                attrs: input_attrs(field)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let krate = crate::tablo_core_path(&input.ident, derive)?;
    Ok(expand_struct(&krate, &input.ident, &fields))
}

/// The field's `#[form(..)]` keys, refusing a text input's on a `bool`, which renders a checkbox.
fn input_attrs(field: &syn::Field) -> syn::Result<FormAttrs> {
    let attrs = form_attrs(field, Derive::Input)?;
    if last_segment(&field.ty).as_deref() == Some("bool") && attrs.options.is_none() {
        let text_only = [
            (attrs.multiline.is_some(), "`multiline`"),
            (attrs.placeholder.is_some(), "`placeholder`"),
            (attrs.email, "`email`"),
            (attrs.password, "`password`"),
        ];
        if let Some((_, key)) = text_only.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                &field.ty,
                format!("{key} applies to a text input, and a `bool` renders a checkbox"),
            ));
        }
    }
    if attrs.password && last_segment(&field.ty).as_deref() != Some("String") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "`password` reads a `String`, kept as typed",
        ));
    }
    if attrs.password && attrs.multiline.is_some() {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "`password` masks a one-line input, and `multiline` makes a `<textarea>`: declare one",
        ));
    }
    Ok(attrs)
}

fn expand_struct(krate: &TokenStream2, ident: &syn::Ident, fields: &[FieldSpec]) -> TokenStream2 {
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
        if attrs.email {
            control = quote! { #control.email() };
        }
        if attrs.password {
            control = quote! { #control.password() };
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
        // A password is a secret: it never renders back into a form.
        if !attrs.password {
            let name = &field.ident;
            writes.push(quote_spanned! {ty.span()=>
                out.insert(
                    ::std::string::String::from(#key),
                    #krate::__macro::FormScalar::to_form(&self.#name),
                );
            });
        }
        let blank = blank_option(ty, attrs);
        // A password keeps its outer spaces: the login compares it as typed.
        let parse = if attrs.password {
            quote_spanned! {ty.span()=> #krate::__macro::parse_password(#key, values, #blank) }
        } else {
            quote_spanned! {ty.span()=> #krate::__macro::parse_scalar::<#ty>(#key, values, #blank) }
        };
        reads.push(quote_spanned! {ty.span()=>
            let #binding = #krate::__macro::take_leaf(#parse, &mut errors);
        });
    }
    let names: Vec<&syn::Ident> = fields.iter().map(|f| &f.ident).collect();
    let bindings: Vec<syn::Ident> = names
        .iter()
        .map(|name| format_ident!("__read_{}", name))
        .collect();
    quote! {
        impl #krate::__macro::FormInput for #ident {
            fn schema() -> #krate::__macro::Schema {
                #krate::__macro::Schema::empty()
                    #(.extend(#krate::__macro::IntoSchema::into_schema(#controls)))*
            }

            fn parse(
                cx: &#krate::__macro::Cx,
                values: &#krate::__macro::FormValues,
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

            fn write(&self, out: &mut #krate::__macro::FormValues) {
                let _ = &out;
                #(#writes)*
            }
        }
    }
}

#[cfg(test)]
mod tests;
