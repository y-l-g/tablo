# 0031 Rich text is cleaned HTML, and its toolbar is its schema

A rich text field binds a `String` column and stores HTML: `Field::rich_text(lens!(Post.body))`,
`.rich_text()` on a record form's text control, or `Field::rich_text_input("note")` for a form
input. Its toolbar is a list of groups of `Tool`, a closed enum, and each tool names the elements
and attributes it produces. The toolbar decides both what the editor loads and what the server
keeps: a tag no enabled tool produces cannot be typed, is dropped from a paste, and is removed from
a forged post.

The server cleans a rich text value as it reads the submission, with `ammonia` over the toolbar's
allowlist, so the parse, the rules, the write and a refused form's re-render all see the cleaned
value. A document holding no text reads as empty, so the field's blank answer applies. Length
rules count the characters a reader sees, not the markup. No allowlist keeps a `style` attribute,
an event handler or a URL scheme other than `http`, `https` and `mailto`, and every link gets
`rel="noopener noreferrer nofollow"`.

`RichTextColumn` cleans again as it renders, with every tool's allowlist, since a row can be
written by a seed, a migration or another app. A table cell shows the text, cut to a length; a
detail page shows the HTML. Both the column and the editor style the content through one set of
Tailwind classes in `tablo-core`, which the app's stylesheet build scans.

The control renders a `<textarea>` holding the HTML, so the form posts and re-renders without
JavaScript. `rich_text.js`, a shell script like `selects.js` and `repeaters.js`, mounts the
editor over each textarea and copies the document back into it on every change; the editor runs
only where a control asks for it, so a page without one loads no editor code. The editor is
TipTap, vendored as one ES module under `crates/tablo-ui/assets/tiptap/`, built from the pinned
`package-lock.json` beside it by `cargo xtask vendor-tiptap`. The bundle names the lockfile's
hash, and a guard fails when the two drift. An app's build runs no Node.

[ADR-0026](0026-topcoat-runtime-only.md) keeps tables and dialogs on Topcoat's runtime. A rich
text control is a form control the runtime has no word for, as a filterable select is, so its
script is a control script, not a table or dialog script.

## Rejected

- Storing TipTap's JSON: the server would need a second document schema, kept in step with the
  editor's, to validate, clean and render it, and every other reader of the column would too.
- Markdown: rendering it needs a parser on every reader, and an editor that shows formatting
  round-trips it lossily.
- Cleaning only when rendering, as Filament does by default: every reader outside the panel gets
  the HTML the client posted.
- Allowing `style`: it carries CSS that positions content over the page or loads URLs.
- A fixed allowlist apart from the toolbar: a forged post could store markup no editor on the
  form can produce or show.
- `contenteditable` with `execCommand`: the command is deprecated, and its markup differs by
  browser.
- Trix: its own HTML dialect, with no way to narrow it per field.
- Loading the editor on every page: the bundle outweighs every other script the shell loads.
- Bundling at the app's build: every app's build would need Node and the npm registry.
- A CDN: a third-party origin at runtime, which an offline or locked-down panel cannot reach.
