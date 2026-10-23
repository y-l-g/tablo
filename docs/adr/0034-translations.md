# 0034 A panel's words are Fluent messages its request's locale spells

Every word Tablo renders is a message of a Fluent catalog, and every label an app declares is a
`Text`: plain, rendering as written, or a message the request's locale spells. `Panel::locales`
lists the locales a panel offers, the default first, and `Panel::translations` adds the app's
catalog for one. A request reads the signed-in user's `PanelUser::locale`, else the best match of
its `Accept-Language`, else the panel's default.

- Fluent rather than gettext or key-value files: a message selects its own plural and gender
  variants, so a sentence such as "3 posts deleted" translates without code knowing the
  language's plural rules.
- Each locale's bundle layers English, then the default locale, then the locale itself, with the
  app's catalog over Tablo's at each layer. A message a catalog leaves out reads from the layer
  below, so a partial translation renders, and an app overrides any of Tablo's words by
  redefining its id. An English locale skips the default's layer: English is complete, and a
  derived label's fallback is English.
- Tablo's message ids start with `tablo-`. An app's do not, so they never collide.
- A label Tablo derives from a name is a message with an English fallback: the field `word_count`
  reads the message `field-word_count`, else "Word count". An app translates its derived labels
  in its catalog without declaring them. Fields, columns and filters share the id of their name,
  so one message translates all three.
- `Text` implements `From<&str>` and `From<String>`, so a declaration written in one language
  keeps compiling, and `text!` builds a message with its arguments. A label is rendered where the
  request is known, never at mount.
- Mails are written in the locale of the request that sends them.
- A shipped catalog is held by a test to the messages Tablo's code names and to English's
  variables, so a missing or misspelled id fails CI rather than rendering its id.
- The pseudo-locale `en-XA` accents every catalog letter, so a page shows at a glance which words
  no catalog spells.
- What an operator reads, a declaration error or a log line, stays English.

## Rejected

- Laravel's `__()` over a string key: the English sentence is the id, so fixing its wording
  breaks every translation, and plural forms need a second API.
- A `Translator` trait an app implements: every app would restate the negotiation and the
  fallback chain, and Tablo's own words would need one per app.
- Resolving labels to strings at mount: one panel serves many locales.
- A locale segment in the URL: links and redirects would all carry it, and the user's setting and
  the browser already say which locale to read.
- Translating the synced UI primitives: they are a copy of upstream components, and their few
  words ("More", "Toggle sidebar") are left in English.
