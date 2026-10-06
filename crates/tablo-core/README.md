# tablo-core

The toolkit behind the [`tablo`](https://crates.io/crates/tablo) facade: `Panel`, `Resource` and
`ResourceDef`, the `Table` and `Schema` builders, `Policy`, auth, tenancy and uploads, plus the
`RecordForm`, `EmbeddedForm` and `Options` derives.

An app depends on `tablo`, which re-exports this crate at its root; a direct dependency skips the
facade's driver features and testing client. The [user guide](https://y-l.fr/tablo/nightly/guide/)
documents each part, and the [API reference](https://docs.rs/tablo-core) covers the rest.
