//! The `RecordForm`, `EmbeddedForm` and `Options` derives, re-exported by
//! `tablo-core`.

mod embedded;
mod fields;
mod options;
mod record_form;

use proc_macro::TokenStream;
use syn::DeriveInput;

/// Derives `EmbeddedForm` for an embedded struct or enum; documented on the `tablo-core` re-export.
#[proc_macro_derive(EmbeddedForm, attributes(form))]
pub fn embedded_form(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    embedded::expand(input)
}

/// Derive `RecordForm` for the typed value a resource's form writes; documented on the `tablo-core`
/// re-export.
#[proc_macro_derive(RecordForm, attributes(form))]
pub fn record_form(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    record_form::expand_tokens(input).into()
}

/// Derive `Options` for a unit-variant enum; documented on the `tablo-core` re-export.
#[proc_macro_derive(Options, attributes(option))]
pub fn options(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    options::expand_tokens(input).into()
}

/// The path the generated code names `tablo-core` by: the `tablo` facade, which
/// re-exports it at its root, else `::tablo_core`, either under the consumer's
/// rename. The facade comes first: it is what an app depends on, and it
/// reaches every path the generated code names.
fn tablo_core_path(span: &syn::Ident, derive: &str) -> syn::Result<proc_macro2::TokenStream> {
    let found = proc_macro_crate::crate_name("tablo")
        .map(|found| (found, "tablo"))
        .or_else(|_| proc_macro_crate::crate_name("tablo-core").map(|found| (found, "tablo_core")));
    match found {
        Ok((found, own_name)) => {
            let name = match found {
                proc_macro_crate::FoundCrate::Itself => own_name.to_string(),
                proc_macro_crate::FoundCrate::Name(n) => n,
            };
            let ident = syn::Ident::new(&name.replace('-', "_"), proc_macro2::Span::call_site());
            Ok(quote::quote! { ::#ident })
        }
        Err(_) => Err(syn::Error::new_spanned(
            span,
            format!("`tablo` (or `tablo-core`) must be a dependency to #[derive({derive})]"),
        )),
    }
}
