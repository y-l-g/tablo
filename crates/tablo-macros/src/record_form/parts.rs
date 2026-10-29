//! The tokens each field of a record form adds to the derive's generated methods.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;

use super::{DefaultControl, FieldSpec};
use crate::fields::assert_scalar;

/// The tokens each generated method holds for the fields, in declaration order.
#[derive(Default)]
pub(super) struct Parts {
    pub(super) claims: Vec<TokenStream2>,
    pub(super) hydrates: Vec<TokenStream2>,
    pub(super) reads: Vec<TokenStream2>,
    pub(super) creates: Vec<TokenStream2>,
    pub(super) updates: Vec<TokenStream2>,
    pub(super) asserts: Vec<TokenStream2>,
    /// A many-to-many field's `links` entry, variant and include.
    pub(super) links: Vec<TokenStream2>,
    pub(super) link_variants: Vec<TokenStream2>,
    pub(super) includes: Vec<TokenStream2>,
}

/// Adds `field`'s tokens to `parts`.
pub(super) fn field_parts(
    krate: &TokenStream2,
    model: &syn::Path,
    field_enum: &syn::Ident,
    field: &FieldSpec,
    parts: &mut Parts,
) {
    let name = &field.ident;
    let ty = &field.ty;
    let variant = &field.variant;
    let name_str = name.to_string();
    let name_str = name_str.trim_start_matches("r#");
    let path = quote! { <#model>::fields().#name() };
    let binding = format_ident!("__read_{}", name);
    if let DefaultControl::Links(source, element) = &field.control {
        // The model's field is the `via` list of records, not this list of their keys: no
        // column is assigned, and the write links the records instead.
        let key = quote_spanned! {ty.span()=>
            #krate::__macro::links_key::<#model, #source, #element>(#path)
        };
        parts.claims.push(quote! {
            #krate::__macro::FormField {
                field: #field_enum::#variant,
                name: #name_str,
                required: ::std::vec::Vec::new(),
                keys: ::std::vec![#key],
            }
        });
        parts.hydrates.push(quote! {
            if let ::std::option::Option::Some(value) =
                #krate::__macro::write_links(&record.#name)
            {
                out.insert(#key, value);
            }
        });
        parts.reads.push(quote_spanned! {ty.span()=>
            let #binding: ::std::option::Option<#ty> = #krate::__macro::take_leaf(
                #krate::__macro::parse_list::<#element>(&#key, values),
                &mut errors,
            );
        });
        parts.links.push(quote! {
            (
                #field_enum::#variant,
                self.#name.iter().map(#krate::__macro::FormScalar::to_form).collect(),
            )
        });
        parts.link_variants.push(quote! { #field_enum::#variant });
        parts.includes.push(quote! {
            let includes = #krate::__macro::links_include::<#model, #source>(includes, #path);
        });
        return;
    }
    let key = quote_spanned! {ty.span()=>
        #krate::__macro::form_key::<#model, #ty>(#path)
    };
    let setter = format_ident!("set_{}", name_str);
    parts.asserts.push(quote! { let _: &#ty = &record.#name; });
    parts
        .creates
        .push(quote! { let create = create.#name(self.#name); });
    parts.updates.push(quote! {
        if named.contains(&#field_enum::#variant) {
            update.#setter(self.#name);
        }
    });
    if field.embed {
        parts.claims.push(quote! {
            #krate::__macro::embedded_field::<#model, #ty, _>(
                #path,
                #field_enum::#variant,
                #name_str,
            )
        });
        parts.hydrates.push(quote! {
            #krate::__macro::EmbeddedForm::write_form(&record.#name, #path, &mut out);
        });
        parts.reads.push(quote! {
            let #binding = #krate::__macro::take_value(
                <#ty as #krate::__macro::EmbeddedForm>::read_form(#path, values),
                &mut errors,
            );
        });
    } else if let DefaultControl::Files | DefaultControl::Choices(..) | DefaultControl::Tags(_) =
        &field.control
    {
        let element = match &field.control {
            DefaultControl::Choices(_, element) => quote! { #element },
            _ => quote! { ::std::string::String },
        };
        // Toasty names a list's path `List<T>`, not the field's `Vec<T>`.
        let key = quote_spanned! {ty.span()=>
            #krate::__macro::form_key::<#model, _>(#path)
        };
        parts.claims.push(quote! {
            #krate::__macro::FormField {
                field: #field_enum::#variant,
                name: #name_str,
                required: ::std::vec::Vec::new(),
                keys: ::std::vec![#key],
            }
        });
        parts.hydrates.push(quote_spanned! {ty.span()=>
            out.insert(#key, #krate::__macro::write_list(&record.#name));
        });
        parts.reads.push(quote_spanned! {ty.span()=>
            let #binding: ::std::option::Option<#ty> = #krate::__macro::take_leaf(
                #krate::__macro::parse_list::<#element>(&#key, values),
                &mut errors,
            );
        });
    } else if field.repeat {
        // Toasty names a list's path `List<T>`, not the field's `Vec<T>`.
        let key = quote_spanned! {ty.span()=>
            #krate::__macro::form_key::<#model, _>(#path)
        };
        parts.claims.push(quote! {
            #krate::__macro::FormField {
                field: #field_enum::#variant,
                name: #name_str,
                required: ::std::vec::Vec::new(),
                keys: ::std::vec![#key],
            }
        });
        parts.hydrates.push(quote_spanned! {ty.span()=>
            out.insert(#key, #krate::__macro::write_items(&record.#name));
        });
        parts.reads.push(quote_spanned! {ty.span()=>
            let #binding: ::std::option::Option<#ty> = #krate::__macro::take_value(
                #krate::__macro::parse_items(cx, &#key, values),
                &mut errors,
            );
        });
    } else {
        let blank = &field.blank;
        let required = field.required;
        let assert = assert_scalar(krate, ty);
        parts.claims.push(quote! {
            {
                #assert
                let key = #key;
                #krate::__macro::FormField {
                    field: #field_enum::#variant,
                    name: #name_str,
                    required: if #required {
                        ::std::vec![::std::clone::Clone::clone(&key)]
                    } else {
                        ::std::vec::Vec::new()
                    },
                    keys: ::std::vec![key],
                }
            }
        });
        parts.hydrates.push(quote_spanned! {ty.span()=>
            out.insert(#key, #krate::__macro::FormScalar::to_form(&record.#name));
        });
        parts.reads.push(quote_spanned! {ty.span()=>
            let #binding = #krate::__macro::take_leaf(
                #krate::__macro::parse_scalar::<#ty>(&#key, values, #blank),
                &mut errors,
            );
        });
    }
}
