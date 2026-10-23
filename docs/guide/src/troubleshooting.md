# Troubleshooting

A mistake in a declaration refuses the mount with a message naming it; [Mount errors](./mount-errors.md)
lists each one. This chapter covers what goes wrong once the panel mounts.

## Startup

| Symptom | Cause | Fix |
| --- | --- | --- |
| `cargo run` stops with an error about a missing asset bundle | `AssetBundle::load()` reads `assets/` beside the executable, which `cargo build` does not write | run the app with `topcoat dev`, or `topcoat asset bundle` first; see [Deployment](./deployment.md) |
| the build script fails fetching Tailwind | `tablo_build::tailwind()` downloads the Tailwind CLI on its first run | build once with network access; the CLI stays in Topcoat's cache |

## Pages

| Symptom | Cause | Fix |
| --- | --- | --- |
| pages render unstyled, and delete confirmations do not open | the router has no asset bundle, or the panel no `shell_assets` | install both: [Assets](./panel-and-routing.md#assets) |
| a Tailwind class in your own page has no style | the stylesheet scans only the sources `styles.css` names | add an `@source` line for them |
| a page renders a document inside a document | the app declares its own `#[layout]` at the panel prefix | drop it, and frame pages with `Panel::layout` |
| a resource is missing from the sidebar | its policy does not allow `ViewAny` for this user | allow `ViewAny`; see [Policy](./policy-auth-tenancy.md#policy) |
| no Create button, or the create page answers 403 | the policy does not allow `Create`, or the resource's `Form` is `NoForm` | allow `Create` and name a record form |
| a record's detail page answers 404 | the resource's detail lists no column: `Detail::empty()`, or a hand-written `ModelColumns` impl | declare one with `ResourceDef::detail`, or derive `ModelColumns` |
| `` `Post` declares no default columns `` | the resource's model does not implement `ModelColumns` | add `#[derive(tablo::ModelColumns)]` to the model |
| a tenant-owned resource answers 403 everywhere | the request has no tenant: the user has no membership, and the shipped `AdminUser` has none | sign in [your own user table](./policy-auth-tenancy.md#tenancy), whose `PanelUser::tenants` lists memberships |
| the export answers 413 | the search and filters match more rows than the [export](https://y-l.fr/tablo/nightly/api/tablo/struct.Table.html#export) writes | narrow the list before exporting |

## Signing in and forms

| Symptom | Cause | Fix |
| --- | --- | --- |
| every form POST answers 403 off `localhost` | the session and CSRF cookies are `Secure`, so the browser drops them over plain HTTP | serve the app over HTTPS |
| a user signs in and gets 403 | the user has no panel access: `AdminUser.active` is `false`, or your `PanelUser` denies it | activate the user |
| sign-ins answer the wrong-password message for a correct password | the throttle allows five attempts a minute per login and address, and fifty an hour per login; behind a proxy every client shares the proxy's address | wait out the window; install `TrustedProxies` on the router |
| a form POST answers 400 | it carries a key the form does not declare: a custom control posting an extra input, or a hand-built request | post only the record form's keys |
| a route you added under the prefix redirects to the login page or answers 401 | the auth gate covers every path under the panel prefix | sign in, or move a public route outside the prefix |
