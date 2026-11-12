use super::*;

/// The names of the fields a column shows for `source`, or the refusal.
fn shown(source: &str) -> Result<Vec<String>, String> {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    fields(&input)
        .map(|fields| {
            fields
                .shown
                .iter()
                .map(|(field, _)| field.ident.as_ref().expect("named").to_string())
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
fn the_tenant_columns_are_the_fields_typed_tenant_id() {
    let input: DeriveInput = syn::parse_str(
        "struct M {
             #[key] #[auto] id: Uuid,
             tenant_id: TenantId,
             owner: Uuid,
             #[index] previous_tenant: Option<tablo::TenantId>,
         }",
    )
    .expect("the derive input parses");
    assert_eq!(
        fields(&input).unwrap().tenant_columns,
        ["tenant_id", "previous_tenant"]
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
    assert!(
        message.contains("drop its `#[columns(..)]` keys"),
        "{message}"
    );
    let message = shown("struct M { #[version] #[columns(label = \"V\")] v: u64 }").unwrap_err();
    assert!(
        message.contains("drop its `#[columns(..)]` keys"),
        "{message}"
    );
}

#[test]
fn the_presentation_keys_parse_and_a_shown_field_keeps_them() {
    let fields = shown(
        "struct M {
             #[columns(label = \"Total\", money = EUR, hidden_by_default)] total: i64,
             #[columns(badge, filter)] #[columns(label = \"State\")] status: Status,
         }",
    );
    assert_eq!(fields.unwrap(), ["total", "status"]);
}

#[test]
fn a_repeated_key_skip_beside_another_or_a_bare_money_is_refused() {
    for (source, expected) in [
        (
            "struct M { #[columns(badge, badge)] a: String }",
            "given twice",
        ),
        (
            "struct M { #[columns(badge)] #[columns(badge)] a: String }",
            "given twice",
        ),
        (
            "struct M { #[columns(skip, badge)] a: String }",
            "drop this key or `skip`",
        ),
        ("struct M { #[columns(money)] a: i64 }", "`money = USD`"),
        (
            "struct M { #[columns(label = 3)] a: i64 }",
            "expected string literal",
        ),
    ] {
        let message = shown(source).unwrap_err();
        assert!(message.contains(expected), "{source}: {message}");
    }
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

/// The currency `money = ..` passes to `Money::money`, for the field's `#[columns(..)]` source.
fn currency(attribute: &str) -> String {
    let input: DeriveInput =
        syn::parse_str(&format!("struct M {{ {attribute} a: i64 }}")).expect("the input parses");
    let syn::Data::Struct(data) = input.data else {
        unreachable!("the input is a struct")
    };
    let field = data.fields.iter().next().expect("one field");
    let calls = ColumnKeys::parse(field)
        .expect("the keys parse")
        .column_calls(&quote!(tablo), true)
        .to_string();
    let start = calls.find("__column ,").expect("a money call") + "__column ,".len();
    calls[start..calls.rfind(')').expect("the call closes")]
        .trim()
        .to_string()
}

#[test]
fn money_names_a_currency_constant_by_its_iso_code_and_takes_any_other_name_as_is() {
    assert_eq!(
        currency("#[columns(money = EUR)]"),
        "tablo :: Currency :: EUR"
    );
    assert_eq!(
        currency("#[columns(money = SHOP_CURRENCY)]"),
        "SHOP_CURRENCY"
    );
    assert_eq!(currency("#[columns(money = Eur)]"), "Eur");
    assert_eq!(
        currency("#[columns(money = shop::CURRENCY)]"),
        "shop :: CURRENCY"
    );
}
