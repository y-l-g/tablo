//! `#[derive(ModelForm)]` writes a model's record form for it: the `RecordForm` struct of the
//! fields a person fills, expanded as that derive expands it.

use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Type, spanned::Spanned};

use crate::{fields::last_segment, record_form};

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_checked(input: DeriveInput) -> syn::Result<TokenStream2> {
    let form = form_of(&input)?;
    let expansion = record_form::expand_as(form.clone(), "ModelForm")?;
    let model = &input.ident;
    let doc = format!(
        "The record form of [`{model}`], one field per column a person fills, from \
         `#[derive(ModelForm)]`."
    );
    let vis = &form.vis;
    let name = &form.ident;
    let Data::Struct(syn::DataStruct {
        fields: Fields::Named(named),
        ..
    }) = form.data
    else {
        unreachable!("`form_of` builds a struct with named fields");
    };
    // No derive on the emitted struct owns `#[form(..)]`: the expansion has read its keys.
    let fields = named.named.into_iter().map(|mut field| {
        field.attrs.retain(|attr| !attr.path().is_ident("form"));
        field
    });
    Ok(quote! {
        #[doc = #doc]
        #vis struct #name {
            #(#fields,)*
        }

        #expansion
    })
}

/// The `#[derive(RecordForm)]` input of `input`'s form: its fields a person fills.
fn form_of(input: &DeriveInput) -> syn::Result<DeriveInput> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(ModelForm)] does not support a generic model",
        ));
    }
    let Data::Struct(syn::DataStruct {
        fields: Fields::Named(named),
        ..
    }) = &input.data
    else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[derive(ModelForm)] derives on a `toasty::Model` struct",
        ));
    };
    let model = &input.ident;
    let vis = &input.vis;
    let name = form_name(input)?;
    let keys: Vec<syn::Ident> = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("key"))
        .flat_map(|attr| idents(inner(attr)))
        .collect();
    let foreign = foreign_keys(named.named.iter());

    let mut kept = syn::punctuated::Punctuated::<syn::Field, syn::Token![,]>::new();
    for field in &named.named {
        let ident = field.ident.as_ref().expect("named field");
        let form: Vec<&syn::Attribute> = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("form"))
            .collect();
        let skip = form.iter().any(|attr| asks_skip(attr));
        if skip && form.iter().any(|attr| !is_bare_skip(attr)) {
            return Err(syn::Error::new_spanned(
                field,
                "`skip` leaves the field out of the form: it takes no other `#[form(..)]` key",
            ));
        }
        if let Some(reason) = never_written(field, keys.contains(ident)) {
            if let Some(attr) = form.first() {
                let fix = if skip {
                    "Drop `skip`"
                } else {
                    "Declare a `#[derive(RecordForm)]` struct to write it"
                };
                return Err(syn::Error::new_spanned(
                    attr,
                    format!("#[derive(ModelForm)] leaves this field out: {reason}. {fix}"),
                ));
            }
            continue;
        }
        // Toasty fills a defaulted field and a foreign key needs a source: a form key opts in.
        let opt_in = has(field, "default") || foreign.contains(ident);
        if skip || (opt_in && form.is_empty()) {
            continue;
        }
        kept.push(form_field(field));
    }
    if kept.is_empty() {
        return Err(syn::Error::new_spanned(
            model,
            format!("#[derive(ModelForm)] leaves out every field of `{model}`: no form to write"),
        ));
    }
    Ok(syn::parse_quote! {
        #[form(model = #model)]
        #vis struct #name { #kept }
    })
}

/// `#[form(name = PostEditor)]`, else `{Model}Form`.
fn form_name(input: &DeriveInput) -> syn::Result<syn::Ident> {
    let mut name = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("form"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                name = Some(meta.value()?.parse::<syn::Ident>()?);
                Ok(())
            } else if meta.path.is_ident("model") {
                Err(meta.error(
                    "#[derive(ModelForm)] writes the form of the model it derives on: drop \
                     `model = ..`",
                ))
            } else {
                Err(meta.error("unknown `#[form(..)]` key: expected `name = <Ident>`"))
            }
        })?;
    }
    Ok(name.unwrap_or_else(|| format_ident!("{}Form", input.ident)))
}

/// Why no form writes `field`, when none does: Toasty, the panel or a relation writes it.
fn never_written(field: &syn::Field, struct_key: bool) -> Option<&'static str> {
    if has(field, "key") || struct_key {
        Some("a key is not edited")
    } else if has(field, "auto") || has(field, "update") || has(field, "version") {
        Some("Toasty writes it")
    } else if has(field, "belongs_to")
        || has(field, "has_many")
        || has(field, "has_one")
        || last_segment(&field.ty).is_some_and(|name| name == "Deferred")
    {
        Some("a relation is written through its foreign key")
    } else if is_tenant(&field.ty) {
        Some("the panel stamps the tenant column")
    } else {
        None
    }
}

fn has(field: &syn::Field, name: &str) -> bool {
    field.attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// Whether `ty` is spelled `TenantId` or `Option<TenantId>`.
fn is_tenant(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    match (last.ident.to_string().as_str(), &last.arguments) {
        ("TenantId", syn::PathArguments::None) => true,
        ("Option", syn::PathArguments::AngleBracketed(args)) => matches!(
            args.args.first(),
            Some(syn::GenericArgument::Type(inner)) if is_tenant(inner)
        ),
        _ => false,
    }
}

/// The fields every `#[belongs_to]` names as its key: `key = a`, `key = [a, b]`, else
/// `<field>_id`.
fn foreign_keys<'a>(fields: impl Iterator<Item = &'a syn::Field>) -> Vec<syn::Ident> {
    let mut out = Vec::new();
    for field in fields {
        for attr in field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("belongs_to"))
        {
            let tokens: Vec<TokenTree> = inner(attr).into_iter().collect();
            let named = tokens.windows(3).find_map(|window| match window {
                [TokenTree::Ident(key), TokenTree::Punct(eq), value]
                    if key == "key" && eq.as_char() == '=' =>
                {
                    Some(idents(value.clone().into()))
                }
                _ => None,
            });
            match named {
                Some(keys) => out.extend(keys),
                None => {
                    let ident = field.ident.as_ref().expect("named field");
                    out.push(format_ident!("{}_id", ident));
                }
            }
        }
    }
    out
}

/// Every identifier in `tokens`, at any depth.
fn idents(tokens: TokenStream2) -> Vec<syn::Ident> {
    let mut out = Vec::new();
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => out.push(ident),
            TokenTree::Group(group) => out.extend(idents(group.stream())),
            _ => {}
        }
    }
    out
}

/// Whether `attr` holds the `skip` key.
fn asks_skip(attr: &syn::Attribute) -> bool {
    inner(attr)
        .into_iter()
        .any(|tree| matches!(&tree, TokenTree::Ident(ident) if ident == "skip"))
}

/// Whether `attr` is `#[form(skip)]`.
fn is_bare_skip(attr: &syn::Attribute) -> bool {
    let tokens: Vec<TokenTree> = inner(attr).into_iter().collect();
    matches!(tokens.as_slice(), [TokenTree::Ident(ident)] if ident == "skip")
}

/// `field` as the form declares it: public, with its docs and `#[form(..)]` keys.
fn form_field(field: &syn::Field) -> syn::Field {
    let mut field = field.clone();
    field
        .attrs
        .retain(|attr| attr.path().is_ident("doc") || attr.path().is_ident("form"));
    field.vis = syn::parse_quote_spanned! {field.span()=> pub };
    field
}

/// The tokens inside `attr`'s delimiters, empty for a bare path.
fn inner(attr: &syn::Attribute) -> TokenStream2 {
    match &attr.meta {
        syn::Meta::List(list) => list.tokens.clone(),
        syn::Meta::Path(_) | syn::Meta::NameValue(_) => TokenStream2::new(),
    }
}

#[cfg(test)]
mod tests;
