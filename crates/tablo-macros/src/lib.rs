//! Procedural macros for Tablo.

mod embedded;
mod fields;
mod record_form;

use proc_macro::TokenStream;
use syn::DeriveInput;

/// Derive `EmbeddedForm` for an embedded struct or enum.
///
/// The generated impl builds the value's schema node from the columns the app
/// schema resolves for the parent path — one text field per leaf, a nested
/// node per `#[form(embed)]` value, and for an enum the variant control plus
/// one group per variant — and converts the value to and from the panel's flat
/// form map through that node's keys. It also generates `form(cx, parent)`,
/// the value's schema. The framework supplies the storage names, the derive
/// supplies the Rust shape.
///
/// ```ignore
/// #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
/// pub enum Publication {
///     #[column(variant = 1)]
///     Scheduled {
///         #[shared(timestamp)]
///         #[form(label = "Publication timestamp")]
///         scheduled_at: String,
///         scheduled_for: String,
///     },
///     #[column(variant = 2)]
///     Published { #[shared(timestamp)] published_at: String, canonical_url: String },
/// }
///
/// // form declaration — no field bindings written by hand
/// Section::new("Publication").schema(Publication::form(cx, Post::fields().publication()))
/// ```
///
/// # How a field is classified
///
/// A field marked `#[form(embed)]` is another **embedded value**, delegated to
/// its own `EmbeddedForm`. Every other field is a **scalar**: one column, read
/// and written through `FormScalar` (`String`, a `TypedValue` type, or an
/// `Option` of one). A scalar of another type fails to compile at the field,
/// naming the trait.
///
/// # Which variant an enum reads
///
/// A named discriminant always wins, and an undeclared one is refused;
/// otherwise the first variant, in declaration order, with a **payload of its
/// own** submitted — a `#[shared(..)]` column belongs to several variants and
/// never selects one; otherwise the first variant.
///
/// # Per-field attributes
///
/// - `#[form(embed)]` — a nested `EmbeddedForm` value.
/// - `#[form(label = "Canonical URL")]` — the control's label (default: the field name, humanized).
/// - `#[form(multiline = 3)]` — a `<textarea>` of 3 rows.
///
/// Anything else in `#[form(..)]` is a compile error, as are `label` and
/// `multiline` on an embedded value.
#[proc_macro_derive(EmbeddedForm, attributes(form))]
pub fn embedded_form(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    embedded::expand(input)
}

/// Derive `RecordForm` for the typed value a resource's form writes.
///
/// One field per model column the form writes, named and typed like the
/// model's field. A scalar (`String`, a `TypedValue` type, or an `Option` of
/// one) binds the key its control posts; a `#[form(embed)]` field binds every
/// key of an `EmbeddedForm` value and is written whole.
///
/// ```ignore
/// #[derive(tablo_core::RecordForm)]
/// #[form(model = User)]
/// pub struct UserForm {
///     pub name: String,
///     #[form(blank = "member")]
///     pub role: String,
///     #[form(blank = 0)]
///     pub age: i64,
/// }
/// ```
///
/// The derive also emits `UserFormField`, one variant per field, which
/// `FieldErrors` and `Posted` key on.
///
/// # Attributes
///
/// - `#[form(model = User)]` on the struct: the model the form writes.
/// - `#[form(blank = <expr>)]` on a scalar: the value an empty submission reads as. `String`
///   answers `""` and `Option<T>` answers `None` without one.
/// - `#[form(embed)]` on an `EmbeddedForm` value.
///
/// A generic struct, a tuple struct, an empty struct, a `Deferred<_>` field,
/// `blank` on an `Option` or an embedded value, and an unknown key are compile
/// errors. So are a field the model lacks, a type the model's field does not
/// have, and a scalar that is not a `FormScalar`.
#[proc_macro_derive(RecordForm, attributes(form))]
pub fn record_form(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    record_form::expand_tokens(input).into()
}

/// The path the generated code names `tablo-core` by: `::tablo_core`, or the
/// consumer's rename of it.
fn tablo_core_path(span: &syn::Ident, derive: &str) -> syn::Result<proc_macro2::TokenStream> {
    match proc_macro_crate::crate_name("tablo-core") {
        Ok(found) => {
            let name = match found {
                proc_macro_crate::FoundCrate::Itself => "tablo_core".to_string(),
                proc_macro_crate::FoundCrate::Name(n) => n,
            };
            let ident = syn::Ident::new(&name.replace('-', "_"), proc_macro2::Span::call_site());
            Ok(quote::quote! { ::#ident })
        }
        Err(_) => Err(syn::Error::new_spanned(
            span,
            format!("tablo-core must be a dependency to #[derive({derive})]"),
        )),
    }
}
