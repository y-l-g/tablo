//! The catalogs the audit log ships: English holds every message its code names and no other, and
//! French holds English's messages with English's variables.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

/// Each message of a catalog, by id, with the variables its value reads.
type Messages = BTreeMap<String, BTreeSet<String>>;

#[test]
fn english_holds_every_message_the_code_names_and_no_other() {
    let english: BTreeSet<String> = catalog("en").into_keys().collect();
    assert_eq!(english, named_ids());
}

#[test]
fn french_holds_englishs_messages_with_its_variables() {
    assert_eq!(catalog("fr"), catalog("en"));
}

fn catalog(locale: &str) -> Messages {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("locales/{locale}.ftl"));
    let source = fs::read_to_string(&path).expect("read a catalog");
    let resource = fluent_syntax::parser::parse(source).expect("the catalog parses");
    resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(message) => {
                let mut variables = BTreeSet::new();
                if let Some(value) = &message.value {
                    pattern_variables(value, &mut variables);
                }
                Some((message.id.name.clone(), variables))
            }
            _ => None,
        })
        .collect()
}

fn pattern_variables(pattern: &Pattern<String>, into: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            expression_variables(expression, into);
        }
    }
}

fn expression_variables(expression: &Expression<String>, into: &mut BTreeSet<String>) {
    match expression {
        Expression::Inline(inline) => inline_variables(inline, into),
        Expression::Select { selector, variants } => {
            inline_variables(selector, into);
            for variant in variants {
                pattern_variables(&variant.value, into);
            }
        }
    }
}

fn inline_variables(inline: &InlineExpression<String>, into: &mut BTreeSet<String>) {
    match inline {
        InlineExpression::VariableReference { id } => {
            into.insert(id.name.clone());
        }
        InlineExpression::Placeable { expression } => expression_variables(expression, into),
        _ => {}
    }
}

/// The `tablo-audit-` string literals the crate's sources hold.
fn named_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read a source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let text = fs::read_to_string(&path).expect("read a source");
            for literal in text.split('"').skip(1).step_by(2) {
                if literal.starts_with("tablo-audit-") {
                    ids.insert(literal.to_owned());
                }
            }
        }
    }
    ids
}
