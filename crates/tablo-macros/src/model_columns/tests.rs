use super::*;

/// The names of the fields a column shows for `source`, or the refusal.
fn shown(source: &str) -> Result<Vec<String>, String> {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    shown_fields(&input)
        .map(|fields| {
            fields
                .iter()
                .map(|field| field.ident.as_ref().expect("named").to_string())
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[test]
fn the_columns_show_what_a_person_reads_whoever_writes_it() {
    let fields = shown(
        "pub struct Post {
             #[key] #[auto] pub id: Uuid,
             #[index] pub tenant_id: TenantId,
             pub title: String,
             #[columns(skip)] pub token: String,
             #[default(Timestamp::now())] pub created_at: Timestamp,
             #[update(Timestamp::now())] pub updated_at: Timestamp,
             #[auto] pub published_at: Timestamp,
             #[version] pub version: u64,
             pub seo: Seo,
             #[document] pub links: Vec<Link>,
             #[index] pub writer: Uuid,
             #[belongs_to(key = writer, references = id)] pub author: Deferred<Author>,
             pub editor_id: Option<Uuid>,
             #[belongs_to] pub editor: Deferred<Option<Author>>,
             #[has_many] pub comments: Deferred<Vec<Comment>>,
             #[has_many(via = post_tags.tag)] pub tags: Deferred<Vec<Tag>>,
         }",
    );
    assert_eq!(
        fields.unwrap(),
        [
            "title",
            "created_at",
            "updated_at",
            "published_at",
            "seo",
            "links"
        ]
    );
}

#[test]
fn a_generated_key_is_left_out_wherever_it_is_named() {
    let fields = shown(
        "#[key(id)] struct M { #[auto] id: u64, #[auto] created_at: Timestamp, slug: String }",
    );
    assert_eq!(fields.unwrap(), ["created_at", "slug"]);
}

#[test]
fn skip_on_a_field_no_column_shows_is_refused() {
    let message = shown("struct M { #[key] #[auto] #[columns(skip)] id: Uuid }").unwrap_err();
    assert!(message.contains("drop `#[columns(skip)]`"), "{message}");
}

#[test]
fn an_unknown_key_or_a_struct_level_key_is_refused() {
    let message = shown("struct M { #[columns(hide)] a: String }").unwrap_err();
    assert!(message.contains("unknown"), "{message}");
    let message = shown("#[columns(skip)] struct M { a: String }").unwrap_err();
    assert!(message.contains("goes on a field"), "{message}");
}

#[test]
fn a_generic_model_is_refused() {
    let message = shown("struct M<T> { a: T }").unwrap_err();
    assert!(message.contains("generic"), "{message}");
}
