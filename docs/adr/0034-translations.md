# 0034 A panel's words are Fluent messages its request's locale spells

Every word Tablo renders is a message of a Fluent catalog, and every label an app declares is a
`Text`: plain, rendering as written, or a message the request's locale spells. `Panel::locales`
lists the locales a panel offers, the default first, and `Panel::translations` adds the app's
catalog for one. A request reads the signed-in user's `PanelUser::locale`, else the best match of
its `Accept-Language` by weight, else the panel's default. A region or script reads the closest
offered locale of its language, by likely subtags: `zh-HK` reads `zh-Hant`.

- A locale is Topcoat's `l10n::Locale`, and its negotiation, fallback chain and `Accept-Language`
  parse are Topcoat's `negotiate`, `lookup` and `accept_language`: an app names a locale with
  `locale!`, checked at compile time, and Tablo keeps no language-tag code of its own. Entering a
  panel installs the request's locale in its context, and signing in replaces it with the user's,
  so it is negotiated once per request and `topcoat::l10n::locale` reads it in an app's own views
  too. The page's `<html dir>` follows the locale's script.

- Fluent rather than gettext or key-value files: a message selects its own plural and gender
  variants, so a sentence such as "3 posts deleted" translates without code knowing the
  language's plural rules.
- Each locale's bundle layers English, then the default locale, then the locale itself, with the
  app's catalog over Tablo's at each layer. A message a catalog leaves out reads from the layer
  below, so a partial translation renders, and an app overrides any of Tablo's words by
  redefining its id. A message redefined without one of its attributes reads that attribute from
  below too: each locale keeps a bundle per height of its stack. An English locale skips the default's layer: English is complete, and a
  derived label's fallback is English.
- Tablo's message ids start with `tablo-`; an app's other ids never collide with them.
- A label Tablo derives from a name is a message with an English fallback: the field `word_count`
  reads the message `field-word_count`, else "Word count". An app translates its derived labels
  in its catalog without declaring them. Fields, columns and filters share the id of their name,
  so one message translates all three.
- An enum's options are the attributes of one message, `option-{enum}.{variant}`: the variant's
  snake-case name is always a valid Fluent identifier, where an option's value, which a form
  posts, need not be, and a translator finds an enum's words together. An option's label is a
  `Text`, spelled where the request is known: a form's select, a column's cell, a group's header,
  a filter's control.
- `Text` implements `From<&str>` and `From<String>`, so a declaration written in one language
  keeps compiling, and `text!` builds a message with its arguments. A label is rendered where the
  request is known, never at mount.
- Mails are written in their recipient's `PanelUser::locale`, else the sending request's: a
  password reset is asked for by someone signed out, maybe in another browser.
- The request's `Accept-Language` is matched against the panel's locales once per request, and
  only its first 16 languages, so a long header costs one short negotiation.
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
- Topcoat's `messages!` and `t!` for the catalogs: each `messages!` is a namespace of its own, so
  an app could not redefine one of Tablo's messages; `t!` takes an id fixed at compile time,
  where a derived label's id is built from a name; and a message a locale lacks renders its id
  rather than the default locale's or English's words. The upstream gaps are tracked as
  [#807](https://github.com/y-l-g/tablo/issues/807) to
  [#812](https://github.com/y-l-g/tablo/issues/812).
- Translating the synced UI primitives: they are a copy of upstream components, and their few
  words ("More", "More pages", "Sidebar", "Toggle sidebar") are left in English.
