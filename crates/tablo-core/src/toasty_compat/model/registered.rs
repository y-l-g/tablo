//! Which models a compiled schema registers: what tells a shipped model the app's `Db` holds
//! from one it left out.

use super::AppSchema;

impl AppSchema {
    /// The compiled schema `ex` runs against: its `Db`'s.
    pub(crate) fn of_executor(ex: &mut dyn toasty::Executor) -> Self {
        Self(ex.schema().clone())
    }

    /// The name of each model the schema gives a table, in upper camel case.
    #[cfg(test)]
    pub(crate) fn table_models(&self) -> Vec<String> {
        self.0
            .app
            .models()
            .filter(|model| model.is_root())
            .map(|model| model.name().upper_camel_case())
            .collect()
    }

    /// Whether the schema registers a model named `name`, in upper camel case.
    pub(crate) fn registers(&self, name: &str) -> bool {
        self.0
            .app
            .models()
            .any(|model| model.name().upper_camel_case() == name)
    }
}
