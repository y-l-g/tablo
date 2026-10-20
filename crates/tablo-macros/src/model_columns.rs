//! `#[derive(ModelColumns)]` gives a model its default table and detail page: a column per field
//! a person reads, picked by the field's type.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

use crate::{
    fields::{last_segment, named_fields, reject_generics},
    model_form::{foreign_keys, has, is_tenant, struct_keys},
};

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_checked(input: DeriveInput) -> syn::Result<TokenStream2> {
    let Fields {
        shown,
        tenant_columns,
    } = fields(&input)?;
    let krate = crate::tablo_core_path(&input.ident, "ModelColumns")?;
    let model = &input.ident;
    let fields: Vec<TokenStream2> = shown
        .iter()
        .map(|field| {
            let name = field.ident.as_ref().expect("named field");
            let ty = &field.ty;
            quote! {
                (&&&&&&&#krate::__macro::pick::ModelField(#krate::__macro::Lens::<
                    #model,
                    #ty,
                    #krate::__macro::pick::PathOf<#ty>,
                >::new(
                    <#model>::fields().#name(),
                    |record: &#model| &record.#name,
                )))
            }
        })
        .collect();
    let version = version_field(&input)?.map(|field| {
        let name = field.ident.as_ref().expect("named field");
        quote! {
            fn version(&self) -> ::core::option::Option<u64> {
                ::core::option::Option::Some(
                    #krate::__macro::Version::as_u64(self.#name),
                )
            }
        }
    });
    Ok(quote! {
        impl #krate::__macro::ModelColumns for #model {
            const TENANT_COLUMNS: &'static [&'static str] = &[#(#tenant_columns),*];

            fn table() -> #krate::__macro::Table<#model> {
                #[allow(unused_imports)]
                use #krate::__macro::pick::{
                    PickDate as _, PickEmbedded as _, PickNone as _, PickRepeater as _,
                    PickScalar as _, PickString as _, PickToggle as _,
                };
                // `__` names: a `let` pattern would match a unit struct the model's module names.
                let __table = #krate::__macro::Table::new(());
                #(let __table = #fields.table(__table);)*
                __table
            }

            fn detail() -> #krate::__macro::Detail<#model> {
                #[allow(unused_imports)]
                use #krate::__macro::pick::{
                    PickDate as _, PickEmbedded as _, PickNone as _, PickRepeater as _,
                    PickScalar as _, PickString as _, PickToggle as _,
                };
                let __detail = #krate::__macro::Detail::empty();
                #(let __detail = #fields.detail(__detail);)*
                __detail
            }

            #version
        }
    })
}

/// What the derive reads off a model's fields.
struct Fields<'a> {
    /// The fields a column shows, in declaration order.
    shown: Vec<&'a syn::Field>,
    /// The names of the fields typed `TenantId` or `Option<TenantId>`.
    tenant_columns: Vec<String>,
}

/// The fields of `input` a column shows, and its tenant columns.
fn fields(input: &DeriveInput) -> syn::Result<Fields<'_>> {
    reject_generics(
        input,
        "#[derive(ModelColumns)] does not support a generic model",
    )?;
    if let Some(attr) = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("columns"))
    {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[columns(..)]` goes on a field: `#[columns(skip)]`",
        ));
    }
    let named = named_fields(
        input,
        "#[derive(ModelColumns)] derives on a `toasty::Model` struct",
        None,
    )?;
    let foreign = foreign_keys(named.named.iter());
    let keys = struct_keys(input);
    let mut shown = Vec::new();
    let mut tenant_columns = Vec::new();
    for field in &named.named {
        let ident = field.ident.as_ref().expect("named field");
        if is_tenant(&field.ty) {
            tenant_columns.push(ident.to_string());
        }
        let skip = asks_skip(field)?;
        let key = has(field, "key") || keys.contains(ident);
        match (left_out(field, key, foreign.contains(ident)), skip) {
            (Some(reason), Some(attr)) => {
                return Err(syn::Error::new_spanned(
                    attr,
                    format!(
                        "#[derive(ModelColumns)] leaves this field out: {reason}; drop \
                         `#[columns(skip)]`"
                    ),
                ));
            }
            (None, None) => shown.push(field),
            _ => {}
        }
    }
    Ok(Fields {
        shown,
        tenant_columns,
    })
}

/// The model's `#[version]` field, which Toasty allows once per model.
fn version_field(input: &DeriveInput) -> syn::Result<Option<&syn::Field>> {
    let named = named_fields(
        input,
        "#[derive(ModelColumns)] derives on a `toasty::Model` struct",
        None,
    )?;
    Ok(named.named.iter().find(|field| has(field, "version")))
}

/// The field's `#[columns(skip)]`, if it has one.
fn asks_skip(field: &syn::Field) -> syn::Result<Option<&syn::Attribute>> {
    let mut skip = None;
    for attr in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("columns"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                skip = Some(attr);
                Ok(())
            } else {
                Err(meta.error("unknown `#[columns(..)]` key: expected `skip`"))
            }
        })?;
    }
    Ok(skip)
}

/// Why no default column shows `field`, when none does.
///
/// An `#[auto]` field off the key shows: Toasty writes an `#[auto] created_at` or `updated_at`
/// timestamp a person reads.
fn left_out(field: &syn::Field, key: bool, foreign_key: bool) -> Option<&'static str> {
    if key && has(field, "auto") {
        Some("Toasty generates this key")
    } else if has(field, "version") {
        Some("Toasty counts its versions")
    } else if has(field, "belongs_to")
        || has(field, "has_many")
        || has(field, "has_one")
        || last_segment(&field.ty).is_some_and(|name| name == "Deferred")
    {
        Some("a relation's records show through a `RelationColumn`")
    } else if foreign_key {
        Some("a foreign key's record shows through a `RelationColumn`")
    } else if is_tenant(&field.ty) {
        Some("a `TenantId` column belongs to the resource's `Tenancy`")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
