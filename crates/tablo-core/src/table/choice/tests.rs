use std::sync::Arc;

use proptest::{collection, prelude::*};
use topcoat::context::Cx;

use super::ColumnChoice;
use crate::table::{BoxColumn, Column, ColumnToggle};

/// A column that declares only its name and toggle.
struct Named(String, ColumnToggle);

impl Column<()> for Named {
    fn name(&self) -> &str {
        &self.0
    }

    fn label(&self) -> crate::Text {
        (&self.0).into()
    }

    fn text(&self, _cx: &Cx, _row: &()) -> String {
        String::new()
    }

    fn toggle(&self) -> ColumnToggle {
        self.1
    }
}

fn toggle() -> impl Strategy<Value = ColumnToggle> {
    prop_oneof![
        Just(ColumnToggle::Fixed),
        Just(ColumnToggle::ShownByDefault),
        Just(ColumnToggle::HiddenByDefault),
    ]
}

proptest! {
    /// Whatever boxes the picker posts, the stored choice shows every fixed column and exactly the
    /// checked toggleable ones; a posted name no column declares changes nothing.
    #[test]
    fn a_stored_choice_shows_what_was_checked(
        columns in collection::btree_map("[a-z_]{1,8}", toggle(), 0..12),
        checked in collection::btree_set("[a-z_]{1,8}", 0..12),
    ) {
        let declared: Vec<BoxColumn<()>> = columns
            .iter()
            .map(|(name, toggle)| Arc::new(Named(name.clone(), *toggle)) as BoxColumn<()>)
            .collect();
        let posted: Vec<&str> = checked.iter().map(String::as_str).collect();
        let stored = ColumnChoice::checked(&declared, &posted).encode();
        let choice = ColumnChoice::decode(&stored);
        for (name, toggle) in &columns {
            let expected = *toggle == ColumnToggle::Fixed || checked.contains(name);
            prop_assert_eq!(choice.shows(name, *toggle), expected, "{}", name);
        }
    }

    /// A choice within the caps survives its stored spelling, whatever its names hold.
    #[test]
    fn a_choice_survives_its_stored_spelling(
        hidden in collection::btree_set("\\PC{1,40}", 0..30),
        shown in collection::btree_set("\\PC{1,40}", 0..30),
    ) {
        let choice = ColumnChoice { hidden, shown };
        prop_assert_eq!(ColumnChoice::decode(&choice.encode()), choice);
    }
}
