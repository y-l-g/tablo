//! The `RecordForm`, `EmbeddedForm` and `Options` derives, re-exported by
//! `tablo-core`.

mod embedded;
mod fields;
mod options;
mod record_form;

use proc_macro::TokenStream;
use syn::DeriveInput;

/// Derives `EmbeddedForm` for an embedded struct or enum.
///
/// Builds the schema node and converts the value through that node's keys.
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     publication: Publication,
/// # }
/// # use tablo_core::Section;
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
///     Published {
///         #[shared(timestamp)]
///         published_at: String,
///         canonical_url: String,
///     },
/// }
///
/// // form declaration — no field bindings written by hand
/// Section::new("Publication").schema(Publication::form(Post::fields().publication()));
/// ```
///
/// # How a field is classified
///
/// A field marked `#[form(embed)]` is another **embedded value**, delegated to
/// its own `EmbeddedForm`. Every other field is a **scalar**: one column, read
/// and written through `FormScalar` (`String`, a `TypedValue` type, or an
/// `Option` of one). A scalar of another type fails to compile at the field,
/// naming the trait. An empty scalar is its declared `#[form(blank = ..)]`,
/// else its `FormScalar::blank()`; with neither, the parse refuses its key.
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
/// - `#[form(blank = ..)]` — what an empty submission reads as, overriding the leaf type's own
///   answer.
///
/// Anything else in `#[form(..)]` is a compile error, as are `label`, `multiline`, and `blank` on
/// an embedded value.
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
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # pub struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     role: String,
/// #     age: i64,
/// # }
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
/// `Posted` keys on and `RecordForm::fields` answers with each variant's keys.
/// It emits `UserFormControls`, one control per field chosen from the field —
/// a `bool` is a toggle, `#[form(options = T)]` a choice over `T`'s options,
/// `#[form(choice)]` a bare choice, `#[form(file)]` a file field,
/// `#[form(embed)]` the embedded value's schema, and any other field a text
/// field — with `controls()` handing them over and `RecordForm::schema`
/// arranging one per field in declaration order. `RecordForm::table` lists a
/// sortable column per text field, searchable over a `String` or
/// `Option<String>`, an options field by its option's label, and a toggle as
/// yes or no. A resource's `ResourceDef` defaults its form and table to them;
/// `ResourceDef::form` and `ResourceDef::table` arrange or extend them instead.
///
/// # Attributes
///
/// - `#[form(model = User)]` on the struct: the model the form writes.
/// - `#[form(blank = <expr>)]` on a scalar: the value an empty submission reads as, overriding the
///   default (`String` answers `""` and `Option<T>` answers `None` through the type's own blank,
///   and `bool` answers `false` through the derive's default).
/// - `#[form(options = Status)]`: a choice over `Status::options()`.
/// - `#[form(choice)]`: a bare choice, whose options or relationship the resource's `form` may add.
/// - `#[form(file)]` on a `String`: a file field.
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

/// Derive `Options` for a unit-variant enum: the `(value, label)` list a
/// choice field, a select filter and a column share.
///
/// ```rust
/// # use tablo_core::Options;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
/// pub enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// assert_eq!(Status::Published.value(), "published");
/// assert_eq!(Status::Published.label(), "Live");
/// assert_eq!(Status::from_value("draft"), Some(Status::Draft));
/// ```
///
/// Each variant stores its `snake_case` name and reads as that name in
/// sentence case. `#[option(value = "..")]` and `#[option(label = "..")]`
/// override either. A generic enum, a variant with fields, two variants
/// storing one value, and an unknown key are compile errors.
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
