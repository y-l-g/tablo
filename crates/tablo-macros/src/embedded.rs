//! `#[derive(EmbeddedForm)]` — the typed half of an embedded value.
//!
//! The derive knows the Rust shape (which fields exist, their types, which
//! variants there are); the framework knows the storage (which column each leaf
//! occupies, what the discriminant column is called). The derive builds the
//! value's schema node through the framework's builder, one resolved field per
//! leaf, and reads and writes through that node's keys, so it never spells a
//! flattened name itself.
//!
//! See `tablo-core`'s `schema::embedded` module for the contract.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote, quote_spanned};
use syn::{Data, DeriveInput, Fields, Type, ext::IdentExt, spanned::Spanned};

use crate::fields::{Derive, FormAttrs, assert_scalar, form_attrs};

pub fn expand(input: DeriveInput) -> TokenStream {
    expand_tokens(input).into()
}

/// The expansion over `proc_macro2` tokens: the proc-macro entry point converts
/// its result once, and the attribute checks stay unit-testable without a
/// proc-macro context.
fn expand_tokens(input: DeriveInput) -> TokenStream2 {
    match expand_checked(&input) {
        Ok(tokens) => tokens,
        Err(error) => error.to_compile_error(),
    }
}

fn expand_checked(input: &DeriveInput) -> syn::Result<TokenStream2> {
    // The attributes first: a misspelled key is reported whether or not the
    // consumer's manifest resolves.
    let shape = Shape::read(input)?;
    let krate = crate::tablo_core_path(&input.ident, "EmbeddedForm")?;
    Ok(match shape {
        Shape::Struct(fields) => expand_struct(&krate, input, &fields),
        Shape::Enum(variants) => expand_enum(&krate, input, &variants),
    })
}

/// One named field as the derive binds it.
struct Member {
    ident: syn::Ident,
    ty: Type,
    attrs: FormAttrs,
    /// `#[shared(..)]` (enum payloads): a column several variants declare.
    shared: bool,
}

struct VariantSpec {
    ident: syn::Ident,
    /// `None` for a unit variant.
    members: Option<Vec<Member>>,
}

enum Shape {
    Struct(Vec<Member>),
    Enum(Vec<VariantSpec>),
}

impl Shape {
    fn read(input: &DeriveInput) -> syn::Result<Self> {
        let unsupported = |expected: &str| {
            syn::Error::new_spanned(
                &input.ident,
                format!(
                    "#[derive(EmbeddedForm)] supports {expected}: an embedded value's fields are \
                     read and written by name (GH #191)"
                ),
            )
        };
        match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(named) => Ok(Shape::Struct(members(&named.named)?)),
                _ => Err(unsupported("a struct with named fields")),
            },
            Data::Enum(data) => data
                .variants
                .iter()
                .map(|variant| {
                    let members = match &variant.fields {
                        Fields::Named(named) => Some(members(&named.named)?),
                        Fields::Unit => None,
                        Fields::Unnamed(_) => {
                            return Err(unsupported("a struct or enum with named fields"));
                        }
                    };
                    Ok(VariantSpec {
                        ident: variant.ident.clone(),
                        members,
                    })
                })
                .collect::<syn::Result<Vec<_>>>()
                .map(Shape::Enum),
            Data::Union(_) => Err(unsupported("a struct or enum")),
        }
    }
}

fn members<'a>(fields: impl IntoIterator<Item = &'a syn::Field>) -> syn::Result<Vec<Member>> {
    fields
        .into_iter()
        .map(|field| {
            Ok(Member {
                ident: field.ident.clone().expect("named field"),
                ty: field.ty.clone(),
                attrs: form_attrs(field, Derive::Embedded)?,
                shared: field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("shared")),
            })
        })
        .collect()
}

/// The path to field `index` of `owner`, chained onto `parent`, rooted at
/// variant `variant` when it is `Some`.
///
/// The field's value type is the member's own, so the chained path is typed.
/// `Path::chain` drops the chained path's root, so this composes under whatever
/// parent it is chained onto. A variant payload is addressed from the variant
/// root: the enum's payload index is variant-local, so the variant step comes
/// first.
fn chained(
    krate: &TokenStream2,
    owner: &syn::Ident,
    ty: &Type,
    index: usize,
    variant: Option<usize>,
) -> TokenStream2 {
    let field = quote! { <#owner as #krate::__macro::Embed>::path_field::<#ty>(#index) };
    match variant {
        Some(variant) => quote! {
            parent.clone().chain(
                <#owner as #krate::__macro::Embed>::path_root()
                    .into_variant(#krate::__macro::VariantId {
                        model: <#owner as #krate::__macro::Embed>::id(),
                        index: #variant,
                    })
                    .chain(#field)
            )
        },
        None => quote! { parent.clone().chain(#field) },
    }
}

/// The label a derived control renders: the Rust field name, humanized.
///
/// The name is read through [`IdentExt::unraw`], so a raw identifier drops only
/// the `r#` prefix a keyword needs: `r#type` renders `Type`, not `R#type`.
fn label(ident: &syn::Ident) -> String {
    let name = ident.unraw().to_string();
    let mut out = String::with_capacity(name.len());
    for (i, part) in name.split('_').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    out
}

/// The builder call adding member `index`: a nested value's schema, or a
/// leaf's text field — labelled, multi-line when asked, and shared when
/// several variants declare its column.
fn build_member(
    krate: &TokenStream2,
    owner: &syn::Ident,
    member: &Member,
    index: usize,
    variant: Option<usize>,
) -> TokenStream2 {
    let ty = &member.ty;
    let path = chained(krate, owner, ty, index, variant);
    if member.attrs.embed {
        return quote! {
            builder.nested(<#ty as #krate::__macro::EmbeddedForm>::build_schema(dx, #path));
        };
    }
    let text = member
        .attrs
        .label
        .clone()
        .unwrap_or_else(|| label(&member.ident));
    let rows = member
        .attrs
        .multiline
        .map(|rows| quote! { .multiline(#rows) });
    let add = if member.shared {
        quote! { shared }
    } else {
        quote! { leaf }
    };
    // The assertion comes first and the leaf's only bound is `FormScalar`, so
    // a field of another type fails once, at the field.
    let assert = assert_scalar(krate, ty);
    let field = quote_spanned! {ty.span()=>
        #krate::__macro::Field::embedded_leaf::<_, #ty>(dx, #path)
    };
    quote! {
        #assert
        builder.#add(#field.label(#text)#rows);
    }
}

/// The write of member `index`, held in `binding`, through `node`.
fn write_member(
    krate: &TokenStream2,
    member: &Member,
    binding: &TokenStream2,
    index: usize,
    variant: &TokenStream2,
) -> TokenStream2 {
    let ty = &member.ty;
    if member.attrs.embed {
        return quote! {
            <#ty as #krate::__macro::EmbeddedForm>::write_node(
                #binding,
                node.nested(#variant, #index),
                out,
            );
        };
    }
    quote_spanned! {ty.span()=>
        out.insert(
            node.key(#variant, #index).to_string(),
            <#ty as #krate::__macro::FormScalar>::to_form(#binding),
        );
    }
}

/// The read of member `index` through `node`, its errors collected.
fn read_member(
    krate: &TokenStream2,
    member: &Member,
    index: usize,
    variant: &TokenStream2,
) -> TokenStream2 {
    let ty = &member.ty;
    if member.attrs.embed {
        return quote! {
            #krate::__macro::take_value(
                <#ty as #krate::__macro::EmbeddedForm>::read_node(
                    node.nested(#variant, #index),
                    values,
                ),
                &mut errors,
            )
        };
    }
    let blank = declared_blank(member);
    let read = quote_spanned! {ty.span()=>
        #krate::__macro::parse_leaf::<#ty>(node.key(#variant, #index), values, #blank)
    };
    quote! {
        #krate::__macro::take_leaf(#read, &mut errors)
    }
}

/// Member's declared blank answer, or `None` when it declares none.
fn declared_blank(member: &Member) -> TokenStream2 {
    let ty = &member.ty;
    match &member.attrs.blank {
        Some(expr) => quote_spanned! {ty.span()=>
            ::std::option::Option::Some(::std::convert::Into::<#ty>::into(#expr))
        },
        None => quote! { ::std::option::Option::None },
    }
}

/// Whether member's blank submission has an answer: a declared one, the
/// scalar's own, or — for a nested value — every leaf of that value's.
fn answers_blank(krate: &TokenStream2, member: &Member) -> TokenStream2 {
    let ty = &member.ty;
    if member.attrs.embed {
        return quote_spanned! {ty.span()=>
            <#ty as #krate::__macro::EmbeddedForm>::answers_blank()
        };
    }
    if member.attrs.blank.is_some() {
        return quote! { true };
    }
    quote_spanned! {ty.span()=>
        <#ty as #krate::__macro::FormScalar>::blank().is_some()
    }
}

/// A read body: read every member, collecting each one's errors into
/// `errors`, and build `ctor` only when none failed.
///
/// Each value binds under a prefixed name, so a field called `values` or
/// `errors` cannot shadow the reads after it.
fn collected_read(
    krate: &TokenStream2,
    ctor: &TokenStream2,
    members: &[Member],
    reads: &[TokenStream2],
) -> TokenStream2 {
    let names: Vec<&syn::Ident> = members.iter().map(|member| &member.ident).collect();
    let bindings: Vec<syn::Ident> = names
        .iter()
        .map(|name| format_ident!("__read_{}", name.unraw()))
        .collect();
    quote! {
        let mut errors: ::std::vec::Vec<#krate::__macro::FieldError> = ::std::vec::Vec::new();
        #( let #bindings = #reads; )*
        match (#(#bindings,)*) {
            (#(::std::option::Option::Some(#bindings),)*) if errors.is_empty() => {
                ::std::result::Result::Ok(#ctor { #(#names: #bindings),* })
            }
            _ => ::std::result::Result::Err(errors),
        }
    }
}

/// The trait impl plus the `form` constructor, around the four bodies.
fn wrap(
    krate: &TokenStream2,
    input: &DeriveInput,
    build: TokenStream2,
    write: TokenStream2,
    read: TokenStream2,
    answers_blank: TokenStream2,
) -> TokenStream2 {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    quote! {
        impl #impl_generics #krate::__macro::EmbeddedForm for #ident #ty_generics #where_clause {
            fn build_schema<M>(
                dx: &#krate::__macro::DeclCx,
                parent: #krate::__macro::Path<M, Self>,
            ) -> #krate::__macro::Schema
            where
                M: #krate::__macro::Model,
            {
                #build
            }

            fn answers_blank() -> bool {
                #answers_blank
            }

            fn write_node(
                &self,
                node: &#krate::__macro::Embedded,
                out: &mut ::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) {
                #write
            }

            fn read_node(
                node: &#krate::__macro::Embedded,
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) -> ::std::result::Result<Self, ::std::vec::Vec<#krate::__macro::FieldError>> {
                #read
            }
        }

        impl #impl_generics #ident #ty_generics #where_clause {
            /// The form schema for this embedded value under `parent`: one
            /// control per leaf column, resolved from the app schema.
            ///
            /// The app composes it into a layout —
            /// `Section::new("SEO").schema(Seo::form(dx, Post::fields().seo()))`
            /// — and declares no field bindings of its own.
            pub fn form<M>(
                dx: &#krate::__macro::DeclCx,
                parent: impl ::std::convert::Into<#krate::__macro::Path<M, Self>>,
            ) -> #krate::__macro::Schema
            where
                M: #krate::__macro::Model,
            {
                <Self as #krate::__macro::EmbeddedForm>::build_schema(dx, parent.into())
            }
        }
    }
}

/// One builder call, write, and read per named field.
fn expand_struct(krate: &TokenStream2, input: &DeriveInput, members: &[Member]) -> TokenStream2 {
    let owner = &input.ident;
    let none = quote! { ::std::option::Option::None };
    let adds = members
        .iter()
        .enumerate()
        .map(|(index, member)| build_member(krate, owner, member, index, None));
    let writes = members.iter().enumerate().map(|(index, member)| {
        let name = &member.ident;
        write_member(krate, member, &quote! { &self.#name }, index, &none)
    });
    let reads: Vec<TokenStream2> = members
        .iter()
        .enumerate()
        .map(|(index, member)| read_member(krate, member, index, &none))
        .collect();
    let build = quote! {
        let mut builder = #krate::__macro::EmbeddedBuilder::structure();
        #(#adds)*
        builder.finish()
    };
    let write = quote! { #(#writes)* };
    let read = collected_read(krate, &quote! { Self }, members, &reads);
    let blank = answers_blank_body(members.iter().map(|member| answers_blank(krate, member)));
    wrap(krate, input, build, write, read, blank)
}

/// The conjunction of every leaf's blank answer: a value answers a blank
/// submission when each of its leaves does. An empty conjunction is `true`, for
/// a value with no leaves.
fn answers_blank_body(members: impl Iterator<Item = TokenStream2>) -> TokenStream2 {
    let checks: Vec<TokenStream2> = members.collect();
    quote! { true #(&& #checks)* }
}

/// The variant control, then per variant its members, its write arm, and its
/// read arm.
fn expand_enum(
    krate: &TokenStream2,
    input: &DeriveInput,
    variants: &[VariantSpec],
) -> TokenStream2 {
    let owner = &input.ident;
    let mut adds = Vec::new();
    let mut write_arms = Vec::new();
    let mut read_arms = Vec::new();
    for (variant_index, variant) in variants.iter().enumerate() {
        let name = &variant.ident;
        let selector = quote! { ::std::option::Option::Some(#variant_index) };
        adds.push(quote! { builder.variant(); });
        let Some(members) = &variant.members else {
            // A unit variant carries no payload: only its discriminant names
            // it, and its group is empty.
            write_arms.push(quote! {
                Self::#name => node.write_variant(#variant_index, out),
            });
            read_arms.push(quote! {
                #variant_index => ::std::result::Result::Ok(Self::#name),
            });
            continue;
        };
        for (index, member) in members.iter().enumerate() {
            adds.push(build_member(
                krate,
                owner,
                member,
                index,
                Some(variant_index),
            ));
        }
        // Bound under prefixed names, so a payload called `node` or `out`
        // cannot shadow the writer's own arguments.
        let fields: Vec<&syn::Ident> = members.iter().map(|member| &member.ident).collect();
        let bindings: Vec<syn::Ident> = fields
            .iter()
            .map(|field| format_ident!("__field_{}", field.unraw()))
            .collect();
        let writes = members
            .iter()
            .zip(&bindings)
            .enumerate()
            .map(|(index, (member, binding))| {
                write_member(krate, member, &quote! { #binding }, index, &selector)
            });
        write_arms.push(quote! {
            Self::#name { #(#fields: #bindings),* } => {
                node.write_variant(#variant_index, out);
                #(#writes)*
            }
        });
        let reads: Vec<TokenStream2> = members
            .iter()
            .enumerate()
            .map(|(index, member)| read_member(krate, member, index, &selector))
            .collect();
        let read_body = collected_read(krate, &quote! { Self::#name }, members, &reads);
        read_arms.push(quote! {
            #variant_index => { #read_body }
        });
    }
    let build = quote! {
        let mut builder = #krate::__macro::EmbeddedBuilder::enumeration(dx, parent.clone());
        #(#adds)*
        builder.finish()
    };
    let write = quote! {
        match self {
            #(#write_arms)*
        }
    };
    let read = quote! {
        match node.variant_index(values)? {
            #(#read_arms)*
            other => ::std::unreachable!(
                "variant index {other} is outside {}: the app schema and this type disagree \
                 about the variants",
                ::std::stringify!(#owner),
            ),
        }
    };
    let blank = answers_blank_body(
        variants
            .iter()
            .filter_map(|variant| variant.members.as_deref())
            .flatten()
            .map(|member| answers_blank(krate, member)),
    );
    wrap(krate, input, build, write, read, blank)
}

#[cfg(test)]
mod tests;
