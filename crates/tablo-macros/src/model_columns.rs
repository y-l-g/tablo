//! `#[derive(ModelColumns)]` gives a model its default table and detail page: a column per field
//! a person reads, picked by the field's type and shaped by its `#[columns(..)]` keys.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::DeriveInput;

use self::keys::ColumnKeys;
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
    let pick = quote!(#krate::__macro::pick);
    let mut table = TokenStream2::new();
    let mut detail = TokenStream2::new();
    for (field, keys) in &shown {
        let name = field.ident.as_ref().expect("named field");
        let ty = &field.ty;
        let model_field = quote! {
            let __field = #pick::ModelField(#krate::__macro::Lens::<
                #model,
                #ty,
                #pick::PathOf<#ty>,
            >::new(
                <#model>::fields().#name(),
                |record: &#model| &record.#name,
            ));
        };
        let table_calls = keys.column_calls(&krate, true);
        let filter = keys.filter(&krate);
        table.extend(quote! {
            #model_field
            let __column = (&&&&&&&__field).table_column();
            #table_calls
            let __table = #pick::Place::place_in_table(__column, __table);
            #filter
        });
        let detail_calls = keys.column_calls(&krate, false);
        detail.extend(quote! {
            #model_field
            let __column = (&&&&&&&__field).detail_column();
            #detail_calls
            let __detail = #pick::Place::place_in_detail(__column, __detail);
        });
    }
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
    let picks = quote! {
        #[allow(unused_imports)]
        use #pick::{
            PickDate as _, PickDateFilter as _, PickEmbedded as _, PickNoFilter as _,
            PickNone as _, PickRepeater as _, PickScalar as _, PickSelectFilter as _,
            PickString as _, PickTernaryFilter as _, PickToggle as _,
        };
    };
    Ok(quote! {
        impl #krate::__macro::ModelColumns for #model {
            const TENANT_COLUMNS: &'static [&'static str] = &[#(#tenant_columns),*];

            fn table() -> #krate::__macro::Table<#model> {
                #picks
                // `__` names: a `let` pattern would match a unit struct the model's module names.
                let __table = #krate::__macro::Table::new(());
                #table
                __table
            }

            fn detail() -> #krate::__macro::Detail<#model> {
                #picks
                let __detail = #krate::__macro::Detail::empty();
                #detail
                __detail
            }

            #version
        }
    })
}

/// What the derive reads off a model's fields.
struct Fields<'a> {
    /// The fields a column shows, in declaration order, with their `#[columns(..)]` keys.
    shown: Vec<(&'a syn::Field, ColumnKeys)>,
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
        let column_keys = ColumnKeys::parse(field)?;
        let key = has(field, "key") || keys.contains(ident);
        match left_out(field, key, foreign.contains(ident)) {
            Some(reason) => {
                if let Some(span) = column_keys.skip.or_else(|| column_keys.presentation()) {
                    return Err(syn::Error::new(
                        span,
                        format!(
                            "#[derive(ModelColumns)] leaves this field out: {reason}; drop its \
                             `#[columns(..)]` keys"
                        ),
                    ));
                }
            }
            None if column_keys.skip.is_none() => shown.push((field, column_keys)),
            None => {}
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

mod keys;

#[cfg(test)]
mod tests;
