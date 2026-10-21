//! The crate's layers: a module names the modules listed before it, in its own layer or a lower
//! one, never one listed after it; only the serving layer and the render hooks below it name
//! `tablo_ui`; and only `toasty_compat` reaches Toasty's internals.
//!
//! The checks read the sources: every `crate::` path, every `super::` path that leaves its
//! top-level module, and every root re-export they go through. Tests and their fixtures are
//! exempt, since a test may drive any layer.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// The top-level modules of each layer, lowest first. A module names only the modules listed
/// before it, so outside [`BACK_EDGES`] the modules form no cycle and the list reads as the order
/// to learn them in.
const LAYERS: &[(&str, &[&str])] = &[
    (
        "foundations",
        &[
            "dom",
            "naming",
            "topcoat_compat",
            "declaration",
            "error",
            "toasty_compat",
            "lens",
            "db",
            "csrf",
            "outcome",
        ],
    ),
    (
        "the declaration model",
        &[
            "policy",
            "tenancy",
            "navigation",
            "display",
            "values",
            "rich_text",
            "schema",
            "column",
            "detail",
            "table",
            "form",
            "extend",
        ],
    ),
    ("resources", &["resource", "page"]),
    ("serving", &["upload", "notification", "auth", "panel"]),
];

/// The edges that reach a module listed later in the same layer, each a cycle, with the files that
/// take it and the reason it stands. The check fails on any other file, and on a listed file that
/// no longer takes its edge.
const BACK_EDGES: &[(&str, &[&str], &str)] = &[
    (
        "auth",
        &[
            "notification.rs",
            "notification/bell.rs",
            "notification/stored.rs",
        ],
        "the live toaster and the bell restate the auth gate, the bell reads its signed-in user, \
         and `auth` flashes notifications, registers stored ones and wakes the bells it revokes",
    ),
    (
        "panel",
        &[
            "auth.rs",
            "auth/email_change.rs",
            "auth/gate.rs",
            "auth/login.rs",
            "auth/mail.rs",
            "auth/mount.rs",
            "auth/page.rs",
            "auth/password_reset.rs",
            "auth/profile.rs",
            "auth/registration.rs",
            "auth/session.rs",
            "auth/tenant_profile.rs",
            "auth/tenant_registration.rs",
            "auth/token.rs",
            "auth/two_factor.rs",
            "auth/two_factor/challenge.rs",
            "auth/two_factor/email.rs",
            "auth/two_factor/settings.rs",
            "auth/verification.rs",
            "notification/bell.rs",
            "notification/stored.rs",
        ],
        "the auth pages render in the panel's shell, parse its form bodies and read the request's \
         panel, whose state holds the auth; the bell and stored notifications read the request's \
         panel too, and the shell renders the bell",
    ),
];

#[test]
fn every_module_reaches_only_the_modules_listed_before_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let lib = strip(&fs::read_to_string(src.join("lib.rs")).expect("read lib.rs"));
    for module in declared_modules(&lib) {
        assert!(
            rank(&module).is_some(),
            "`{module}` is in no layer: add it to `LAYERS`"
        );
    }
    let exports = root_exports(&lib);
    let mut reached_up = Vec::new();
    let mut back_edges = Vec::new();
    for file in sources(&src) {
        let relative = file.strip_prefix(&src).expect("under src");
        let segments = module_path(relative);
        let from = &segments[0];
        let (from_layer, from_rank) = rank(from).expect("every module is in a layer");
        let path = slashed(relative);
        let text = strip(&fs::read_to_string(&file).expect("read a source"));
        for target in reached_modules(&text, &segments) {
            let target = exports.get(&target).cloned().unwrap_or(target);
            let Some((to_layer, to_rank)) = rank(&target) else {
                continue;
            };
            if to_layer > from_layer {
                reached_up.push(format!(
                    "{} reaches `{target}` in {}",
                    relative.display(),
                    LAYERS[to_layer].0
                ));
            } else if to_layer == from_layer && to_rank > from_rank {
                if BACK_EDGES
                    .iter()
                    .any(|(to, files, _)| *to == target && files.contains(&path.as_str()))
                {
                    back_edges.push((path.clone(), target));
                    continue;
                }
                reached_up.push(format!(
                    "{} reaches `{target}`, listed after `{from}`",
                    relative.display()
                ));
            }
        }
    }
    reached_up.sort();
    reached_up.dedup();
    assert!(
        reached_up.is_empty(),
        "a module reaches one listed after it in `LAYERS`; move what it needs down, or the caller \
         up:\n{}",
        reached_up.join("\n")
    );
    let stale: Vec<_> = BACK_EDGES
        .iter()
        .flat_map(|(to, files, _)| files.iter().map(move |file| (*file, *to)))
        .filter(|(file, to)| !back_edges.iter().any(|(f, t)| f == file && t == to))
        .collect();
    assert!(
        stale.is_empty(),
        "these `BACK_EDGES` are no longer taken; remove them: {stale:?}"
    );
}

/// The files below the serving layer that render markup: a column's cell, a filter's control, and
/// a form's controls and layouts. A page, a dialog or a table's chrome belongs to the serving
/// layer.
const RENDER_HOOKS: &[&str] = &[
    "column/boolean.rs",
    "column/text.rs",
    "schema/embedded.rs",
    "schema/fields.rs",
    "schema/fields/choice.rs",
    "schema/fields/choice/multiple.rs",
    "schema/fields/custom.rs",
    "schema/fields/file.rs",
    "schema/fields/read_only.rs",
    "schema/fields/rich_text.rs",
    "schema/fields/text.rs",
    "schema/layouts.rs",
    "schema/repeater.rs",
    "table/filter/date.rs",
    "table/filter/input.rs",
];

#[test]
fn only_serving_and_the_render_hooks_name_tablo_ui() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let serving = LAYERS.len() - 1;
    let mut named = Vec::new();
    let mut hooks_naming = Vec::new();
    for file in sources(&src) {
        let relative = file.strip_prefix(&src).expect("under src");
        let segments = module_path(relative);
        if rank(&segments[0]).map(|(layer, _)| layer) == Some(serving) {
            continue;
        }
        let text = strip(&fs::read_to_string(&file).expect("read a source"));
        if !names(&text, "tablo_ui") {
            continue;
        }
        let path = slashed(relative);
        if RENDER_HOOKS.contains(&path.as_str()) {
            hooks_naming.push(path);
        } else {
            named.push(path);
        }
    }
    named.sort();
    assert!(
        named.is_empty(),
        "below the serving layer, only a render hook names `tablo_ui`; move these to `panel`, or \
         list a render hook in `RENDER_HOOKS`:\n{}",
        named.join("\n")
    );
    let stale: Vec<_> = RENDER_HOOKS
        .iter()
        .filter(|hook| !hooks_naming.iter().any(|path| path == *hook))
        .collect();
    assert!(
        stale.is_empty(),
        "these `RENDER_HOOKS` name no `tablo_ui`; remove them: {stale:?}"
    );
}

/// Whether `text` names the path that starts with `word`.
fn names(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// What names Toasty's internals: its internal crate, and the reads of a model's `app`, `mapping`
/// or `db` schema (`Model::schema()` and `Db::schema()` walked through `as_root` and
/// `app_unwrap`). `toasty::schema::{app, mapping, db}` re-export the same structures, so they
/// count too.
const TOASTY_INTERNALS: &[&str] = &[
    "toasty_core",
    "codegen_support::core",
    ".as_root",
    ".app_unwrap()",
    ".schema()",
];

/// The halves of a model's schema `toasty::schema` re-exports from Toasty's internal crate.
const SCHEMA_HALVES: &[&str] = &["app", "mapping", "db"];

#[test]
fn only_toasty_compat_reaches_toasty_internals() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = sources(&src);
    files.push(src.join("lib.rs"));
    let mut reached = Vec::new();
    for file in files {
        let relative = file.strip_prefix(&src).expect("under src");
        if module_path(relative)[0] == "toasty_compat" {
            continue;
        }
        let text = strip(&fs::read_to_string(&file).expect("read a source"));
        for internal in TOASTY_INTERNALS {
            let named = text.match_indices(internal).any(|(at, _)| {
                !text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
            });
            if named {
                reached.push(format!("{} names `{internal}`", relative.display()));
            }
        }
        for half in schema_halves(&text) {
            reached.push(format!(
                "{} names `toasty::schema::{half}`",
                relative.display()
            ));
        }
    }
    reached.sort();
    reached.dedup();
    assert!(
        reached.is_empty(),
        "only `toasty_compat` reaches Toasty's internals; add what these need there:\n{}",
        reached.join("\n")
    );
}

/// The schema halves `text` names through `toasty::schema::`, or a `toasty::{schema::..}` group.
fn schema_halves(text: &str) -> Vec<String> {
    let mut named = Vec::new();
    for (at, _) in text.match_indices("toasty::schema::") {
        named.extend(first_segments(&text[at + "toasty::schema::".len()..]));
    }
    for (at, _) in text.match_indices("toasty::{") {
        let group = &text[at + "toasty::{".len()..];
        for item in top_level_items(&group[..group_end(group)]) {
            if let Some(rest) = item.strip_prefix("schema::") {
                named.extend(first_segments(rest));
            }
        }
    }
    named.retain(|segment| SCHEMA_HALVES.contains(&segment.as_str()));
    named
}

/// The layer `module` is in, and its place in that layer.
fn rank(module: &str) -> Option<(usize, usize)> {
    LAYERS.iter().enumerate().find_map(|(layer, (_, modules))| {
        modules
            .iter()
            .position(|listed| *listed == module)
            .map(|place| (layer, place))
    })
}

/// The source files the check reads: everything under `src` but the crate root, tests and test
/// fixtures.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let root = dir.ends_with("src");
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("read a source directory") {
        let path = entry.expect("a directory entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if name != "tests" {
                files.extend(sources(&path));
            }
        } else if name.ends_with(".rs")
            && !matches!(name, "tests.rs" | "test_support.rs")
            && !(root && name == "lib.rs")
        {
            files.push(path);
        }
    }
    files
}

/// `relative` with `/` between its segments, as [`BACK_EDGES`] and [`RENDER_HOOKS`] spell it.
fn slashed(relative: &Path) -> String {
    relative
        .iter()
        .map(|segment| segment.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// `panel/forms/submit.rs` → `[panel, forms, submit]`; a `mod.rs` names its directory.
fn module_path(relative: &Path) -> Vec<String> {
    let mut segments: Vec<String> = relative
        .iter()
        .map(|segment| {
            segment
                .to_string_lossy()
                .trim_end_matches(".rs")
                .to_string()
        })
        .collect();
    if segments.last().is_some_and(|last| last == "mod") {
        segments.pop();
    }
    segments
}

/// The top-level modules `text`, in the module at `segments`, names.
fn reached_modules(text: &str, segments: &[String]) -> Vec<String> {
    let mut reached = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("crate::") {
        let preceded = rest[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':');
        rest = &rest[at + "crate::".len()..];
        if !preceded {
            reached.extend(first_segments(rest));
        }
    }
    // `super::` that climbs out of the top-level module lands at the crate root.
    let mut rest = text;
    while let Some(at) = rest.find("super::") {
        let preceded = rest[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':');
        rest = &rest[at..];
        let mut climbs = 0;
        while let Some(after) = rest.strip_prefix("super::") {
            climbs += 1;
            rest = after;
        }
        if !preceded && climbs >= segments.len() {
            reached.extend(first_segments(rest));
        }
    }
    reached
}

/// The first segment of the path at the start of `rest`, or of every path in a `{..}` group.
fn first_segments(rest: &str) -> Vec<String> {
    match rest.strip_prefix('{') {
        Some(group) => top_level_items(&group[..group_end(group)])
            .iter()
            .filter_map(|item| identifier(item))
            .collect(),
        None => identifier(rest).into_iter().collect(),
    }
}

fn identifier(text: &str) -> Option<String> {
    let text = text.trim_start();
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    (end > 0).then(|| text[..end].to_string())
}

/// The index of the `}` that closes a group whose `{` precedes `group`.
fn group_end(group: &str) -> usize {
    let mut depth = 1;
    for (index, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    group.len()
}

/// A `{..}` group's comma-separated items, nested groups kept whole.
fn top_level_items(group: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (index, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                items.push(group[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    items.push(group[start..].trim());
    items.retain(|item| !item.is_empty());
    items
}

/// The top-level modules the crate root declares, test-only ones aside.
fn declared_modules(lib: &str) -> Vec<String> {
    let mut modules = Vec::new();
    let mut test_only = false;
    for line in lib.lines().map(str::trim) {
        if line == "#[cfg(test)]" {
            test_only = true;
            continue;
        }
        let declared = line
            .strip_prefix("pub mod ")
            .or_else(|| line.strip_prefix("mod "))
            .and_then(|rest| rest.strip_suffix(';'));
        if let Some(module) = declared
            && !test_only
        {
            modules.push(module.to_string());
        }
        test_only = false;
    }
    modules
}

/// Each name the crate root re-exports from one of its modules, mapped to that module. A derive
/// shares its trait's name, so a name `tablo_macros` re-exports too keeps the module's entry.
fn root_exports(lib: &str) -> HashMap<String, String> {
    let mut exports = HashMap::new();
    let mut rest = lib;
    while let Some(at) = rest.find("\npub use ") {
        rest = &rest[at + "\npub use ".len()..];
        let statement = &rest[..rest.find(';').unwrap_or(rest.len())];
        let Some(module) = identifier(statement).filter(|module| rank(module).is_some()) else {
            continue;
        };
        for leaf in leaves(statement) {
            exports.insert(leaf, module.clone());
        }
    }
    exports
}

#[test]
fn a_derive_of_the_same_name_keeps_the_trait_s_module() {
    let lib = "\npub use form::RecordForm;\npub use tablo_macros::RecordForm;\n";
    assert_eq!(root_exports(lib)["RecordForm"], "form");
}

/// The last segment of every path a `use` tree names.
fn leaves(tree: &str) -> Vec<String> {
    let mut leaves = Vec::new();
    let mut word = String::new();
    let chars: Vec<char> = tree.chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        if !word.is_empty() && !(c == ':' && chars.get(index + 1) == Some(&':')) {
            leaves.push(std::mem::take(&mut word));
        }
        word.clear();
    }
    if !word.is_empty() {
        leaves.push(word);
    }
    leaves
}

/// `source` without its comments and the contents of its string and character literals.
fn strip(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for c in chars.by_ref() {
                    if previous == '*' && c == '/' {
                        break;
                    }
                    previous = c;
                }
            }
            '\'' => {
                // A character literal is skipped; a lifetime's quote is kept.
                let mut ahead = chars.clone();
                match (ahead.next(), ahead.next()) {
                    (Some('\\'), _) => {
                        for c in chars.by_ref() {
                            if c == '\'' {
                                break;
                            }
                        }
                    }
                    (Some(_), Some('\'')) => {
                        chars.next();
                        chars.next();
                    }
                    _ => out.push(c),
                }
            }
            '"' => {
                out.push('"');
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}
