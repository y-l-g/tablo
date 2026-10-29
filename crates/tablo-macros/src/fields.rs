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
use syn::{Data, DeriveInput, Fields, Type, spanned::Spanned};

/// Which derive reads the attributes: each accepts its own keys.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Derive {
    /// `#[derive(EmbeddedForm)]`: `embed`, `label = ".."`, `multiline = N`,
    /// `blank = <expr>`, `optional`.
    Embedded,
    /// `#[derive(RecordForm)]`: `embed`, `blank = <expr>`, `optional`, and the
    /// control keys `options`, `options = <Type>`, `relationship = <Source>`, `file`.
    Record,
    /// `#[derive(FormInput)]`: `label = ".."`, `multiline = N`, `placeholder = ".."`, `email`,
    /// `url`, `tel`, `password`, `blank = <expr>`, `optional`, `options`, `options = <Type>`.
    Input,
}

impl Derive {
    /// Whether the derive's fields post their own names, as a form input's do.
    fn named(self) -> bool {
        self == Self::Input
    }
}

pub(crate) fn reject_generics(input: &DeriveInput, message: &str) -> syn::Result<()> {
    if input.generics.params.is_empty() {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(&input.generics, message))
    }
}

pub(crate) fn named_fields<'a>(
    input: &'a DeriveInput,
    shape: &str,
    empty: Option<&str>,
) -> syn::Result<&'a syn::FieldsNamed> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) if !named.named.is_empty() => Ok(named),
            Fields::Named(named) => match empty {
                Some(message) => Err(syn::Error::new_spanned(&input.ident, message)),
                None => Ok(named),
            },
            _ => Err(syn::Error::new_spanned(&input.ident, shape)),
        },
        _ => Err(syn::Error::new_spanned(&input.ident, shape)),
    }
}

/// What `#[form(..)]` says about one field.
#[derive(Default)]
pub(crate) struct FormAttrs {
    /// `#[form(embed)]`: an `EmbeddedForm` value, bound whole.
    pub(crate) embed: bool,
    /// `#[form(repeat)]`: a list of `FormInput` values, bound whole.
    pub(crate) repeat: bool,
    /// `#[form(label = "..")]`: the control's label.
    pub(crate) label: Option<String>,
    /// `#[form(multiline = N)]`: a `<textarea>` of `N` rows.
    pub(crate) multiline: Option<u32>,
    /// `#[form(placeholder = "..")]`: a text control's placeholder.
    pub(crate) placeholder: Option<String>,
    /// `#[form(blank = <expr>)]`: what an empty submission reads as.
    pub(crate) blank: Option<syn::Expr>,
    /// `#[form(optional)]` on a `String`: an empty submission reads as `""`.
    pub(crate) optional: bool,
    /// `#[form(options = <Type>)]`: a choice over an `Options` type's list, `Some(None)` for a
    /// bare `#[form(options)]` over the field's own type.
    pub(crate) options: Option<Option<Type>>,
    /// `#[form(relationship = <Source>)]`: a choice over an `OptionSource`'s rows.
    pub(crate) relationship: Option<Type>,
    /// `#[form(file)]`: a file field.
    pub(crate) file: bool,
    /// `#[form(tags)]`: a multiple choice taking typed values, its `options` suggestions.
    pub(crate) tags: bool,
    /// `#[form(email)]`: a text control validating an email address.
    pub(crate) email: bool,
    /// `#[form(url)]`: a text control validating an `http` or `https` URL.
    pub(crate) url: bool,
    /// `#[form(tel)]`: a text control for a telephone number.
    pub(crate) tel: bool,
    /// `#[form(password)]`: a masked text control that never renders its value back.
    pub(crate) password: bool,
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
            if meta.path.is_ident("embed") && !derive.named() {
                out.embed = true;
            } else if meta.path.is_ident("repeat") && derive == Derive::Record {
                out.repeat = true;
            } else if meta.path.is_ident("label") && derive != Derive::Record {
                let text: syn::LitStr = meta.value()?.parse()?;
                out.label = Some(text.value());
            } else if meta.path.is_ident("multiline") && derive != Derive::Record {
                let rows: syn::LitInt = meta.value()?.parse()?;
                out.multiline = Some(rows.base10_parse()?);
            } else if meta.path.is_ident("placeholder") && derive.named() {
                let text: syn::LitStr = meta.value()?.parse()?;
                out.placeholder = Some(text.value());
            } else if meta.path.is_ident("email") && derive.named() {
                out.email = true;
            } else if meta.path.is_ident("url") && derive.named() {
                out.url = true;
            } else if meta.path.is_ident("tel") && derive.named() {
                out.tel = true;
            } else if meta.path.is_ident("password") && derive == Derive::Input {
                out.password = true;
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
            } else if meta.path.is_ident("relationship") && derive == Derive::Record {
                out.relationship = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("choice") && derive == Derive::Record {
                return Err(meta.error(
                    "a bare choice offers nothing to choose: declare `relationship = <Source>`, or \
                     call `.choice()` on the control in the resource's form and give it options",
                ));
            } else if meta.path.is_ident("file") && derive == Derive::Record {
                out.file = true;
            } else if meta.path.is_ident("tags") && derive == Derive::Record {
                out.tags = true;
            } else {
                let expected = match derive {
                    Derive::Embedded => {
                        "`embed`, `label = \"…\"`, `multiline = N`, `blank = <expr>`, or \
                         `optional`"
                    }
                    Derive::Record => {
                        "`embed`, `repeat`, `blank = <expr>`, `optional`, `options`, \
                         `options = <Type>`, `relationship = <Source>`, `file`, or `tags`"
                    }
                    Derive::Input => {
                        "`label = \"…\"`, `multiline = N`, `placeholder = \"…\"`, `email`, \
                         `url`, `tel`, `password`, `blank = <expr>`, `optional`, `options`, or \
                         `options = <Type>`"
                    }
                };
                return Err(meta.error(format!("unknown `#[form(..)]` key: expected {expected}")));
            }
            Ok(())
        })?;
    }
    if out.options.is_some() {
        let text_only = [
            (out.multiline.is_some(), "`multiline`"),
            (out.placeholder.is_some(), "`placeholder`"),
            (out.email, "`email`"),
            (out.url, "`url`"),
            (out.tel, "`tel`"),
            (out.password, "`password`"),
        ];
        if let Some((_, key)) = text_only.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                field,
                format!("{key} applies to a text input, and `options` makes a choice: declare one"),
            ));
        }
    }
    // `tags` takes `options = <Type>` as its suggestions.
    let controls = [
        (out.options.is_some() && !out.tags, "`options`"),
        (out.relationship.is_some(), "`relationship`"),
        (out.file, "`file`"),
        (out.tags, "`tags`"),
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
    if out.embed && out.repeat {
        return Err(syn::Error::new_spanned(
            field,
            "`embed` and `repeat` each bind the field whole: declare one",
        ));
    }
    if out.repeat {
        let misplaced = [
            (out.blank.is_some(), "`blank`"),
            (out.optional, "`optional`"),
            (out.options.is_some(), "`options`"),
            (out.relationship.is_some(), "`relationship`"),
            (out.file, "`file`"),
            (out.tags, "`tags`"),
        ];
        if let Some((_, key)) = misplaced.iter().find(|(set, _)| *set) {
            return Err(syn::Error::new_spanned(
                field,
                format!(
                    "{key} does not apply to a repeater: its item's fields declare their controls \
                     and their blank answers, and no rows is its blank answer"
                ),
            ));
        }
        return Ok(out);
    }
    if out.embed {
        let misplaced = [
            (out.label.is_some(), "`label`"),
            (out.multiline.is_some(), "`multiline`"),
            (out.blank.is_some(), "`blank`"),
            (out.optional, "`optional`"),
            (out.options.is_some(), "`options`"),
            (out.relationship.is_some(), "`relationship`"),
            (out.file, "`file`"),
            (out.tags, "`tags`"),
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

/// A snake_case name as a label in sentence case, the casing every default label in Tablo uses:
/// `canonical_url` → `Canonical url`, `published_late` → `Published late`.
pub(crate) fn sentence_case(name: &str) -> String {
    let words: Vec<&str> = name.split('_').filter(|word| !word.is_empty()).collect();
    let spaced = words.join(" ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests;
