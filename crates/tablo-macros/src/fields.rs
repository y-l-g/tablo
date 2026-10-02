//! The field classifier both form derives share.
//!
//! A field is an **embedded value** when it is marked `#[form(embed)]` and a
//! **scalar** otherwise. A scalar's type must be a `FormScalar` (`String`, a
//! `TypedValue` type, or an `Option` of one); the derive asserts it with a
//! bound spanned on the field's type, so a `Vec<String>` field fails there,
//! naming the trait and the fix, rather than inside generated code.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use syn::{Type, spanned::Spanned};

/// Which derive reads the attributes: each accepts its own keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Derive {
    /// `#[derive(EmbeddedForm)]`: `embed`, `label = ".."`, `multiline = N`,
    /// `blank = <expr>`.
    Embedded,
    /// `#[derive(RecordForm)]`: `embed`, `blank = <expr>`, and the control
    /// keys `options = <Type>`, `choice`, `file`.
    Record,
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
    /// `#[form(options = <Type>)]`: a choice over an `Options` type's list.
    pub(crate) options: Option<syn::Path>,
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
            if meta.path.is_ident("embed") {
                out.embed = true;
            } else if meta.path.is_ident("label") && derive == Derive::Embedded {
                let text: syn::LitStr = meta.value()?.parse()?;
                out.label = Some(text.value());
            } else if meta.path.is_ident("multiline") && derive == Derive::Embedded {
                let rows: syn::LitInt = meta.value()?.parse()?;
                out.multiline = Some(rows.base10_parse()?);
            } else if meta.path.is_ident("blank") {
                out.blank = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("options") && derive == Derive::Record {
                out.options = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("choice") && derive == Derive::Record {
                out.choice = true;
            } else if meta.path.is_ident("file") && derive == Derive::Record {
                out.file = true;
            } else {
                let expected = match derive {
                    Derive::Embedded => {
                        "`embed`, `label = \"…\"`, `multiline = N`, or `blank = <expr>`"
                    }
                    Derive::Record => {
                        "`embed`, `blank = <expr>`, `options = <Type>`, `choice`, or `file`"
                    }
                };
                return Err(meta.error(format!("unknown `#[form(..)]` key: expected {expected}")));
            }
            Ok(())
        })?;
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
    if out.blank.is_some() && last_segment(&field.ty).is_some_and(|name| name == "Option") {
        return Err(syn::Error::new_spanned(
            &field.ty,
            "an `Option` field's blank answer is `None`",
        ));
    }
    Ok(out)
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
