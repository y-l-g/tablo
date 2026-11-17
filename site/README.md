# Tablo's site

The page at [y-l.fr/tablo](https://y-l.fr/tablo/): the landing page.
It is static HTML, CSS and a little JavaScript, with no build step. The docs workflow
(`.github/workflows/docs.yml`) publishes `index.html` and `assets/` at the root, beside
the guide, the API reference and the atlas under `nightly/`.

| Path | Holds |
| --- | --- |
| `index.html` | the landing page |
| `assets/ledger.css` | Ledger, the identity the site, tablo-demo and tablo-saas share |
| `assets/site.css`, `assets/site.js` | what only this site uses: the hero's callouts, the theme switcher |
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

## Captures

`scripts/capture.mjs` signs in to a running tablo-demo, takes every shot the landing page shows,
and prints where the hero's callouts sit on the post list, which `index.html` positions them by.
Its header says how to run it. Start the demo fresh: it reseeds on every start, and the audit shot
edits records. Pass a running tablo-saas as the second address to add its home page.

## Fonts

[Schibsted Grotesk](https://github.com/schibsted/schibsted-grotesk) and
[Martian Mono](https://github.com/evilmartians/mono), both under the SIL Open Font License 1.1,
from their Fontsource packages. Each license sits beside its font in `assets/fonts/`.
