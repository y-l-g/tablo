# 0024 `tablo-core` is layered, and a test enforces it

The crate's top-level modules form four layers: foundations, the declaration model, resources
(and pages), and serving. A module names its own layer and the ones below; `tests/layers.rs`
lists the layers and fails on any path that reaches up. A lower layer needing request state asks
through a function the serving layer installs in the app context (`MountScope`, `TenantSource`).

Within a layer the modules are ordered too: `LAYERS` lists each layer's modules lowest first, and a
module names only the modules listed before it, so the list reads as the order to learn them in. A
concept two modules share lives in a module below both: `column`, what a table lists and a
detail page shows; `values`, the form values, scalars and field errors a schema reads and renders
and a record form hydrates and parses; `display`, the colors and number spellings a cell and an
option share. A user never names a module by its layer: the crate root re-exports every public
type, from whichever module defines it, so a type moves between layers without changing its path.

The modules form no cycle. What a module needs of one listed after it moves down, as the flash
cookie and the bell's wake bus did, or the later module installs it behind a trait the earlier one
owns: the serving layer orders `auth`, `notification`, then `panel`, and `auth` reads the request's
panel as its own `Realm` and renders through the `Host` trait the panel implements, so it names no
panel type.

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
- Cycles the test allows file by file: `auth` named the panel in 19 files, so a change to the
  panel's state rippled through auth, and what auth needs of the panel had no single place to
  read it.
- Layering by module name alone: a module named after a declaration, such as `table`, could then
  hold the routes and page UI that serve it, and the check would still pass.
