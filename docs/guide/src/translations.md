# Translations

A panel renders its words in the request's locale. Tablo ships English and French catalogs, and
an app adds its own in [Fluent](https://projectfluent.org/), the format a message picks its plural
and gender forms in.

## Offering locales

[`Panel::locales`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.locales) lists
the locales a panel offers, the default first, and
[`Panel::translations`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.translations)
adds the app's catalog for one. A locale is a
[`topcoat::l10n::Locale`](https://docs.rs/topcoat/latest/topcoat/l10n/struct.Locale.html), which
`locale!` checks at compile time; enable the `l10n` feature of the app's `topcoat` dependency to
name it:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:locales}}
```

A panel that lists none offers English alone. A request reads, in order:

1. the locale the visitor picked from the shell's language menu;
2. the signed-in user's `PanelUser::locale`, when the panel offers a match;
3. the best match of the browser's `Accept-Language`, by weight, without a language it weighs
   `q=0`;
4. the panel's default, the first it lists.

## The language menu

A panel offering several locales shows them in the sidebar footer, beside the theme: picking one
posts to `{prefix}/locale` and lands back on the page it was picked from, and a cookie under the
panel's prefix remembers it for a year, so two panels on one host read in two languages. A panel
offering one locale shows no menu and serves no post, and a locale dropped from `Panel::locales`
since a visitor picked it reads as none.

Each locale is named by an attribute of the message `tablo-shell-language`, which Tablo's catalogs
spell for English and French; a panel offering another locale names it in its own catalog, and one
no catalog names reads as its tag:

```ftl
tablo-shell-language =
    .de = Deutsch
```

A region reads its language's locale: `fr-CA` reads a panel's `fr`, and `zh-HK` reads `zh-Hant`
over `zh-Hans`. The page's `<html lang>` names the locale it was read in, its `<html dir>` the
direction the locale's script runs in, `rtl` for Arabic or Hebrew, and a panel offering several
locales answers `Vary: Accept-Language, Cookie`, so a shared cache keeps one copy per language. A page,
a handler or a view of the panel reads the request's locale with `topcoat::l10n::locale(cx)`.

The panel refuses to mount an empty list, a locale listed twice (`iw` and `he` name one), a
locale no catalog translates, a catalog for a locale the panel does not offer (`en` aside), and a
catalog that is not valid Fluent, each as a
[`LocaleFault`](https://y-l.fr/tablo/nightly/api/tablo/enum.LocaleFault.html) under
`DeclarationErrorKind::Locale` ([Mount errors](./mount-errors.md)).

## Catalogs

A catalog holds the app's messages, and any of Tablo's it spells otherwise:

```ftl
{{#include ../../../examples/guide/locales/fr.ftl:catalog}}
```

A message the request's locale leaves out reads from the default locale's catalogs, then from
English, so a partial translation renders, and so does an attribute a message redefines without.
An English locale reads English alone. Tablo's message ids start with `tablo-`, so an app's
other ids never collide with them; its `locales/en.ftl` lists every one, and an app overrides one
by defining its id.

A catalog calls Fluent's `NUMBER` function, such as `{ NUMBER($total, minimumFractionDigits: 2) }`.
Numbers render without grouping in every locale.

## Labels

Every label a declaration takes is a [`Text`](https://y-l.fr/tablo/nightly/api/tablo/struct.Text.html):
a string renders as written, and `text!` names a message, with its arguments:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:label}}
```

A label Tablo derives from a name is a message whose fallback is the derived English, so an app
translates it in its catalog without declaring it:

| Declared | Message | Fallback |
| --- | --- | --- |
| a field, column or filter on `word_count` | `field-word_count` | Word count |
| a resource over the model `BlogPost` | `resource-blog-post` | Blog post |
| its plural | `resource-blog-post.plural` | Blog posts |
| the plural of a label `text!("post")` | `post.plural` | the id |
| a page `ReportsPage` | `page-reports` | Reports |
| an action named `publish-drafts` | `action-publish-drafts` | Publish drafts |
| the variant `PublishedLate` of an `Options` enum `PostStatus` | `option-post-status.published_late` | Published late |

A field, its column and its filter share the message of their name, so one translation spells
all three. An enum's options are the attributes of one message, which spells the enum's select, its
column's cells, its group headers and its filter's options:

```ftl
option-post-status =
    .draft = Brouillon
    .published_late = Publié en retard
```

An embedded enum's variant reads the same message. `#[option(label = "..")]` declares a plain
label, and a choice's `options` takes `(value, label)` pairs whose label is a `Text`, so
`[("draft", text!("status-draft"))]` translates too. A plain label's plural is the label pluralized in English: one written in another
language wants its `plural_label` declared.

## Words outside a label

`t!` spells a message in the request's locale where a `String` is wanted, such as a
notification's title, which is stored as sent:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:t}}
```

A `Text` also renders in a `view!` as it is, read in the view's locale. The auth mails, a reset
link, a verification link and a sign-in code, are written in their recipient's
`PanelUser::locale`, else in the locale of the request that sends them.

## The user's locale

`PanelUser::locale` names the locale a user reads the panel in, such as one they pick on their
profile and the app stores as text. A locale with no match among the panel's counts as none, and
one the user picks from the language menu outranks it for the rest of their visits, though their
mails still read the stored one:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:user-locale}}
```

## Finding untranslated words

Offer the pseudo-locale `en-XA` to see which words no catalog spells: it renders every catalog
message with accented letters, so a word that reads plain is one written as a plain string.

What an operator reads, a mount error or a log line, stays English. So do the few words of the UI
primitives Tablo copies from upstream ("More", "More pages", "Sidebar", "Toggle sidebar").
