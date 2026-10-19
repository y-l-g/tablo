# Mount errors

`.panel(..)` checks every declaration before the panel serves a request, and returns a
`MountError` listing each mistake it found, not only the first. Each `DeclarationError` names the
resource or page type that declares it (`resource`, `None` for the panel's own configuration), the `Site` of the declaration it is in, and a
`DeclarationErrorKind`, whose `Display` is the message the app prints:

```text
panel '/admin' cannot mount:
  - `showcase::app::PostResource` form: field `slug` is marked unique, but no unique index covers its column: …
```

A test matches on the kind rather than the message; see
[Testing](./testing-and-benchmarks.md#testing-a-panel). The kinds are `#[non_exhaustive]`, so a
`match` on them ends in `_`.

| Site | The declaration |
| --- | --- |
| `Panel` | the panel's own configuration: its prefix, served directories, assets and auth |
| `Registration` | the resource or page itself: its registration, slug, actions or create columns |
| `Tenancy` | `ResourceDef::tenancy` |
| `Table` | `ResourceDef::table` |
| `Form` | `ResourceDef::form`, checked against the record form |
| `Detail` | `ResourceDef::detail` |
| `Relation(slug)` | the `ResourceDef::relation` to that resource |

## The router and the panel

| Kind | Refused when | Fix |
| --- | --- | --- |
| `MissingDb` | the router holds no `toasty::Db` | call `.app_context(db)` before `.panel(..)` |
| `MissingAuthModels` | auth is on and the `Db` does not register `AuthSession`, `AdminUser` for the shipped authenticator, `ColumnPreference` while a list has a toggleable column, or `AuthToken` for a feature that mails links | add them to `toasty::models!(..)`, or `.auth(Auth::disabled())` |
| `ShellAssetsWithoutBundle` | `shell_assets` is set and the router has no asset bundle | call `.assets(..)` before `.panel(..)` |
| `FileFieldWithoutUploader` | a form declares a file field and the panel installs no uploader | `Panel::uploads` or `Panel::uploads_dir` |
| `InvalidSegment` | the prefix, a `ResourceDef::slug` or a `PageDef::slug` is empty, `.` or `..`, or holds whitespace, a control character, URL punctuation, a quote, a backslash or one of `{ } ( )` | pick a plain segment such as `blog-posts` |
| `PrefixOverlapsRuntime` | the prefix overlaps Topcoat's `/_topcoat/runtime` | pick another prefix |
| `PrefixOverlapsPanel` | the prefix is another panel's, or one is under the other (`/admin` and `/admin/reports`) | give each panel a prefix of its own |
| `EndpointOutsideRuntime` | a `Panel::endpoint` route is not a shard or procedure under `/_topcoat/runtime` | declare only shards and procedures; the panel's prefix guards its other routes |
| `ServeDirWithoutCatchAll` | a `serve_dir` pattern does not end in a catch-all | end it in `{*file}`: `/uploads/{*file}` |
| `ServeDirTwice` | the panel serves one pattern twice | drop the second `serve_dir` |
| `ServeDirTaken` | another panel already serves the pattern | serve it from one panel |
| `SignUp` | the sign-up page cannot be served; the `SignUpFault` says why | see below |
| `Profile` | the profile page cannot be served; the `ProfileFault` says why | see below |
| `PasswordReset` | the password reset cannot be served; the `ResetFault` says why | see below |
| `AuthMail` | the auth's mail cannot be sent; the `AuthMailFault` says why | see below |

`SignUpFault`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `AuthDisabled` | `Auth::registration` on `Auth::disabled()` | turn authentication on |
| `UserType` | the registrar creates another user type than the authenticator loads | register the type the panel signs in |
| `ReservedField` | a sign-up field is named `csrf_token` or `next` | rename the field |
| `FileField` | the sign-up form has a file field | ask for the file once the account signs in |
| `RelationshipField` | the sign-up form has a relationship choice, which would list records to an anonymous visitor | offer static options |

`ProfileFault`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `AuthDisabled` | `Auth::profile` on `Auth::disabled()` | turn authentication on |
| `UserType` | the profile editor edits another user type than the authenticator loads | edit the type the panel signs in |
| `ReservedField` | a profile field is named `csrf_token` | rename the field |
| `FileField` | the profile form has a file field | edit the file on a resource's form |
| `RelationshipField` | the profile form has a relationship choice, whose options no route serves the page | offer static options |

`ResetFault`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `AuthDisabled` | `Auth::password_reset` on `Auth::disabled()` | turn authentication on |
| `UserType` | the resetter resets another user type than the authenticator loads | reset the type the panel signs in |

`AuthMailFault`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `Unset` | a feature that mails links, such as `Auth::password_reset`, on an `Auth` without `Auth::mail` | install a sender with `Auth::mail(AuthMail::new(..))` |
| `BaseUrl` | the base URL is not an absolute `http` or `https` URL, or carries a query or fragment | pass the scheme and host, such as `https://admin.example.com` |
| `MissingMail` | the router registers no `MailConfig` | call `.mail(..)` before `.panel(..)` |

## Registration

| Kind | Refused when | Fix |
| --- | --- | --- |
| `DuplicateResource` | one resource type is registered twice on a panel | register it once; mount a variant under its own type |
| `DuplicatePage` | one page type is registered twice on a panel, as `home` or `page` | register it once; mount a variant under its own type |
| `SecondHome` | `Panel::home` is called twice | keep one home page; register the other with `page` |
| `ReservedSlug` | a slug is `login`, `logout`, `register`, `profile`, `forgot-password`, `reset-password` or `-`, which the panel routes itself | rename it with `ResourceDef::slug` or `PageDef::slug` |
| `DuplicateSlug` | two resources or pages share a slug | give each a distinct slug |
| `DuplicateAction` | two actions of a resource, record or header, share a `NAME` | rename one |
| `ActionInput` | an action's input cannot be served; the `ActionInputFault` says why | see below |
| `UnwrittenColumn` | the resource serves a create page and nothing writes a non-nullable column: not the form, not a Toasty `#[default(..)]`, not the tenant stamp, not a `create_column` | add the field to the form, a default to the model, or a `create_column`; or make the resource `edit_only` |
| `CreateColumnsNameTenant` | a `create_column` names the tenant column | drop it: the framework stamps the tenant |

`ActionInputFault`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `ReservedField` | an input field is named `csrf_token`, `confirm` or `ids` | rename the field |
| `FileField` | the input has a file field, which an action's POST does not upload | take the file in a record form |
| `RefusesEmpty` | the input has no field but refuses an empty submission | declare the fields it parses, or use `()` |
| `DependentChoice` | the input has a dependent choice | drop `depends_on` |

An action's `NAME` that is not one URL segment does not compile, so it never reaches the mount.

## Tables and detail pages

| Kind | Refused when | Fix |
| --- | --- | --- |
| `NoColumns` | a table has no column | declare columns; a resource whose model's table lists nothing declares its own `table` |
| `ZeroPageSize` | `Table::paginate(0)` | a page size of at least 1 |
| `UnsortableDefaultSort` | `Table::default_sort` names no sortable column | name a column declared `.sortable()` |
| `DuplicateColumn` | two columns share a name | rename one |
| `DuplicateFilter` | two filters share a name | rename one |
| `DottedFilterName` | a filter's name holds a `.`, which its URL parameters reserve for a part | rename it without the `.` |
| `UnparsedFilterOption` | a `SelectFilter` option's value does not parse as the field's type, so it filters nothing | spell it as the field's form value, or build the filter with `SelectFilter::of` |
| `UnregisteredLabelSource` | `RelationColumn::of::<R>` names a resource the panel does not register | register `R`, or use `RelationColumn::new` with a projection |

## Forms

| Kind | Refused when | Fix |
| --- | --- | --- |
| `DuplicateField` | two fields share a name | each input binds a distinct field |
| `MissingControl` | a hand-written `RecordForm::control` renders no control for a record-form field | render the field's control |
| `FormClaimsTenantColumn` | a tenant-owned resource's record form has a field for the tenant column | drop the field: the framework stamps the tenant |
| `EmptyChoice` | a choice has neither options nor a relationship | `.options(..)`, `.relationship::<R>()`, or `#[form(relationship = R)]` |
| `CompositeKeyChoice` | a relationship choice or filter loads a model with a composite primary key | choose over a model with a single-column key |
| `UnregisteredOptionSource` | a relationship choice or filter takes its options from a resource the panel does not register | register that resource |
| `UniqueWithoutIndex` | a `unique()` field's column has no unique index | add `#[unique]` to the model, or drop `.unique()` |
| `OptionalUnique` | a `unique()` field answers an empty submission and its column is not nullable, so every empty submission stores one value | make the field required, or an `Option` |
| `DisabledWithoutValue` | a field disabled on create has no `default` and no blank answer | give it a `default`, make it optional, or use `disabled_on_edit` |
| `UnpostedDefault` | a field's `default` is a value its control never posts, or one value on a multiple choice | spell the default as the field's form value, a list for a multiple choice |
| `RepeaterLeafModifier` | a repeater's item disables a control of its row or gives it a default | disable the repeater's own field |
| `RepeaterPassword` | a repeater's item has a password control, which an edit never renders back | keep the secret out of the repeater's item |
| `UnplacedWatchedField` | a condition watches a field the schema does not place | place the watched field in the same schema |
| `HiddenWatchedField` | a condition watches a field another condition hides while the guarded field still shows | place the guarded field inside the block that hides the watched one |
| `UnpostedConditionValue` | a condition names a value its watched field never posts | name one of its options, or `true` or `false` for a toggle |
| `RequiredConditionalField` | a field a condition hides has no blank answer | make it an `Option`, or give it `#[form(blank = ..)]` |
| `MisdeclaredDependentChoice` | a dependent choice has no relationship, or its column is not on the relationship's model | `relationship::<R>()`, and a column of `R`'s model in `depends_on` |
| `MultipleDependentChoice` | a dependent choice is multiple, or depends on a multiple one | drop `depends_on` or `multiple` |
| `UnplacedParentField` | a dependent choice depends on a field the schema does not place | place that field in the same schema |
| `FieldRule` | a text field's length, bounds or pattern cannot work; the `FieldRuleFault` says why | see below |
| `ManyToMany` | a many-to-many field cannot link records; the `ManyToManyFault` says why | see below |

`FieldRuleFault`, for a field or a control in a repeater's rows:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `UnorderedBound` | a `min` or `max` is no value of the field's type, or the type has no order | bound a number, date, time or timestamp field by a value of its type |
| `InvalidPattern` | a `pattern` does not compile on the server | fix the pattern |
| `EmptyRange` | a minimum is above its maximum, so the field refuses every value | lower the minimum or raise the maximum |
| `RefusedDefault` | the field's default breaks one of its rules, so a create that keeps it is refused | change the default or the rule |

`ManyToManyFault`, for a form's field or a `Relation::belongs_to_many`:

| Fault | Refused when | Fix |
| --- | --- | --- |
| `NotJoinModel` | the field is no `#[has_many(via = joins.target)]` through a join model | declare it through a `has_many` to the join model and the join model's `belongs_to` |
| `CompositeKey` | a foreign key of the join model spans several columns, or references no single-column key | key each side by one column |
| `UnwritableColumn` | the join model has a column a new link cannot fill | make it nullable or `#[auto]`, or link the records in your own record fn |
| `NotMultipleChoice` | the field's control is not a multiple choice over a relationship | `.choice().relationship::<R>().multiple()` |
| `OtherSource` | the choice offers another model's records than the join model links | point the relationship at the target's resource |
| `NoJoin` | a multiple choice binds no many-to-many field | bind it to a `#[has_many(via = ..)]` field |

## Lenses and bindings

| Kind | Refused when | Fix |
| --- | --- | --- |
| `TraversalLens` | a lens of several steps goes where one field of the model is needed | name one field of the model |
| `UnresolvedLens` | a multi-step lens reaches no single column: it hops a relation instead of stepping into embedded values | step only through embedded structs, enum variant fields and `#[document]` fields |
| `Uncompiled` | an embedded lens's model compiles to no schema under Toasty's PostgreSQL, SQLite, MySQL or DynamoDB driver, so its path names no column; the error gives each driver's reason | fix the model for your driver |

## Tenancy

| Kind | Refused when | Fix |
| --- | --- | --- |
| `TenancyColumnNotAField` | the `Tenancy::column` lens is not one field of the model | name the model's `TenantId` field, or use `Tenancy::via` |
| `TenancyViaOwnColumn` | the `Tenancy::via` lens names one field of the model | use `Tenancy::column` |
| `TenancyViaWithoutBelongsTo` | the `Tenancy::via` lens does not start at a `belongs_to` | start at the relation whose parent holds the tenant |
| `UnguardedForeignKey` | a `Tenancy::via` resource's form writes the parent's foreign key other than through a relationship choice over a tenant-scoped resource | write the key through such a choice, so the write re-checks its tenant |

## Relations

| Kind | Refused when | Fix |
| --- | --- | --- |
| `UnregisteredRelation` | a relation names a resource the panel does not register | register the related resource |
| `DuplicateRelation` | a resource declares two relations to one resource | keep one |

## At request time

`NotMounted` answers a request with a 500 instead of refusing a mount: app code asked for a
resource that the context's panel does not register, through `panel::wired_table`, `scoped_query`,
`write_create` or `write_update`. Register the resource on that panel, or reach it through a
context of a panel that does.
