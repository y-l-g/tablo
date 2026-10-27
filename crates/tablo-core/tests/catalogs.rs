//! The catalogs Tablo ships: English holds every message the code names and names no other, and
//! each other locale holds English's messages with English's variables.
//!
//! The checks read the sources, this crate's and its derives': a message id is a string literal
//! starting `tablo-`, which every `text!`, `t!` and `Text::message` call takes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

/// The `tablo-` literals the code holds that name no message: DOM ids, hooks, and the crate name
/// the derives resolve.
const NOT_MESSAGES: &[&str] = &[
    "tablo-condition",
    "tablo-core",
    "tablo-header-create",
    "tablo-table",
    "tablo-table-input",
    "tablo-tiptap",
    "tablo-variant",
];

/// Each message of a catalog, by id, with the variables its value and attributes read. An
/// attribute counts as its own message, `id.attribute`.
type Messages = BTreeMap<String, BTreeSet<String>>;

#[test]
fn english_holds_every_message_the_code_names_and_no_other() {
    let english = catalog("en");
    let named = named_ids();
    let missing: Vec<_> = named
        .iter()
        .filter(|id| !english.contains_key(*id))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "the code names messages `locales/en.ftl` does not hold (or list a literal naming no \
         message in `NOT_MESSAGES`):\n{}",
        missing.join("\n")
    );
    let unnamed: Vec<_> = english
        .keys()
        .filter(|id| !id.contains('.') && !named.contains(*id))
        .cloned()
        .collect();
    assert!(
        unnamed.is_empty(),
        "`locales/en.ftl` holds messages no code names:\n{}",
        unnamed.join("\n")
    );
}

#[test]
fn each_catalog_holds_englishs_messages_with_its_variables() {
    let english = catalog("en");
    for tag in locales().into_iter().filter(|tag| tag != "en") {
        let translated = catalog(&tag);
        let missing: Vec<_> = english
            .keys()
            .filter(|id| !translated.contains_key(*id))
            .cloned()
            .collect();
        let extra: Vec<_> = translated
            .keys()
            .filter(|id| !english.contains_key(*id))
            .cloned()
            .collect();
        let mismatched: Vec<_> = translated
            .iter()
            .filter(|(id, variables)| english.get(*id).is_some_and(|en| en != *variables))
            .map(|(id, variables)| format!("{id}: {variables:?}, English reads {:?}", english[id]))
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty() && mismatched.is_empty(),
            "`locales/{tag}.ftl` disagrees with English\nmissing:\n{}\nunknown:\n{}\nother \
             variables:\n{}",
            missing.join("\n"),
            extra.join("\n"),
            mismatched.join("\n")
        );
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The tags of the shipped catalogs.
fn locales() -> Vec<String> {
    let mut tags: Vec<String> = fs::read_dir(root().join("locales"))
        .expect("read the locales")
        .map(|entry| entry.expect("a locale").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
        .map(|path| {
            path.file_stem()
                .expect("a catalog's name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    tags.sort();
    tags
}

/// The messages of the catalog `tag`, which must parse.
fn catalog(tag: &str) -> Messages {
    let source = fs::read_to_string(root().join("locales").join(format!("{tag}.ftl")))
        .expect("read a catalog");
    let resource = fluent_syntax::parser::parse(source.as_str())
        .unwrap_or_else(|(_, errors)| panic!("`locales/{tag}.ftl` parses: {errors:?}"));
    let mut messages = Messages::new();
    for entry in resource.body {
        match entry {
            Entry::Message(message) => {
                let id = message.id.name.to_string();
                if let Some(value) = &message.value {
                    messages.insert(id.clone(), variables(value));
                }
                for attribute in &message.attributes {
                    messages.insert(
                        format!("{id}.{}", attribute.id.name),
                        variables(&attribute.value),
                    );
                }
            }
            Entry::Junk { content } => panic!("`locales/{tag}.ftl` holds junk: {content}"),
            Entry::Term(_)
            | Entry::Comment(_)
            | Entry::GroupComment(_)
            | Entry::ResourceComment(_) => {}
        }
    }
    messages
}

/// The variables `pattern` reads, in its placeables, selectors and variants.
fn variables(pattern: &Pattern<&str>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    pattern_variables(pattern, &mut found);
    found
}

fn pattern_variables(pattern: &Pattern<&str>, found: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            expression_variables(expression, found);
        }
    }
}

fn expression_variables(expression: &Expression<&str>, found: &mut BTreeSet<String>) {
    match expression {
        Expression::Select { selector, variants } => {
            inline_variables(selector, found);
            for variant in variants {
                pattern_variables(&variant.value, found);
            }
        }
        Expression::Inline(inline) => inline_variables(inline, found),
    }
}

fn inline_variables(inline: &InlineExpression<&str>, found: &mut BTreeSet<String>) {
    match inline {
        InlineExpression::VariableReference { id } => {
            found.insert(id.name.to_string());
        }
        InlineExpression::FunctionReference { arguments, .. } => {
            for argument in &arguments.positional {
                inline_variables(argument, found);
            }
            for argument in &arguments.named {
                inline_variables(&argument.value, found);
            }
        }
        InlineExpression::Placeable { expression } => expression_variables(expression, found),
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. }
        | InlineExpression::MessageReference { .. }
        | InlineExpression::TermReference { .. } => {}
    }
}

/// Every `tablo-` literal of the crate's and its derives' sources but [`NOT_MESSAGES`].
fn named_ids() -> BTreeSet<String> {
    let mut named = BTreeSet::new();
    // The derives' code renders with Tablo's catalog too.
    let derives = root().join("../tablo-macros/src");
    for file in sources(&root().join("src"))
        .into_iter()
        .chain(sources(&derives))
    {
        let text = fs::read_to_string(&file).expect("read a source");
        for (start, _) in text.match_indices("\"tablo-") {
            let rest = &text[start + 1..];
            let Some(end) = rest.find('"') else { continue };
            let literal = &rest[..end];
            // A literal a `format!` completes, such as a DOM id, holds a brace.
            let is_id = literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
            if is_id && !NOT_MESSAGES.contains(&literal) {
                named.insert(literal.to_string());
            }
        }
    }
    named
}

/// The `.rs` files under `dir`.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("read a source directory") {
        let path = entry.expect("a source").path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
    found
}
