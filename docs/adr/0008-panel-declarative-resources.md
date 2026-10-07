# 0008 The panel declares resources and owns the shell

`Panel` is mounted on an app-owned router and declares its resources and pages; it registers
their routes, sidebar entries and the shell document, so the app writes no document HTML. One
router mounts several panels at distinct prefixes. Navigation, auth and uploads are per-panel
state carried on each request, never app-context singletons. A declaration owns its label, order
and icon; the panel owns its URL.
