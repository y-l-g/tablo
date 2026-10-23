# Translations

A panel renders its words in the request's locale. Tablo ships English and French catalogs, and
an app adds its own in [Fluent](https://projectfluent.org/), the format a message picks its plural
and gender forms in.

## Offering locales

[`Panel::locales`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.locales) lists
the locales a panel offers, the default first, and
[`Panel::translations`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.translations)
adds the app's catalog for one:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:locales}}
```

A panel that lists none offers English alone. A request reads, in order:

1. the signed-in user's `PanelUser::locale`, when the panel offers it;
2. the best match of the browser's `Accept-Language`;
3. the panel's default, the first it lists.

A region reads its language's locale: `fr-CA` reads a panel's `fr`. The page's `<html lang>`
names the locale it was read in.

The panel refuses to mount an empty list, a tag that is not BCP 47, a locale listed twice, a
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
English, so a partial translation renders. An English locale reads English alone. Tablo's
message ids start with `tablo-`, so an app's never collide with them; its `locales/en.ftl` lists
every one.

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
| a page `ReportsPage` | `page-reports` | Reports |
| an action named `publish-drafts` | `action-publish-drafts` | Publish drafts |

A field, its column and its filter share the message of their name, so one translation spells
all three.

## Words outside a label

`t!` spells a message in the request's locale where a `String` is wanted, such as a
notification's title, which is stored as sent:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:t}}
```

A `Text` also renders in a `view!` as it is, read in the view's locale. Mails are written in the
locale of the request that sends them.

## The user's locale

`PanelUser::locale` names the locale a user reads the panel in, such as one they pick on their
profile and the app stores. A locale the panel does not offer counts as none:

```rust,ignore
{{#include ../../../examples/guide/src/translations.rs:user-locale}}
```

## Finding untranslated words

Offer the pseudo-locale `en-XA` to see which words no catalog spells: it renders every catalog
message with accented letters, so a word that reads plain is one written as a plain string.

What an operator reads, a mount error or a log line, stays English. So do the few words of the UI
primitives Tablo copies from upstream ("More", "Toggle sidebar").
