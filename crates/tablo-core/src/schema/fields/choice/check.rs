//! What a choice checks of a submission: each posted key among the options its parent value
//! offers, on submit and again in the write's transaction.

use topcoat::context::Cx;

use super::{ChoiceControl, Scope};
use crate::{
    Text,
    schema::relationship::{MAX_RELATIONSHIP_OPTIONS, OptionLoadError, RelatedCheck},
    text,
    values::FormValue,
};

impl ChoiceControl {
    /// Checks whether a non-empty `value` matches an option the parent value `parent` offers, or
    /// for a multiple choice, whether each value its list holds does, but the stored value
    /// `stored` holds already: keeping a link changes nothing, so it holds past the option cap.
    pub(in crate::schema) async fn validate_exists(
        &self,
        cx: &Cx,
        label: &Text,
        value: &FormValue,
        parent: Option<&str>,
        stored: Option<&FormValue>,
    ) -> Vec<Text> {
        if !self.multiple {
            let Some(value) = value.as_text() else {
                return Vec::new();
            };
            let kept = stored
                .and_then(FormValue::as_text)
                .is_some_and(|held| held.trim() == value.trim());
            if kept {
                return Vec::new();
            }
            return self.validate_one(cx, label, value, parent).await;
        }
        let held = stored.and_then(FormValue::as_list).unwrap_or_default();
        let keys = match chosen(label, value, held) {
            Ok(keys) => keys,
            Err(message) => return vec![message],
        };
        for key in keys.iter().filter(|key| !held.contains(key)) {
            let errors = self.validate_one(cx, label, key, parent).await;
            if !errors.is_empty() {
                return errors;
            }
        }
        Vec::new()
    }

    /// Checks whether a non-empty `value` matches an option the parent value `parent` offers.
    async fn validate_one(
        &self,
        cx: &Cx,
        label: &Text,
        value: &str,
        parent: Option<&str>,
    ) -> Vec<Text> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let Some(relationship) = &self.relationship else {
            if !self.tags
                && !self.options.is_empty()
                && !self.options.iter().any(|(v, _)| v == trimmed)
            {
                return vec![text!("tablo-field-invalid", label = label)];
            }
            return Vec::new();
        };
        let scope = match self.scope(parent) {
            Scope::All => None,
            Scope::Within(scope) => Some(scope),
            Scope::Nothing => return vec![text!("tablo-field-invalid", label = label)],
        };
        let failure = match self.load_options(cx, parent).await {
            Ok(opts) if opts.iter().any(|(v, _)| v == trimmed) => return Vec::new(),
            Ok(_) => "tablo-field-invalid",
            Err(OptionLoadError::Denied) => "tablo-field-unavailable",
            Err(OptionLoadError::Overflow) if self.searchable => {
                let Ok(mut db) = crate::db::db(cx) else {
                    return vec![text!(
                        load_failure(&OptionLoadError::Misdeclared),
                        label = label
                    )];
                };
                match (relationship.check)(cx, trimmed.to_string(), scope, &mut db).await {
                    Ok(RelatedCheck::FoundViewable) => return Vec::new(),
                    Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "tablo-field-invalid",
                    Err(error) => load_failure(&error),
                }
            }
            Err(error) => load_failure(&error),
        };
        vec![text!(failure, label = label)]
    }

    /// Re-checks a submitted relationship key in the write's transaction, among the rows the
    /// parent value `parent` selects, or for a multiple choice, each key its list holds but the
    /// stored list `stored` does not.
    pub(in crate::schema) async fn recheck(
        &self,
        cx: &Cx,
        label: &Text,
        value: &FormValue,
        parent: Option<&str>,
        stored: Option<&FormValue>,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<Text> {
        if !self.multiple {
            let Some(value) = value.as_text() else {
                return Vec::new();
            };
            return self.recheck_one(cx, label, value, parent, ex).await;
        }
        let held = stored.and_then(FormValue::as_list).unwrap_or_default();
        let keys = match chosen(label, value, held) {
            Ok(keys) => keys,
            Err(message) => return vec![message],
        };
        for key in keys.iter().filter(|key| !held.contains(key)) {
            let errors = self.recheck_one(cx, label, key, parent, &mut *ex).await;
            if !errors.is_empty() {
                return errors;
            }
        }
        Vec::new()
    }

    /// Re-checks one submitted relationship key in the write's transaction, among the rows the
    /// parent value `parent` selects.
    async fn recheck_one(
        &self,
        cx: &Cx,
        label: &Text,
        value: &str,
        parent: Option<&str>,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<Text> {
        let trimmed = value.trim();
        let Some(relationship) = &self.relationship else {
            return Vec::new();
        };
        if trimmed.is_empty() {
            return Vec::new();
        }
        let scope = match self.scope(parent) {
            Scope::All => None,
            Scope::Within(scope) => Some(scope),
            Scope::Nothing => return vec![text!("tablo-field-invalid", label = label)],
        };
        let failure = match (relationship.check)(cx, trimmed.to_string(), scope, ex).await {
            Ok(RelatedCheck::FoundViewable) => return Vec::new(),
            Ok(RelatedCheck::FoundHidden | RelatedCheck::NotFound) => "tablo-field-invalid",
            Err(error) => load_failure(&error),
        };
        vec![text!(failure, label = label)]
    }
}

/// The keys a multiple choice's `value` holds, or the error refusing it: one adding more options
/// than a choice loads. A value holding no list chooses none here: the parse refuses it.
///
/// The keys `held` holds already are not choices this submission adds, and are not loaded again
/// either: counting them would leave a record holding more of them than a choice loads unable to
/// save at all.
fn chosen<'v>(
    label: &Text,
    value: &'v FormValue,
    held: &[String],
) -> std::result::Result<&'v [String], Text> {
    let keys = value.as_list().unwrap_or_default();
    let added = keys.iter().filter(|key| !held.contains(key)).count();
    if added > MAX_RELATIONSHIP_OPTIONS {
        return Err(text!(
            "tablo-field-too-many-choices",
            label = label,
            max = MAX_RELATIONSHIP_OPTIONS
        ));
    }
    Ok(keys)
}

/// The message of a failed option load.
fn load_failure(error: &OptionLoadError) -> &'static str {
    match error {
        OptionLoadError::Denied => "tablo-field-unavailable",
        OptionLoadError::LoadFailed | OptionLoadError::Overflow => "tablo-field-options-retry",
        OptionLoadError::Misdeclared => "tablo-field-options-failed",
    }
}
