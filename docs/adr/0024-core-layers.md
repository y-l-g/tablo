# 0024 `tablo-core` is layered, and a test enforces it

The crate's top-level modules form four layers: foundations, the declaration model, resources,
and serving. A module names its own layer and the ones below; `tests/layers.rs` lists the layers
and fails on any path that reaches up. A lower layer needing request state asks through a
function the serving layer installs in the app context (`MountScope`, `TenantSource`).

A module's layer follows what it does, not what it is about. The panel's URL scheme, the list
page's chrome and dialogs, and a table's request wiring serve requests, so they live under `panel`,
though they concern tables. Below the serving layer only a render hook names `tablo_ui`: a column's
cell, a filter's control, a form's controls and layouts. The test lists those files, so markup that
creeps into the declaration model fails it.

## Rejected

- A separate crate for the declaration model: every shared item would become
  `#[doc(hidden)] pub`, and the derives' paths would move.
- Layering by module name alone: a module named after a declaration, such as `table`, could then
  hold the routes and page UI that serve it, and the check would still pass.
