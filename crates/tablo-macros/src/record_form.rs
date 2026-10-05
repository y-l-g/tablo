//! `#[derive(RecordForm)]` declares the typed value a resource's form writes.
//!
//! See `tablo-core`'s `form` module for the contract.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, Type, spanned::Spanned};

use crate::fields::{Derive, assert_scalar, form_attrs, last_segment};

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
    /// `#[form(blank = <expr>)]`, or `false` for a `bool` that declares none.
    blank: Option<syn::Expr>,
    /// The control the default schema renders for the field.
    control: DefaultControl,
}

/// The control a field gets in the derived schema.
enum DefaultControl {
    /// A text field: any scalar without a control key.
    Text,
    /// A checkbox: a `bool`.
    Toggle,
    /// A choice, over an `Options` type's list when one is named.
    Choice(Option<syn::Path>),
    /// A file field.
    File,
    /// An embedded value's own schema.
    Embed,
}

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

fn field_spec(field: &syn::Field) -> syn::Result<FieldSpec> {
    let ident = field.ident.clone().expect("named field");
    if last_segment(&field.ty).is_some_and(|name| name == "Deferred") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "a record form binds a relation's foreign key (`author_id`), not the relation: no \
             control posts a relation expression",
        ));
    }
    let attrs = form_attrs(field, Derive::Record)?;
    let variant = format_ident!("{}", pascal_case(&ident.to_string()));
    let is_bool =
        matches!(&field.ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident("bool"));
    let control = if attrs.embed {
        DefaultControl::Embed
    } else if let Some(options) = attrs.options {
        DefaultControl::Choice(Some(options))
    } else if attrs.choice {
        DefaultControl::Choice(None)
    } else if attrs.file {
        DefaultControl::File
    } else if is_bool {
        DefaultControl::Toggle
    } else {
        DefaultControl::Text
    };
    // An unchecked toggle posts `false`, so a `bool` reads an empty
    // submission as `false` unless it declares otherwise.
    let blank = attrs
        .blank
        .or_else(|| is_bool.then(|| syn::parse_quote!(false)));
    Ok(FieldSpec {
        ident,
        ty: field.ty.clone(),
        variant,
        embed: attrs.embed,
        blank,
        control,
    })
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
        let key = quote_spanned! {ty.span()=>
            #krate::__macro::form_key::<#model, #ty>(#path)
        };
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
                #krate::__macro::FormField {
                    field: #field_enum::#variant,
                    name: #name_str,
                    keys: #krate::__macro::embedded_keys::<#model, #ty>(#path),
                    answers_blank: <#ty as #krate::__macro::EmbeddedForm>::answers_blank(),
                }
            });
            hydrates.push(quote! {
                #krate::__macro::EmbeddedForm::write_form(&record.#name, cx, #path, &mut out);
            });
            reads.push(quote! {
                let #binding = #krate::__macro::take_value(
                    <#ty as #krate::__macro::EmbeddedForm>::read_form(cx, #path, values),
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
                quote_spanned! {ty.span()=>
                    <#ty as #krate::__macro::FormScalar>::blank().is_some()
                }
            };
            let assert = assert_scalar(krate, ty);
            claims.push(quote! {
                {
                    #assert
                    #krate::__macro::FormField {
                        field: #field_enum::#variant,
                        name: #name_str,
                        keys: ::std::vec![#key],
                        answers_blank: #answers_blank,
                    }
                }
            });
            hydrates.push(quote_spanned! {ty.span()=>
                out.insert(#key, #krate::__macro::FormScalar::to_form(&record.#name));
            });
            reads.push(quote_spanned! {ty.span()=>
                let #binding = #krate::__macro::take_leaf(
                    #krate::__macro::parse_scalar::<#ty>(&#key, values, #declared),
                    &mut errors,
                );
            });
        }
    }
    let controls_ident = format_ident!("{}Controls", ident);
    let controls_doc = format!(
        "One control per field of [`{ident}`], each chosen from the field: arrange them into a \
         layout in `Resource::form`, adjusting any with its builder's modifiers."
    );
    let mut control_fields = Vec::new();
    let mut control_inits = Vec::new();
    for field in fields {
        let name = &field.ident;
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
            DefaultControl::Choice(None) => (
                quote! { #krate::__macro::ChoiceField },
                quote! { #krate::__macro::Field::choice(#path) },
            ),
            DefaultControl::Choice(Some(options)) => (
                quote! { #krate::__macro::ChoiceField },
                quote! {
                    #krate::__macro::Field::choice(#path)
                        .options(<#options as #krate::__macro::Options>::options())
                },
            ),
            DefaultControl::File => (
                quote! { #krate::__macro::FileField },
                quote! { #krate::__macro::Field::file(#path) },
            ),
            DefaultControl::Embed => {
                let ty = &field.ty;
                (
                    quote! { #krate::__macro::Schema },
                    quote! {
                        <#ty as #krate::__macro::EmbeddedForm>::build_schema(
                            ::std::convert::Into::into(#path),
                        )
                    },
                )
            }
        };
        let doc = format!(
            "The `{}` control.",
            name.to_string().trim_start_matches("r#")
        );
        control_fields.push(quote! {
            #[doc = #doc]
            pub #name: #ty
        });
        control_inits.push(quote! { #name: #init });
    }
    let columns: Vec<TokenStream2> = fields
        .iter()
        .filter_map(|field| default_column(krate, model, field))
        .collect();
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
            /// `#[form(choice)]` a bare choice, `#[form(file)]` a file field,
            /// `#[form(embed)]` the embedded value's schema, and any other
            /// field a text field.
            #vis fn controls() -> #controls_ident {
                #controls_ident {
                    #(#control_inits,)*
                }
            }
        }

        impl #krate::__macro::RecordForm for #ident {
            type Model = #model;
            type Field = #field_enum;

            fn schema() -> #krate::__macro::Schema {
                let controls = Self::controls();
                #krate::__macro::Schema::empty()
                    #(.extend(#krate::__macro::IntoSchema::into_schema(controls.#names)))*
            }

            fn fields() -> ::std::vec::Vec<#krate::__macro::FormField<#field_enum>> {
                ::std::vec![#(#claims),*]
            }

            fn table() -> #krate::__macro::Table<#model> {
                #krate::__macro::Table::new(())#(.column(#columns))*
            }

            fn hydrate(
                cx: &#krate::__macro::Cx,
                record: &#model,
            ) -> ::std::collections::HashMap<::std::string::String, ::std::string::String> {
                #(#asserts)*
                let mut out = ::std::collections::HashMap::new();
                #(#hydrates)*
                out
            }

            fn parse(
                cx: &#krate::__macro::Cx,
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
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

/// The column the default table lists a field in: a sortable text column, searchable over a
/// `String`, a choice's option label, or a toggle's yes or no. A bare choice, which holds a key, a
/// file path and an embedded value get none.
fn default_column(
    krate: &TokenStream2,
    model: &syn::Path,
    field: &FieldSpec,
) -> Option<TokenStream2> {
    let name = &field.ident;
    let ty = &field.ty;
    let lens = quote! {
        #krate::__macro::Lens::<#model, #ty>::new(<#model>::fields().#name(), |record| &record.#name)
    };
    match &field.control {
        DefaultControl::Text => {
            let search = is_string(ty).then(|| quote! { .searchable() });
            Some(quote! { #krate::__macro::TextColumn::new(#lens).sortable()#search })
        }
        DefaultControl::Choice(Some(options)) => Some(quote! {
            #krate::__macro::TextColumn::new(#lens).sortable().format(|value| {
                <#options as #krate::__macro::Options>::label_of(
                    &#krate::__macro::FormScalar::to_form(value),
                )
            })
        }),
        DefaultControl::Toggle => {
            Some(quote! { #krate::__macro::BooleanColumn::new(#lens).sortable() })
        }
        DefaultControl::Choice(None) | DefaultControl::File | DefaultControl::Embed => None,
    }
}

/// Whether `ty` is spelled `String` or `Option<String>`.
fn is_string(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    match (last.ident.to_string().as_str(), &last.arguments) {
        ("String", syn::PathArguments::None) => true,
        ("Option", syn::PathArguments::AngleBracketed(args)) => matches!(
            args.args.first(),
            Some(syn::GenericArgument::Type(inner)) if is_string(inner)
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
