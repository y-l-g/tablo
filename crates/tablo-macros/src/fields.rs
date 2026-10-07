//! The field classifier both form derives share.
//!
//! A field is an **embedded value** when it is marked `#[form(embed)]` and a
//! **scalar** otherwise. A scalar's type must be a `FormScalar` (`String`, a
//! `TypedValue` type, an `Options` enum, or an `Option` of one); the derive
//! asserts it with a bound spanned on the field's type, so a `Vec<String>`
//! field fails there, naming the trait and the fix, rather than inside
//! generated code.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use syn::{Type, spanned::Spanned};

/// Which derive reads the attributes: each accepts its own keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Derive {
    /// `#[derive(EmbeddedForm)]`: `embed`, `label = ".."`, `multiline = N`,
    /// `blank = <expr>`, `optional`.
    Embedded,
    /// `#[derive(RecordForm)]`: `embed`, `blank = <expr>`, `optional`, and the
    /// control keys `options`, `options = <Type>`, `choice`, `file`.
    Record,
    /// `#[derive(ActionInput)]`: `label = ".."`, `multiline = N`, `blank = <expr>`, `optional`,
    /// `options`, `options = <Type>`.
    Input,
}

/// What `#[form(..)]` says about one field.
#[derive(Default)]
pub(crate) struct FormAttrs {
    /// `#[form(embed)]`: an `EmbeddedForm` value, bound whole.
    pub(crate) embed: bool,
    /// `#[form(label = "..")]`: the control's label.
    pub(crate) label: Option<String>,
    /// `#[form(multiline = N)]`: a `<textarea>` of `N` rows.
    pub(crate) multiline: Option<u32>,
    /// `#[form(blank = <expr>)]`: what an empty submission reads as.
    pub(crate) blank: Option<syn::Expr>,
    /// `#[form(optional)]` on a `String`: an empty submission reads as `""`.
    pub(crate) optional: bool,
    /// `#[form(options = <Type>)]`: a choice over an `Options` type's list, `Some(None)` for a
    /// bare `#[form(options)]` over the field's own type.
    pub(crate) options: Option<Option<Type>>,
    /// `#[form(choice)]`: a bare choice, its options declared in `form()`.
    pub(crate) choice: bool,
    /// `#[form(file)]`: a file field.
    pub(crate) file: bool,
}

/// Every `#[form(..)]` attribute on `field`, rejecting unknown, unread, and misplaced keys at the
/// attribute.
pub(crate) fn form_attrs(field: &syn::Field, derive: Derive) -> syn::Result<FormAttrs> {
    let mut out = FormAttrs::default();
    for attr in &field.attrs {
        if !attr.path().is_ident("form") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("embed") && derive != Derive::Input {
                out.embed = true;
            } else if meta.path.is_ident("label") && derive != Derive::Record {
                let text: syn::LitStr = meta.value()?.parse()?;
                out.label = Some(text.value());
            } else if meta.path.is_ident("multiline") && derive != Derive::Record {
                let rows: syn::LitInt = meta.value()?.parse()?;
                out.multiline = Some(rows.base10_parse()?);
            } else if meta.path.is_ident("blank") {
                out.blank = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("optional") {
                out.optional = true;
            } else if meta.path.is_ident("options") && derive != Derive::Embedded {
                out.options = Some(if meta.input.peek(syn::Token![=]) {
                    Some(meta.value()?.parse()?)
                } else {
                    None
                });
            } else if meta.path.is_ident("choice") && derive == Derive::Record {
                out.choice = true;
            } else if meta.path.is_ident("file") && derive == Derive::Record {
                out.file = true;
            } else {
                let expected = match derive {
                    Derive::Embedded => {
                        "`embed`, `label = \"…\"`, `multiline = N`, `blank = <expr>`, or \
                         `optional`"
                    }
                    Derive::Record => {
                        "`embed`, `blank = <expr>`, `optional`, `options`, `options = <Type>`, \
                         `choice`, or `file`"
                    }
                    Derive::Input => {
                        "`label = \"…\"`, `multiline = N`, `blank = <expr>`, `optional`, \
                         `options`, or `options = <Type>`"
                    }
                };
                return Err(meta.error(format!("unknown `#[form(..)]` key: expected {expected}")));
            }
            Ok(())
        })?;
    }
    if out.multiline.is_some() && out.options.is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "`multiline` renders a `<textarea>` and `options` a choice: declare one",
        ));
    }
    let controls = [
        (out.options.is_some(), "`options`"),
        (out.choice, "`choice`"),
        (out.file, "`file`"),
    ];
    let chosen: Vec<&str> = controls
        .iter()
        .filter(|(set, _)| *set)
        .map(|(_, key)| *key)
        .collect();
    if chosen.len() > 1 {
        return Err(syn::Error::new_spanned(
            field,
            format!(
                "{} each pick the field's control: declare one",
                chosen.join(" and ")
            ),
        ));
    }
    if out.embed {
        let misplaced = [
            (out.label.is_some(), "`label`"),
            (out.multiline.is_some(), "`multiline`"),
            (out.blank.is_some(), "`blank`"),
            (out.optional, "`optional`"),
            (out.options.is_some(), "`options`"),
            (out.choice, "`choice`"),
            (out.file, "`file`"),
        ];
        if let Some((_, key)) = misplaced.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                field,
                format!(
                    "{key} does not apply to an embedded value: its own fields declare their \
                     controls and their blank answers"
                ),
            ));
        }
    }
    if out.embed {
        return Ok(out);
    }
    let ty = last_segment(&field.ty);
    if (out.blank.is_some() || out.optional) && ty.as_deref() == Some("Option") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "an `Option` field is optional already: its blank answer is `None`",
        ));
    }
    if out.optional && out.blank.is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "`optional` and `blank` each declare the blank answer: declare one",
        ));
    }
    if out.optional && ty.as_deref() != Some("String") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "`optional` reads an empty `String` as `\"\"`; another type has no empty value: \
             declare `blank = <value>`, or make the field an `Option`",
        ));
    }
    Ok(out)
}

/// The value a scalar reads an empty submission as, or `None` when the field is required: its
/// declared `blank`, else `Default::default()` for an `Option` (`None`), a `bool` (`false`, what
/// an unchecked toggle means) and an `optional` `String` (`""`).
pub(crate) fn blank_answer(ty: &Type, attrs: &FormAttrs) -> Option<TokenStream2> {
    if let Some(expr) = &attrs.blank {
        return Some(quote_spanned! {ty.span()=>
            ::std::convert::Into::<#ty>::into(#expr)
        });
    }
    let defaulted =
        attrs.optional || matches!(last_segment(ty).as_deref(), Some("Option" | "bool"));
    defaulted.then(|| {
        quote_spanned! {ty.span()=>
            <#ty as ::std::default::Default>::default()
        }
    })
}

/// `blank_answer` spelled as the `Option` the parse takes.
pub(crate) fn blank_option(ty: &Type, attrs: &FormAttrs) -> TokenStream2 {
    match blank_answer(ty, attrs) {
        Some(blank) => quote::quote! { ::std::option::Option::Some(#blank) },
        None => quote::quote! { ::std::option::Option::None },
    }
}

/// Asserts `ty` is a form scalar, spanned on the type so the error names the field.
pub(crate) fn assert_scalar(krate: &TokenStream2, ty: &Type) -> TokenStream2 {
    quote_spanned! {ty.span()=>
        #krate::__macro::assert_form_scalar::<#ty>();
    }
}

pub(crate) fn last_segment(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
