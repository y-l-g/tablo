//! Reading a posted body into the values a schema's fields hold.
//!
//! A body posts flat `(key, value)` pairs. Its schema reads each field's key in the shape the
//! field's control posts: a scalar control's last value, a multiple choice's every value, and a
//! repeater's rows. Every later step, from completing an edit to parsing, then reads one value per
//! key.

use std::collections::{BTreeSet, HashMap, HashSet};

use topcoat::Result;

use super::{Schema, fields::ChoiceControl};
use crate::values::{FormValue, FormValues};

/// A posted body's pairs, indexed by key, recording which ones a field claimed.
pub(super) struct Pairs<'p> {
    pairs: &'p [(String, String)],
    at: HashMap<&'p str, Vec<usize>>,
    claimed: Vec<bool>,
}

impl<'p> Pairs<'p> {
    fn new(pairs: &'p [(String, String)]) -> Self {
        let mut at: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, (key, _)) in pairs.iter().enumerate() {
            at.entry(key.as_str()).or_default().push(index);
        }
        Self {
            pairs,
            at,
            claimed: vec![false; pairs.len()],
        }
    }

    /// Claims every value posted under `key`, in the order the body posts them.
    pub(super) fn claim(&mut self, key: &str) -> Vec<&'p str> {
        let Some(indices) = self.at.get(key) else {
            return Vec::new();
        };
        indices
            .iter()
            .map(|&index| {
                self.claimed[index] = true;
                self.pairs[index].1.as_str()
            })
            .collect()
    }

    /// The keys no field claimed, sorted, each once.
    fn unclaimed(&self) -> Vec<String> {
        self.pairs
            .iter()
            .zip(&self.claimed)
            .filter(|(_, claimed)| !**claimed)
            .map(|((key, _), _)| key.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl<F> Schema<F> {
    /// Reads a posted body's `(key, value)` pairs, in the order it posts them, into the values
    /// the schema's fields hold: a scalar control's last value, a multiple choice's values without
    /// blanks or repeats, and a repeater's rows in the order its key lists them. A field that
    /// posts nothing holds nothing, so an edit keeps its stored value. A key no field declares is
    /// left out: a page reads its own, such as the CSRF token, from the pairs.
    ///
    /// A resource's form and an action's input read their submissions; a page handling its own
    /// post reads it before parsing it.
    ///
    /// # Errors
    ///
    /// A repeater's key lists something other than its rows, a row twice, a row posting nothing,
    /// or more than [`MAX_ROWS`](super::MAX_ROWS) rows: no browser posts these, so a handler
    /// answers 400.
    pub fn read(&self, posted: &[(String, String)]) -> std::result::Result<FormValues, String> {
        self.read_claiming(&mut Pairs::new(posted))
    }

    /// [`read`](Self::read), with the keys no field claims, sorted and each once.
    pub(crate) fn read_unclaimed(
        &self,
        posted: &[(String, String)],
    ) -> std::result::Result<(FormValues, Vec<String>), String> {
        let mut pairs = Pairs::new(posted);
        let values = self.read_claiming(&mut pairs)?;
        Ok((values, pairs.unclaimed()))
    }

    fn read_claiming(&self, pairs: &mut Pairs<'_>) -> std::result::Result<FormValues, String> {
        let mut values = FormValues::new();
        for field in &self.fields {
            let name = field.name();
            if let Some(repeater) = field.as_repeater() {
                if let Some(rows) = repeater.read(name, pairs)? {
                    values.insert(name, rows);
                }
                continue;
            }
            let posted = pairs.claim(name);
            if field.as_choice().is_some_and(ChoiceControl::is_multiple) {
                if !posted.is_empty() {
                    values.insert(name, distinct(&posted));
                }
            } else if let Some(last) = posted.last() {
                values.insert(name, *last);
            }
        }
        Ok(values)
    }

    /// A posted input as its parse reads it: [read](Self::read), with a condition-hidden field's
    /// key removed, since the browser posts nothing for it.
    ///
    /// # Errors
    ///
    /// 400 for a malformed repeater and for a key that neither a field declares nor `reserved`
    /// names: the keys the POST carries for itself.
    pub(crate) fn read_input(
        &self,
        posted: &[(String, String)],
        reserved: &[&str],
    ) -> Result<FormValues> {
        use topcoat::router::error::bad_request;

        let (mut input, unclaimed) = self.read_unclaimed(posted).map_err(bad_request)?;
        let unknown: Vec<String> = unclaimed
            .into_iter()
            .filter(|key| !reserved.contains(&key.as_str()))
            .collect();
        if !unknown.is_empty() {
            return Err(bad_request(format!("unknown field(s): {}", unknown.join(", "))).into());
        }
        // A disabled field's default first, which a condition reads as the browser did.
        self.drop_disabled(&mut input, false);
        for key in self.condition_hidden(&input) {
            input.remove(&key);
        }
        Ok(input)
    }
}

/// A multiple choice's posted values, trimmed, each once and none blank: its hidden empty value,
/// which posts the key when no box is checked, drops here.
fn distinct(posted: &[&str]) -> FormValue {
    let mut seen = HashSet::new();
    FormValue::List(
        posted
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && seen.insert(*value))
            .map(str::to_string)
            .collect(),
    )
}

#[cfg(test)]
mod tests;
