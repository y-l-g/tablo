# 0024 `tablo-core` is layered, and a test enforces it

The crate's top-level modules form four layers: foundations, the declaration model, resources,
and serving. A module names its own layer and the ones below; `tests/layers.rs` lists the layers
and fails on any path that reaches up. A lower layer needing request state asks through a
function the serving layer installs in the app context (`MountScope`, `TenantSource`).

## Rejected

- A separate crate for the declaration model: every shared item would become
  `#[doc(hidden)] pub`, and the derives' paths would move.
