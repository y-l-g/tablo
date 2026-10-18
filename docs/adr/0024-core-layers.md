# 0024 `tablo-core` is layered, and a test enforces it

The crate's top-level modules form four layers: foundations, the declaration model, resources
(and pages), and serving. A module names its own layer and the ones below; `tests/layers.rs`
lists the layers and fails on any path that reaches up. A lower layer needing request state asks
through a function the serving layer installs in the app context (`MountScope`, `TenantSource`).

Within a layer the modules are ordered too: `LAYERS` lists each layer's modules lowest first, and a
module names only the modules listed before it, so the list reads as the order to learn them in. A
concept two modules share lives in a module below both, and the public module a user names
re-exports it: `column`, what a table lists and a detail page shows, under `table`; `values`, the
form values, scalars and field errors a schema reads and renders and a record form hydrates and
parses, under `form`; `display`, the colors and number spellings a cell and an option share, under
`table`.

Two cycles remain, both in the serving layer: `notification` and `auth`, and `auth` and `panel`.
`BACK_EDGES` lists each with its reason and the files that take it, so the modules form no other
cycle; the check fails on a file outside the list, and on a listed file that no longer takes its
edge.

A module's layer follows what it does, not what it is about. The panel's URL scheme, the list
page's chrome and dialogs, and a table's request wiring serve requests, so they live under `panel`,
though they concern tables. Below the serving layer only a render hook names `tablo_ui`: a column's
cell, a filter's control, a form's controls and layouts. The test lists those files, so a
`tablo_ui` component that creeps into the declaration model fails it; plain `view!` markup, such as
a condition's wrapper, is not checked.

## Rejected

- A separate crate for the declaration model: every shared item would become
  `#[doc(hidden)] pub`, and the derives' paths would move.
- Ordering layers but not the modules inside them: `table`, `schema`, `form` and `detail` then
  named each other freely, and a change to one rippled through the other three.
- Layering by module name alone: a module named after a declaration, such as `table`, could then
  hold the routes and page UI that serve it, and the check would still pass.
