# Tablo's site

The pages at [y-l.fr/tablo](https://y-l.fr/tablo/): the landing page and the examples gallery.
They are static HTML, CSS and a little JavaScript, with no build step. The docs workflow
(`.github/workflows/docs.yml`) publishes `index.html`, `examples/` and `assets/` at the root, beside
the guide, the API reference and the atlas under `nightly/`.

| Path | Holds |
| --- | --- |
| `index.html` | the landing page |
| `examples/index.html` | the examples gallery |
| `assets/ledger.css` | Ledger, the identity the site, tablo-demo and tablo-saas share |
| `assets/site.css`, `assets/site.js` | what only this site uses: the hero's callouts, the theme switcher, the gallery's filters |
| `assets/captures/` | the UI shots, 1600 pixels wide, as WebP |
| `assets/fonts/` | Schibsted Grotesk and Martian Mono, latin subset, variable |
| `scripts/capture.mjs` | the script that takes the shots from a running demo |

To preview, serve the folder: links to `nightly/` answer only once the docs are built beside it.

```bash
python3 -m http.server -d site 4400
```

## Ledger

A page is a ruled sheet: rows that open with a header bar in mono caps and a number in the margin,
content in cells divided by hairlines, two inks (black and ledger red) on pale green paper, and a
highlighter yellow for what the reader points at. Every class starts with `ld-`.

`assets/ledger.css` is the source. tablo-demo and tablo-saas each carry a copy as `ledger.css`,
imported by their `styles.css`; after a change here, copy it to both.

## Add an example

An entry is one `<article>` in `examples/index.html` and, when the app has a UI, a capture.

1. Capture the app at 1600 by 1000 pixels, as WebP, into `assets/captures/`.
2. Copy an `<article>` and fill it in: the capture, a label, a name, two sentences, tags, links.
3. Set `data-tags` to `live`, `template` or `repo`, as many as apply. The filters read them.
4. Open a pull request titled `docs(repo): list <name> in the examples`.

The bar: it runs, its source is public, and it is built on Tablo.

## Captures

`scripts/capture.mjs` signs in to a running tablo-demo, takes every shot the landing page shows,
and prints where the hero's callouts sit on the post list, which `index.html` positions them by.
Its header says how to run it. Start the demo fresh: it reseeds on every start, and the audit shot
edits records. Pass a running tablo-saas as the second address to add its home page.

## Fonts

[Schibsted Grotesk](https://github.com/schibsted/schibsted-grotesk) and
[Martian Mono](https://github.com/evilmartians/mono), both under the SIL Open Font License 1.1,
from their Fontsource packages. Each license sits beside its font in `assets/fonts/`.
