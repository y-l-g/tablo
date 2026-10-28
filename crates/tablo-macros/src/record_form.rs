//! `#[derive(RecordForm)]` declares the typed value a resource's form writes.
//!
//! See `tablo-core`'s `form` module for the contract.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{DeriveInput, Type};

use crate::fields::{
    Derive, blank_answer, blank_option, form_attrs, last_segment, named_fields, reject_generics,
};

mod parts;
mod variant;

use parts::{Parts, field_parts};
pub(crate) use variant::field_variants;

pub fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

struct FieldSpec {
    ident: syn::Ident,
    ty: Type,
    variant: syn::Ident,
    /// `#[form(embed)]`: an `EmbeddedForm` value, bound whole.
    embed: bool,
    /// `#[form(repeat)]`: a list of `FormInput` values, bound whole.
    repeat: bool,
    /// The parse's blank answer, `Option::None` for a required field.
    blank: TokenStream2,
    /// Whether the field has no blank answer, so its control renders required.
    required: bool,
    /// The control the default schema renders for the field.
    control: DefaultControl,
}

/// The control `controls()` hands over for a field.
enum DefaultControl {
    /// A text field: any scalar without a control key.
    Text,
    /// A checkbox: a `bool`.
    Toggle,
    /// A choice over an `Options` type's list.
    Choice(Box<Type>),
    /// A choice over an `OptionSource`'s rows.
    Relationship(Box<Type>),
    /// A multiple choice over an `OptionSource`'s rows: a many-to-many field, a `Vec` of their
    /// keys, of the element type.
    Links(Box<Type>, Box<Type>),
    /// A choice over the field type's own options, which its column reads as labels.
    OwnOptions,
    /// A file field.
    File,
    /// A file list: `#[form(file)]` on a `Vec<String>`.
    Files,
    /// An embedded value's own schema.
    Embed,
    /// A repeater over the list's items.
    Repeat,
}

struct StructAttrs {
    model: syn::Path,
}

fn expand_checked(input: DeriveInput) -> syn::Result<TokenStream2> {
    expand_as(input, "RecordForm")
}

/// The expansion of `input`, naming `derive` when no `tablo` dependency is found.
pub(crate) fn expand_as(input: DeriveInput, derive: &str) -> syn::Result<TokenStream2> {
    reject_generics(
        &input,
        "#[derive(RecordForm)] does not support generic or lifetime parameters: a record \
         form names one model",
    )?;
    let named = named_fields(
        &input,
        "#[derive(RecordForm)] supports a struct with named fields",
        Some("#[derive(RecordForm)] needs at least one field"),
    )?;
    let attrs = struct_attrs(&input)?;
    let variants = field_variants(named.named.iter())?;
    let fields = named
        .named
        .iter()
        .zip(variants)
        .map(|(field, variant)| field_spec(field, variant))
        .collect::<syn::Result<Vec<_>>>()?;
    let krate = crate::tablo_core_path(&input.ident, derive)?;
    Ok(expand_struct(&krate, &input, &attrs, &fields))
}

fn struct_attrs(input: &DeriveInput) -> syn::Result<StructAttrs> {
    let mut model = None;
    for attr in &input.attrs {
        if !attr.path().is_ident("form") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("model") {
                model = Some(meta.value()?.parse::<syn::Path>()?);
                Ok(())
            } else {
                Err(meta.error("unknown `#[form(..)]` key: expected `model = <Model>`"))
            }
        })?;
    }
    let model = model.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "#[derive(RecordForm)] needs `#[form(model = <Model>)]`",
        )
    })?;
    Ok(StructAttrs { model })
}

fn field_spec(field: &syn::Field, variant: syn::Ident) -> syn::Result<FieldSpec> {
    let ident = field.ident.clone().expect("named field");
    if last_segment(&field.ty).is_some_and(|name| name == "Deferred") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "a record form binds a relation's foreign key (`author_id`), not the relation: no \
             control posts a relation expression",
        ));
    }
    let attrs = form_attrs(field, Derive::Record)?;
    let is_bool =
        matches!(&field.ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("bool"));
    let control = if attrs.embed {
        DefaultControl::Embed
    } else if attrs.repeat {
        DefaultControl::Repeat
    } else if let Some(options) = attrs.options.clone() {
        match options {
            Some(named) => DefaultControl::Choice(Box::new(named)),
            None => DefaultControl::OwnOptions,
        }
    } else if let Some(source) = attrs.relationship.clone() {
        match list_element(&field.ty) {
            Some(element) => {
                if attrs.blank.is_some() || attrs.optional {
                    return Err(syn::Error::new_spanned(
                        field,
                        "a many-to-many field links no record when none is chosen: it takes no \
                         `blank` or `optional`",
                    ));
                }
                DefaultControl::Links(Box::new(source), Box::new(element.clone()))
            }
            None => DefaultControl::Relationship(Box::new(source)),
        }
    } else if attrs.file && list_element(&field.ty).is_some() {
        if attrs.blank.is_some() || attrs.optional {
            return Err(syn::Error::new_spanned(
                field,
                "a file list stores no file when none is chosen: it takes no `blank` or \
                 `optional`",
            ));
        }
        DefaultControl::Files
    } else if attrs.file {
        DefaultControl::File
    } else if is_bool {
        DefaultControl::Toggle
    } else {
        DefaultControl::Text
    };
    let links = matches!(control, DefaultControl::Links(..) | DefaultControl::Files);
    let required =
        !attrs.embed && !attrs.repeat && !links && blank_answer(&field.ty, &attrs).is_none();
    Ok(FieldSpec {
        ident,
        ty: field.ty.clone(),
        variant,
        embed: attrs.embed,
        repeat: attrs.repeat,
        blank: blank_option(&field.ty, &attrs),
        required,
        control,
    })
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
    let mut parts = Parts::default();
    for field in fields {
        field_parts(krate, model, &field_enum, field, &mut parts);
    }
    let Parts {
        claims,
        hydrates,
        reads,
        creates,
        updates,
        asserts,
        links,
        link_variants,
        includes,
    } = parts;
    let controls_ident = format_ident!("{}Controls", ident);
    let controls_doc = format!(
        "One control per field of [`{ident}`], each chosen from the field: arrange them into a \
         layout in `ResourceDef::form`, adjusting any with its builder's modifiers. A control \
         left out follows the layout."
    );
    let mut control_fields = Vec::new();
    let mut control_inits = Vec::new();
    let mut control_arms = Vec::new();
    for field in fields {
        let name = &field.ident;
        let variant = &field.variant;
        let path = quote! { <#model>::fields().#name() };
        let (ty, init) = match &field.control {
            DefaultControl::Text => (
                quote! { #krate::__macro::TextField },
                quote! { #krate::__macro::Field::text(#path) },
            ),
            DefaultControl::Toggle => (
                quote! { #krate::__macro::CustomField },
                quote! { #krate::__macro::Field::toggle(#path) },
            ),
            DefaultControl::Relationship(source) => (
                quote! { #krate::__macro::ChoiceField },
                quote! { #krate::__macro::Field::choice(#path).relationship::<#source>() },
            ),
            DefaultControl::Links(source, _) => (
                quote! { #krate::__macro::ChoiceField },
                quote! {
                    #krate::__macro::Field::choice::<
                        #model,
                        #krate::__macro::List<<#source as #krate::__macro::OptionSource>::Model>,
                    >(#path)
                    .relationship::<#source>()
                    .multiple()
                },
            ),
            DefaultControl::Choice(options) => (
                quote! { #krate::__macro::ChoiceField },
                quote! {
                    #krate::__macro::Field::choice(#path)
                        .options(<#options as #krate::__macro::Options>::options())
                },
            ),
            DefaultControl::OwnOptions => {
                let ty = &field.ty;
                (
                    quote! { #krate::__macro::ChoiceField },
                    quote! {
                        #krate::__macro::Field::choice(#path)
                            .options(<#ty as #krate::__macro::Options>::options())
                    },
                )
            }
            DefaultControl::File => (
                quote! { #krate::__macro::FileField },
                quote! { #krate::__macro::Field::file(#path) },
            ),
            DefaultControl::Files => (
                quote! { #krate::__macro::FileField },
                quote! { #krate::__macro::Field::files(#path) },
            ),
            DefaultControl::Repeat => (
                quote! { #krate::__macro::RepeaterField },
                quote! { #krate::__macro::Field::repeater(#path) },
            ),
            DefaultControl::Embed => {
                let ty = &field.ty;
                (
                    quote! { #krate::__macro::Schema },
                    quote! {
                        <#ty as #krate::__macro::EmbeddedForm>::build_schema::<#model>(
                            ::std::convert::Into::into(#path),
                        )
                    },
                )
            }
        };
        // Only the derive hands a control to its form: no other `Schema<#ident>` places one.
        let init = quote! { #krate::__macro::Retype::retype::<#ident>(#init) };
        let doc = format!(
            "The `{}` control.",
            name.to_string().trim_start_matches("r#")
        );
        control_fields.push(quote! {
            #[doc = #doc]
            pub #name: #ty<#ident>
        });
        control_arms.push(quote! {
            #field_enum::#variant => #krate::__macro::IntoSchema::into_schema(#init)
        });
        control_inits.push(quote! { #name: #init });
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

        #[doc = #controls_doc]
        #vis struct #controls_ident {
            #(#control_fields,)*
        }

        impl #ident {
            /// Builds every field's control from the field: a `bool` is a
            /// toggle, `#[form(options = T)]` a choice over `T`'s options,
            /// `#[form(options)]` a choice over the field type's options,
            /// `#[form(relationship = R)]` a choice over `R`'s records (a multiple choice over a
            /// `Vec` of their keys), `#[form(file)]` a file field (a file list over a
            /// `Vec<String>`),
            /// `#[form(embed)]` the embedded value's schema, `#[form(repeat)]` a repeater
            /// over the list's items, and any other field a text field.
            #vis fn controls() -> #controls_ident {
                #controls_ident {
                    #(#control_inits,)*
                }
            }
        }

        impl #krate::__macro::RecordForm for #ident {
            type Model = #model;
            type Field = #field_enum;

            fn control(field: #field_enum) -> #krate::__macro::Schema<Self> {
                match field {
                    #(#control_arms,)*
                }
            }

            fn fields() -> ::std::vec::Vec<#krate::__macro::FormField<#field_enum>> {
                ::std::vec![#(#claims),*]
            }

            fn includes() -> #krate::__macro::Includes<#model> {
                let includes = #krate::__macro::Includes::new();
                #(#includes)*
                includes
            }

            fn links(
                &self,
            ) -> ::std::vec::Vec<(#field_enum, ::std::vec::Vec<::std::string::String>)> {
                ::std::vec![#(#links),*]
            }

            fn hydrate(
                cx: &#krate::__macro::Cx,
                record: &#model,
            ) -> #krate::__macro::FormValues {
                #(#asserts)*
                let mut out = #krate::__macro::FormValues::new();
                #(#hydrates)*
                out
            }

            fn parse(
                cx: &#krate::__macro::Cx,
                values: &#krate::__macro::FormValues,
            ) -> ::std::result::Result<Self, ::std::vec::Vec<#krate::__macro::FieldError>> {
                let mut errors: ::std::vec::Vec<#krate::__macro::FieldError> = ::std::vec::Vec::new();
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
                // A many-to-many field assigns no column: the write links its records.
                let links: &[#field_enum] = &[#(#link_variants),*];
                if named.iter().all(|field| links.contains(field)) {
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

/// The element type of a `Vec<T>`, or `None` for any other type.
fn list_element(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    match (last.ident.to_string().as_str(), &last.arguments) {
        ("Vec", syn::PathArguments::AngleBracketed(args)) => match args.args.first() {
            Some(syn::GenericArgument::Type(element)) if args.args.len() == 1 => Some(element),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests;
