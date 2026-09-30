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
    /// `#[derive(EmbeddedForm)]`: `embed`, `label = ".."`, `multiline = N`.
    Embedded,
    /// `#[derive(RecordForm)]`: `embed`, `blank = <expr>`.
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
}

/// Every `#[form(..)]` attribute on `field`, checked.
///
/// An unknown key, a key the derive does not read, and a key that means
/// nothing on an embedded value are compile errors at the attribute rather
/// than silent no-ops.
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
            } else if meta.path.is_ident("blank") && derive == Derive::Record {
                out.blank = Some(meta.value()?.parse()?);
            } else {
                let expected = match derive {
                    Derive::Embedded => "`embed`, `label = \"…\"`, or `multiline = N`",
                    Derive::Record => "`embed` or `blank = <expr>`",
                };
                return Err(meta.error(format!("unknown `#[form(..)]` key: expected {expected}")));
            }
            Ok(())
        })?;
    }
    if out.embed {
        let misplaced = [
            (out.label.is_some(), "`label`"),
            (out.multiline.is_some(), "`multiline`"),
            (out.blank.is_some(), "`blank`"),
        ];
        if let Some((_, key)) = misplaced.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                field,
                format!(
                    "{key} does not apply to an embedded value: its own fields declare their \
                     controls, and its leaves read an empty key as `Default`"
                ),
            ));
        }
    }
    Ok(out)
}

/// A call asserting that `ty` is a form scalar, spanned on the type so the
/// error names the field.
pub(crate) fn assert_scalar(krate: &TokenStream2, ty: &Type) -> TokenStream2 {
    quote_spanned! {ty.span()=>
        #krate::__macro::assert_form_scalar::<#ty>();
    }
}

#[cfg(test)]
mod tests;
