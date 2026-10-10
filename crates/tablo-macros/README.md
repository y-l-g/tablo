# tablo-macros

The `RecordForm`, `ModelForm`, `EmbeddedForm`, `ActionInput`, `RepeaterItem` and `Options` derives
for Tablo's forms, embedded values, action inputs, repeaters and choice fields.

`tablo-core` re-exports each derive and `tablo` re-exports `tablo-core`, so an app depends on
`tablo` and brings them in with `use tablo::prelude::*;`. The
[API reference](https://docs.rs/tablo-core) documents them where they are re-exported.
