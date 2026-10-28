//! The `{Form}Field` variant each field of a record form or a form input names.

use quote::format_ident;

/// The `{Form}Field` variant of each of `fields`, in order: its name in Pascal case. Refuses, at
/// the later field, two fields whose names make one variant, such as `foo_bar` and `foo__bar`,
/// and a name that makes no identifier, such as `_1`.
pub(crate) fn field_variants<'f>(
    fields: impl IntoIterator<Item = &'f syn::Field>,
) -> syn::Result<Vec<syn::Ident>> {
    let mut variants: Vec<(String, &syn::Ident)> = Vec::new();
    for field in fields {
        let ident = field.ident.as_ref().expect("named field");
        let name = pascal_case(&ident.to_string());
        if !name.starts_with(|first: char| first.is_alphabetic()) {
            return Err(syn::Error::new_spanned(
                ident,
                format!("`{ident}` names no `..Field` variant: start the name with a letter"),
            ));
        }
        if let Some((_, first)) = variants.iter().find(|(taken, _)| *taken == name) {
            return Err(syn::Error::new_spanned(
                ident,
                format!(
                    "`{ident}` and `{first}` both make the `..Field` variant `{name}`: rename \
                     one"
                ),
            ));
        }
        variants.push((name, ident));
    }
    Ok(variants
        .into_iter()
        .map(|(name, _)| format_ident!("{name}"))
        .collect())
}

/// `author_id` → `AuthorId`; a raw identifier drops its `r#`.
pub(crate) fn pascal_case(name: &str) -> String {
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
