//! `#[derive(RecordForm)]` — the typed value a resource's form writes.
//!
//! The derive binds each field by its ident: `M::fields().<ident>()` names the
//! model field, so a field the model lacks is a rustc error at that ident, and
//! a type assertion against the model's field refuses a mismatched type. Keys
//! come from the framework at run time (`leaf_key`, `value_keys`), never from a
//! name this derive spells.
//!
//! See `tablo-core`'s `form` module for the contract.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Type};

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

/// One field as the derive reads it.
struct FieldSpec {
    ident: syn::Ident,
    ty: Type,
    variant: syn::Ident,
    /// `#[record_form(embed)]`: an `EmbeddedForm` value, bound whole.
    embed: bool,
    /// `#[record_form(blank = <expr>)]`.
    blank: Option<syn::Expr>,
}

/// What `#[record_form(..)]` says on the struct.
struct StructAttrs {
    model: syn::Path,
}

fn expand_checked(input: DeriveInput) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(RecordForm)] does not support generic or lifetime parameters: a record \
             form names one model",
        ));
    }
    let named = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) if !named.named.is_empty() => named,
            Fields::Named(_) => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "#[derive(RecordForm)] needs at least one field",
                ));
            }
            _ => {
                return Err(syn::Error::new_spanned(
                    &input.ident,
                    "#[derive(RecordForm)] supports a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[derive(RecordForm)] supports a struct with named fields",
            ));
        }
    };
    let attrs = struct_attrs(&input)?;
    let fields = named
        .named
        .iter()
        .map(field_spec)
        .collect::<syn::Result<Vec<_>>>()?;
    let krate = crate::tablo_core_path(&input.ident, "RecordForm")?;
    Ok(expand_struct(&krate, &input, &attrs, &fields))
}

fn struct_attrs(input: &DeriveInput) -> syn::Result<StructAttrs> {
    let mut model = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("record_form") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("model") {
                model = Some(meta.value()?.parse::<syn::Path>()?);
                Ok(())
            } else {
                Err(meta.error("unknown `#[record_form(..)]` key: expected `model = <Model>`"))
            }
        })?;
    }
    let model = model.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "#[derive(RecordForm)] needs `#[record_form(model = <Model>)]`",
        )
    })?;
    Ok(StructAttrs { model })
}

fn field_spec(field: &syn::Field) -> syn::Result<FieldSpec> {
    let ident = field.ident.clone().expect("named field");
    if last_segment(&field.ty).is_some_and(|name| name == "Deferred") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "a record form binds a relation's foreign key (`author_id`), not the relation: no \
             control posts a relation expression",
        ));
    }
    let mut embed = false;
    let mut blank = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("record_form") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("embed") {
                embed = true;
                Ok(())
            } else if meta.path.is_ident("blank") {
                blank = Some(meta.value()?.parse::<syn::Expr>()?);
                Ok(())
            } else {
                Err(meta.error(
                    "unknown `#[record_form(..)]` key: expected `embed` or `blank = <expr>`",
                ))
            }
        })?;
    }
    if let Some(expr) = &blank {
        if embed {
            return Err(syn::Error::new_spanned(
                expr,
                "`blank` does not apply to an embedded value: its leaves read an empty key as \
                 `Default`",
            ));
        }
        if last_segment(&field.ty).is_some_and(|name| name == "Option") {
            return Err(syn::Error::new_spanned(
                expr,
                "an `Option` field's blank answer is `None`",
            ));
        }
    }
    let variant = format_ident!("{}", pascal_case(&ident.to_string()));
    Ok(FieldSpec {
        ident,
        ty: field.ty.clone(),
        variant,
        embed,
        blank,
    })
}

/// The last path segment of `ty`, when it is a path.
fn last_segment(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// `author_id` → `AuthorId`; a raw identifier drops its `r#`.
fn pascal_case(name: &str) -> String {
    name.trim_start_matches("r#")
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect()
}

fn expand_struct(
    krate: &TokenStream2,
    input: &DeriveInput,
    attrs: &StructAttrs,
    fields: &[FieldSpec],
) -> TokenStream2 {
    let ident = &input.ident;
    let vis = &input.vis;
    let model = &attrs.model;
    let field_enum = format_ident!("{}Field", ident);
    let enum_doc = format!("One variant per field of [`{ident}`].");

    let variants: Vec<&syn::Ident> = fields.iter().map(|f| &f.variant).collect();
    let mut claims = Vec::new();
    let mut hydrates = Vec::new();
    let mut reads = Vec::new();
    let mut creates = Vec::new();
    let mut updates = Vec::new();
    let mut asserts = Vec::new();

    for field in fields {
        let name = &field.ident;
        let ty = &field.ty;
        let variant = &field.variant;
        let name_str = name.to_string();
        let name_str = name_str.trim_start_matches("r#");
        let path = quote! { <#model>::fields().#name() };
        let setter = format_ident!("set_{}", name_str);
        let binding = format_ident!("__read_{}", name);
        asserts.push(quote! { let _: &#ty = &record.#name; });
        creates.push(quote! { let create = create.#name(self.#name); });
        updates.push(quote! {
            if named.contains(&#field_enum::#variant) {
                update.#setter(self.#name);
            }
        });
        if field.embed {
            claims.push(quote! {
                #krate::form::FormField {
                    field: #field_enum::#variant,
                    name: #name_str,
                    keys: #krate::schema::value_keys::<#model, #ty>(cx, #path),
                    answers_blank: true,
                }
            });
            hydrates.push(quote! {
                #krate::schema::write_embedded::<#model, #ty>(cx, #path, &record.#name, &mut out);
            });
            reads.push(quote! {
                let #binding = #krate::schema::take_value(
                    #krate::schema::read_embedded::<#model, #ty>(cx, #path, values),
                    &mut errors,
                );
            });
        } else {
            let declared = match &field.blank {
                Some(expr) => quote! {
                    ::std::option::Option::Some(::std::convert::Into::<#ty>::into(#expr))
                },
                None => quote! { ::std::option::Option::None },
            };
            let answers_blank = if field.blank.is_some() {
                quote! { true }
            } else {
                quote! { <#ty as #krate::form::FormScalar>::blank().is_some() }
            };
            claims.push(quote! {
                #krate::form::FormField {
                    field: #field_enum::#variant,
                    name: #name_str,
                    keys: ::std::vec![#krate::schema::leaf_key::<#model, #ty>(cx, #path)],
                    answers_blank: #answers_blank,
                }
            });
            hydrates.push(quote! {
                out.insert(
                    #krate::schema::leaf_key::<#model, #ty>(cx, #path),
                    #krate::form::FormScalar::to_form(&record.#name),
                );
            });
            reads.push(quote! {
                let #binding = #krate::schema::take_leaf(
                    #krate::form::parse_scalar::<#ty>(
                        &#krate::schema::leaf_key::<#model, #ty>(cx, #path),
                        values,
                        #declared,
                    ),
                    &mut errors,
                );
            });
        }
    }
    let names: Vec<&syn::Ident> = fields.iter().map(|f| &f.ident).collect();
    let bindings: Vec<syn::Ident> = names
        .iter()
        .map(|name| format_ident!("__read_{}", name))
        .collect();

    quote! {
        #[doc = #enum_doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #vis enum #field_enum {
            #(#[allow(missing_docs)] #variants,)*
        }

        impl #krate::form::RecordForm for #ident {
            type Model = #model;
            type Field = #field_enum;

            fn fields(
                cx: &#krate::__macro::Cx,
            ) -> ::std::vec::Vec<#krate::form::FormField<#field_enum>> {
                ::std::vec![#(#claims),*]
            }

            fn hydrate(
                cx: &#krate::__macro::Cx,
                record: &#model,
            ) -> ::std::collections::HashMap<::std::string::String, ::std::string::String> {
                // Each form field has the model field's type.
                #(#asserts)*
                let mut out = ::std::collections::HashMap::new();
                #(#hydrates)*
                out
            }

            fn parse(
                cx: &#krate::__macro::Cx,
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) -> ::std::result::Result<Self, ::std::vec::Vec<#krate::form::FieldError>> {
                let mut errors: ::std::vec::Vec<#krate::form::FieldError> = ::std::vec::Vec::new();
                #(#reads)*
                match (#(#bindings,)*) {
                    (#(::std::option::Option::Some(#bindings),)*) if errors.is_empty() => {
                        ::std::result::Result::Ok(Self { #(#names: #bindings),* })
                    }
                    _ => ::std::result::Result::Err(errors),
                }
            }

            fn into_create(self) -> <#model as #krate::__macro::Model>::Create {
                let create = <<#model as #krate::__macro::Model>::Create as ::std::default::Default>::default();
                #(#creates)*
                create
            }

            fn into_update<'a>(
                self,
                record: &'a mut #model,
                named: &::std::collections::HashSet<#field_enum>,
            ) -> ::std::option::Option<<#model as #krate::__macro::Model>::Update<'a>> {
                if named.is_empty() {
                    return ::std::option::Option::None;
                }
                let mut update = record.update();
                #(#updates)*
                ::std::option::Option::Some(update)
            }

            fn exec_update<'a>(
                update: <#model as #krate::__macro::Model>::Update<'a>,
                ex: &'a mut dyn #krate::__macro::Executor,
            ) -> impl ::std::future::Future<Output = #krate::__macro::DbResult<()>>
                   + ::std::marker::Send
                   + 'a {
                update.exec(ex)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal `source` produces, or the expansion when none fires.
    fn refusal(source: &str) -> String {
        let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
        match expand_checked(input) {
            Ok(_) => String::from("<expanded>"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_generic_form_is_refused() {
        let message = refusal("#[record_form(model = M)] struct F<T> { a: T }");
        assert!(message.contains("generic"), "{message}");
    }

    #[test]
    fn a_tuple_struct_or_an_empty_struct_is_refused() {
        let message = refusal("#[record_form(model = M)] struct F(String);");
        assert!(message.contains("named fields"), "{message}");
        let message = refusal("#[record_form(model = M)] struct F {}");
        assert!(message.contains("at least one field"), "{message}");
    }

    #[test]
    fn a_missing_model_is_refused() {
        let message = refusal("struct F { a: String }");
        assert!(message.contains("model = <Model>"), "{message}");
    }

    #[test]
    fn a_relation_field_is_refused() {
        let message = refusal("#[record_form(model = M)] struct F { author: Deferred<Author> }");
        assert!(message.contains("foreign key"), "{message}");
    }

    #[test]
    fn blank_on_an_option_or_an_embed_is_refused() {
        let message = refusal(
            "#[record_form(model = M)] struct F { #[record_form(blank = None)] a: Option<i64> }",
        );
        assert!(message.contains("`None`"), "{message}");
        let message = refusal(
            "#[record_form(model = M)] struct F { #[record_form(embed, blank = 1)] a: Seo }",
        );
        assert!(message.contains("embedded value"), "{message}");
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let message =
            refusal("#[record_form(model = M)] struct F { #[record_form(blnk = 1)] a: i64 }");
        assert!(message.contains("unknown"), "{message}");
        let message = refusal("#[record_form(model = M, tenant)] struct F { a: i64 }");
        assert!(message.contains("unknown"), "{message}");
    }

    #[test]
    fn field_variants_are_pascal_case() {
        assert_eq!(pascal_case("author_id"), "AuthorId");
        assert_eq!(pascal_case("r#type"), "Type");
        assert_eq!(pascal_case("seo"), "Seo");
    }
}
