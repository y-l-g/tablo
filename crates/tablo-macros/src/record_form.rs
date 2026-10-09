//! `#[derive(RecordForm)]` declares the typed value a resource's form writes.
//!
//! See `tablo-core`'s `form` module for the contract.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, Type, spanned::Spanned};

use crate::fields::{Derive, assert_scalar, blank_answer, blank_option, form_attrs, last_segment};

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
    /// `#[form(repeat)]`: a list of `RepeaterItem` values, bound whole.
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
    /// An embedded value's own schema.
    Embed,
    /// A repeater over the list's items.
    Repeat,
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
    } else if attrs.file {
        DefaultControl::File
    } else if is_bool {
        DefaultControl::Toggle
    } else {
        DefaultControl::Text
    };
    let links = matches!(control, DefaultControl::Links(..));
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
            DefaultControl::Repeat => (
                quote! { #krate::__macro::RepeaterField },
                quote! { #krate::__macro::Field::repeater(#path) },
            ),
            DefaultControl::Embed => {
                let ty = &field.ty;
                (
                    quote! { #krate::__macro::Schema },
                    quote! {
                        #krate::__macro::embedded_form::<#model, #ty>(
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
    let columns: Vec<TokenStream2> = fields
        .iter()
        .filter_map(|field| default_column(krate, model, field))
        .collect();
    let entries: Vec<TokenStream2> = fields
        .iter()
        .map(|field| default_entry(krate, model, field))
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
            /// `#[form(options)]` a choice over the field type's options,
            /// `#[form(relationship = R)]` a choice over `R`'s records (a multiple choice over a
            /// `Vec` of their keys), `#[form(file)]` a file field,
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

            fn fields(
                resolver: &#krate::__macro::FieldResolver,
            ) -> ::std::vec::Vec<#krate::__macro::FormField<#field_enum>> {
                // Only an embedded field resolves its keys through the app schema.
                let _ = resolver;
                ::std::vec![#(#claims),*]
            }

            fn table() -> #krate::__macro::Table<#model> {
                #krate::__macro::Table::new(())#(.column(#columns))*
            }

            fn detail() -> #krate::__macro::Detail<#model> {
                #krate::__macro::Detail::empty()#(.column(#entries))*
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

/// The tokens each generated method holds for the fields, in declaration order.
#[derive(Default)]
struct Parts {
    claims: Vec<TokenStream2>,
    hydrates: Vec<TokenStream2>,
    reads: Vec<TokenStream2>,
    creates: Vec<TokenStream2>,
    updates: Vec<TokenStream2>,
    asserts: Vec<TokenStream2>,
    /// A many-to-many field's `links` entry, variant and include.
    links: Vec<TokenStream2>,
    link_variants: Vec<TokenStream2>,
    includes: Vec<TokenStream2>,
}

/// Adds `field`'s tokens to `parts`.
fn field_parts(
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
                resolver,
                #path,
                #field_enum::#variant,
                #name_str,
            )
        });
        parts.hydrates.push(quote! {
            #krate::__macro::EmbeddedForm::write_form(&record.#name, cx, #path, &mut out);
        });
        parts.reads.push(quote! {
            let #binding = #krate::__macro::take_value(
                <#ty as #krate::__macro::EmbeddedForm>::read_form(cx, #path, values),
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

/// The column the default table lists a field in: a sortable text column, searchable over a
/// `String` or `Option<String>`, a choice's option label, or a toggle's yes or no. A relationship,
/// which holds a key, a file path, an embedded value and a repeater's list get none.
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
        DefaultControl::OwnOptions => {
            Some(quote! { #krate::__macro::TextColumn::new(#lens).sortable() })
        }
        DefaultControl::Text => {
            let search = is_string(ty).then(|| quote! { .searchable() });
            Some(quote! { #krate::__macro::TextColumn::new(#lens).sortable()#search })
        }
        DefaultControl::Choice(options) => Some(quote! {
            #krate::__macro::TextColumn::new(#lens).sortable().format(|value| {
                <#options as #krate::__macro::Options>::label_of(
                    &#krate::__macro::FormScalar::to_form(value),
                )
            })
        }),
        DefaultControl::Toggle => {
            Some(quote! { #krate::__macro::BooleanColumn::new(#lens).sortable() })
        }
        DefaultControl::Relationship(_)
        | DefaultControl::Links(..)
        | DefaultControl::File
        | DefaultControl::Embed
        | DefaultControl::Repeat => None,
    }
}

/// The column the default detail page shows a field in: a text column, a choice's option label, a
/// toggle's yes or no, a file path's link, an embedded value's leaves, or a repeater's items. A
/// relationship shows the key it holds, so a form of keys alone still has a detail page.
fn default_entry(krate: &TokenStream2, model: &syn::Path, field: &FieldSpec) -> TokenStream2 {
    let name = &field.ident;
    let ty = &field.ty;
    let lens = quote! {
        #krate::__macro::Lens::<#model, #ty>::new(<#model>::fields().#name(), |record| &record.#name)
    };
    match &field.control {
        DefaultControl::Text | DefaultControl::Relationship(_) | DefaultControl::OwnOptions => {
            quote! { #krate::__macro::TextColumn::new(#lens) }
        }
        DefaultControl::Choice(options) => quote! {
            #krate::__macro::TextColumn::new(#lens).format(|value| {
                <#options as #krate::__macro::Options>::label_of(
                    &#krate::__macro::FormScalar::to_form(value),
                )
            })
        },
        DefaultControl::Toggle => quote! { #krate::__macro::BooleanColumn::new(#lens) },
        DefaultControl::Links(source, _) => quote! {
            #krate::__macro::RelationColumn::list::<#source>(#krate::__macro::RelationLens::new(
                <#model>::fields().#name(),
                |record: &#model| &record.#name,
            ))
        },
        DefaultControl::File => quote! { #krate::__macro::FileColumn::new(#lens) },
        DefaultControl::Embed => quote! { #krate::__macro::EmbeddedColumn::new(#lens) },
        DefaultControl::Repeat => quote! {
            #krate::__macro::RepeaterColumn::new(#krate::__macro::Lens::new(
                <#model>::fields().#name(),
                |record: &#model| &record.#name,
            ))
        },
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
