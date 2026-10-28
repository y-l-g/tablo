//! A table's browser state: the signals its links, bulk selection and dialogs write.

use std::collections::HashMap;

use topcoat::{
    context::Cx,
    runtime::{Signal, signal},
};

use crate::{
    panel::input::{Selection, TableTarget},
    table::{TableState, request_query},
};

/// A table's browser state: the list's query string, the bulk selection, and
/// the write its confirmation dialog asks about.
///
/// The page reads `query` on the server, so writing it in the browser reruns
/// the page with the new list state.
#[derive(Clone)]
pub(crate) struct TableSignals {
    /// The list's URL query, without the leading `?`.
    pub(crate) query: Signal<String>,
    /// The bulk selection as `,token,` entries, each token's key percent-encoded ([`bulk_token`]
    /// spells one); empty selects none.
    pub(crate) bulk: Signal<String>,
    /// Whether the selection takes every record `query` selects rather than `bulk`'s. A change
    /// of the selection or of `query` clears it.
    pub(crate) all: Signal<bool>,
    /// The collapsed groups as `,token,` entries, each a group label as [`bulk_token`] spells
    /// it; empty collapses none.
    pub(crate) collapsed: Signal<String>,
    pub(crate) confirm: ConfirmSignals,
    /// Where each custom action asking for input posts its dialog, by the action's name.
    pub(crate) inputs: HashMap<&'static str, TableTarget>,
    /// The page's path and the table's prefix, which key the signals.
    key: (String, String),
}

/// The write the confirmation dialog asks about.
#[derive(Clone)]
pub(crate) struct ConfirmSignals {
    /// The write's POST target; empty while the dialog is closed.
    pub(crate) action: Signal<String>,
    pub(crate) title: Signal<String>,
    /// The confirming button's label.
    pub(crate) label: Signal<String>,
    /// Whether the write takes the bulk selection.
    pub(crate) bulk: Signal<bool>,
}

impl TableSignals {
    /// The signals of the table whose parameters carry `prefix`, `query`
    /// seeded with the request's query as written.
    ///
    /// Keyed by the page's path too: runtime navigation keeps the values of
    /// signals two pages share, so another page's table starts from its own
    /// URL. Creates the signals, so it carries [`topcoat::runtime::signal`]'s
    /// contract: call it while a page body runs.
    pub(crate) fn new(cx: &Cx, prefix: Option<&str>) -> Self {
        let path = topcoat::context::try_context::<http::request::Parts>(cx)
            .map(|parts| parts.uri.path().to_string())
            .unwrap_or_default();
        let key = (path, prefix.unwrap_or_default().to_string());
        let cx = cx.keyed(("tablo-table", key.0.as_str(), key.1.as_str()));
        let query = request_query(&cx);
        Self {
            query: signal(&cx, move || query),
            bulk: signal(&cx, String::new),
            all: signal(&cx, || false),
            collapsed: signal(&cx, String::new),
            confirm: ConfirmSignals {
                action: signal(&cx, String::new),
                title: signal(&cx, String::new),
                label: signal(&cx, String::new),
                bulk: signal(&cx, || false),
            },
            inputs: HashMap::new(),
            key,
        }
    }

    /// These signals, with the target of the input dialog of each action in `names`.
    ///
    /// Creates the signals, so it carries [`TableSignals::new`]'s contract.
    pub(crate) fn with_inputs(
        mut self,
        cx: &Cx,
        names: impl Iterator<Item = &'static str>,
    ) -> Self {
        for name in names {
            let cx = cx.keyed((
                "tablo-table-input",
                self.key.0.as_str(),
                self.key.1.as_str(),
                name,
            ));
            let target = TableTarget {
                action: signal(&cx, String::new),
                bulk: signal(&cx, || false),
            };
            self.inputs.insert(name, target);
        }
        self
    }

    /// The selection the table's write forms post.
    pub(crate) fn selection(&self) -> Selection {
        Selection {
            keys: self.bulk.clone(),
            all: self.all.clone(),
            query: self.query.clone(),
        }
    }

    /// The list state `query` spells, read so a change reruns the page.
    pub(crate) fn state(&self, prefix: Option<&str>) -> TableState {
        let query = self.query.get();
        match prefix {
            Some(prefix) => TableState::from_query_prefixed(&query, prefix),
            None => TableState::from_query(&query),
        }
    }
}
