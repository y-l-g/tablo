# Shell parity with shadcn sidebar

Date: 2026-08-28 — Status: accepted

## Decision

The shell follows the shadcn `sidebar.tsx` pattern through the upstream `sidebar` primitive. It
uses the shadcn CSS variables (`--sidebar-width:16rem`, `--sidebar-width-icon:3rem`,
`--sidebar-width-mobile:18rem`), a fixed container with scrolling `SidebarContent`, a `shrink-0`
sticky header, and `data-state=expanded|collapsed`; below `md` it renders its own sheet drawer.

Open state is runtime state. `Panel::render_shell` wraps the shell in a hoisting body and creates
`Signal<bool>`s for desktop (`open`, seeded from the `sidebar_state` cookie) and mobile
(`mobile_open`); the trigger pair carries `@click` handlers, and desktop and mobile navigation
render as one tree. `assets/sidebar.js` persists `data-state` to the cookie (604800s) and binds
`Ctrl+B`; `theme.js` toggles `document.documentElement.classList` (`dark`) with `localStorage`
(fallback cookie), and the blocking `theme_init_script` applies the class before first paint.
Both scripts ship with `asset!` plus `topcoat::runtime::script()`; ADR-0014 owns the asset list.
