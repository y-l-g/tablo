# `tablo-core` is layered, and a test enforces it

Date: 2026-10-05 — Status: accepted

## Decision

The crate's top-level modules form four layers: foundations, the declaration model (`table`,
`schema`, `form`, `policy`, `tenancy`, `navigation`), resources, and serving (`panel`, `auth`,
`page`, `upload`, `notification`). A module names its own layer and the ones below it.
`tests/layers.rs` reads every `crate::` path, every `super::` path that leaves its module, and
every root re-export they go through, and fails on one that reaches up; a new top-level module
must join a layer.

A lower layer that needs request state the serving layer resolves asks through a function the
serving layer installs in the app context (`MountScope`, `TenantSource`) rather than naming the
serving layer. The serving layer renders what reaches serving routes, such as a list page.

## Rejected

- A separate crate for the declaration model: the compiler would enforce the boundary, but every
  item the layers share would become `#[doc(hidden)] pub`, and the derives' paths would move.
