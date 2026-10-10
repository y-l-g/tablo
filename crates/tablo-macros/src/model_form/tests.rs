use super::*;

/// The `RecordForm` input `source` derives, or its refusal.
fn expand(source: &str) -> Result<String, String> {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    form_of(&input)
        .map(|form| quote::ToTokens::to_token_stream(&form).to_string())
        .map_err(|error| error.to_string())
}

/// The fields of the form `source` derives.
fn fields(source: &str) -> Vec<syn::Field> {
    let input: DeriveInput = syn::parse_str(source).expect("the derive input parses");
    let form = form_of(&input).expect("the derive expands");
    let Data::Struct(data) = form.data else {
        unreachable!("a struct");
    };
    data.fields.into_iter().collect()
}

/// The names of the fields of the form `source` derives.
fn names(source: &str) -> Vec<String> {
    fields(source)
        .iter()
        .map(|field| field.ident.as_ref().expect("named").to_string())
        .collect()
}

#[test]
fn the_form_holds_the_fields_a_person_fills() {
    let fields = names(
        "#[unique(tenant_id, email)]
         pub struct Author {
             #[key] #[auto] pub id: Uuid,
             #[index] pub tenant_id: TenantId,
             /// Shown on the byline.
             pub name: String,
             email: String,
             #[default(Timestamp::now())] pub created_at: Timestamp,
             #[update(Timestamp::now())] pub updated_at: Timestamp,
             #[version] pub version: u64,
             #[has_many] pub posts: Deferred<Vec<Post>>,
         }",
    );
    assert_eq!(fields, ["name", "email"]);
}

#[test]
fn a_form_field_keeps_its_visibility_docs_and_form_keys() {
    let fields = fields(
        "pub struct Book {
            /// The cover title.
            #[index]
            #[form(optional)]
            pub title: String,
            pub(crate) note: String,
        }",
    );
    assert!(matches!(fields[0].vis, syn::Visibility::Public(_)));
    assert!(matches!(fields[1].vis, syn::Visibility::Restricted(_)));
    let attrs: Vec<String> = fields[0]
        .attrs
        .iter()
        .map(|attr| attr.path().get_ident().expect("an ident").to_string())
        .collect();
    assert_eq!(attrs, ["doc", "form"]);
}

#[test]
fn the_form_is_named_after_the_model_unless_named() {
    let expansion = expand("struct Book { title: String }").expect("expands");
    assert!(expansion.contains("struct BookForm"), "{expansion}");
    assert!(expansion.contains("# [form (model = Book)]"), "{expansion}");
    let expansion =
        expand("#[form(name = Editor)] struct Book { title: String }").expect("expands");
    assert!(expansion.contains("struct Editor"), "{expansion}");
}

#[test]
fn a_foreign_key_or_a_defaulted_field_joins_on_a_form_key() {
    let source = "struct Comment {
        body: String,
        #[default(false)] hidden: bool,
        post_id: Uuid,
        #[belongs_to(key = post_id, references = id)] post: Deferred<Post>,
        editor_id: Option<Uuid>,
        #[belongs_to] editor: Deferred<Option<User>>,
    }";
    assert_eq!(names(source), ["body"]);
    let source = "struct Comment {
        body: String,
        #[default(false)] #[form(blank = false)] hidden: bool,
        #[form(relationship = Posts)] post_id: Uuid,
        #[belongs_to(key = post_id, references = id)] post: Deferred<Post>,
    }";
    assert_eq!(names(source), ["body", "hidden", "post_id"]);
}

#[test]
fn a_composite_key_is_left_out() {
    let source = "#[key(shelf, slot)] struct Placement { shelf: String, slot: i64, note: String }";
    assert_eq!(names(source), ["note"]);
    let source = "#[key(name = \"pk\", shelf)] struct Placement { shelf: String, name: String }";
    assert_eq!(names(source), ["name"]);
    let source = "#[key(partition = [shelf], local = slot)]
        struct Placement { shelf: String, slot: i64, local: String, partition: String }";
    assert_eq!(names(source), ["local", "partition"]);
}

#[test]
fn a_composite_foreign_key_is_left_out() {
    let source = "struct Copy {
        title: String,
        shelf: String,
        slot: i64,
        #[belongs_to(key = [shelf, slot], references = [shelf, slot])] place: Deferred<Placement>,
    }";
    assert_eq!(names(source), ["title"]);
}

#[test]
fn skip_leaves_a_field_out_and_takes_no_other_key() {
    assert_eq!(
        names("struct Book { title: String, #[form(skip)] isbn: String }"),
        ["title"]
    );
    let message = expand("struct Book { title: String, #[form(skip, optional)] isbn: String }")
        .expect_err("refused");
    assert!(message.contains("no other"), "{message}");
}

#[test]
fn a_form_key_on_a_field_no_form_writes_is_refused_with_its_fix() {
    for (field, reason, fix) in [
        (
            "#[key] #[auto] #[form(optional)] id: Uuid",
            "key",
            "drop the",
        ),
        (
            "#[form(optional)] tenant_id: Option<TenantId>",
            "Tenancy",
            "drop the",
        ),
        (
            "#[auto] #[form(optional)] created_at: Timestamp",
            "Toasty writes it",
            "drop the",
        ),
        (
            "#[belongs_to] #[form(relationship = Authors)] author: Deferred<Author>",
            "foreign key",
            "put the key on the foreign key",
        ),
        (
            "#[has_many(via = taggings.tag)] #[form(relationship = Tags)] tags: Deferred<Vec<Tag>>",
            "many-to-many",
            "RecordForm",
        ),
        (
            "#[has_many] #[form(optional)] comments: Deferred<Vec<Comment>>",
            "related records",
            "drop the",
        ),
    ] {
        let message =
            expand(&format!("struct Book {{ title: String, {field} }}")).expect_err("refused");
        assert!(message.contains(reason), "{field}: {message}");
        assert!(message.contains(fix), "{field}: {message}");
    }
}

#[test]
fn skip_on_a_field_no_form_writes_is_refused() {
    let message =
        expand("struct Book { title: String, #[auto] #[form(skip)] created_at: Timestamp }")
            .expect_err("refused");
    assert!(message.contains("drop `skip`"), "{message}");
}

#[test]
fn a_model_with_nothing_to_fill_is_refused() {
    let message = expand("struct Tick { #[key] #[auto] id: Uuid }").expect_err("refused");
    assert!(message.contains("every field"), "{message}");
}

#[test]
fn a_named_model_or_an_unknown_key_is_refused() {
    let message = expand("#[form(model = Book)] struct Book { title: String }").expect_err("x");
    assert!(message.contains("drop `model"), "{message}");
    let message = expand("#[form(nme = X)] struct Book { title: String }").expect_err("x");
    assert!(message.contains("unknown"), "{message}");
}

#[test]
fn a_generic_model_or_an_enum_is_refused() {
    let message = expand("struct Book<T> { title: T }").expect_err("refused");
    assert!(message.contains("generic"), "{message}");
    let message = expand("enum Book { A }").expect_err("refused");
    assert!(message.contains("toasty::Model"), "{message}");
}
