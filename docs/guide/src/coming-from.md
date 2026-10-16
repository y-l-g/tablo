# Coming from Filament

Tablo follows the shape of Filament's panels: one declaration per model, from
which the list, form and detail pages follow. This table maps their names to Tablo's.

| Filament | Tablo |
| --- | --- |
| `PanelProvider` | [`Panel`](./panel-and-routing.md), mounted on your router with `.panel(..)` |
| `Resource` | a type implementing [`Resource`](./resources.md), declaring a `ResourceDef` |
| `getEloquentQuery()` | `Resource::query` |
| `table()`: columns | `ResourceDef::table(Table::new(columns))` |
| `->searchable()`, `->sortable()` | `.searchable()`, `.sortable()` on a column |
| table filters | `Table::filters`: `SelectFilter`, `TernaryFilter`, `DateFilter`, `QueryFilter` |
| `->paginated([..])` | `Table::paginate(n)` |
| `->groups([..])` | `Table::group_by(lens)` |
| `form()` | `ResourceDef::form`: a `Schema` of the [record form](./forms.md)'s controls |
| `make:filament-resource --generate` | `#[derive(ModelForm)]` on the model, which writes the [record form](./forms.md#a-form-from-the-model) |
| form state | the `#[derive(RecordForm)]` struct a submission parses into |
| `Section`, `Grid`, `Group` | the `Section`, `Grid` and `Group` layout blocks |
| `->required()`, `->nullable()` | the record-form field's type: an `Option` or a blank answer makes it optional |
| `->disabled()` | `.disabled()`, `.disabled_on_edit()` |
| `->visible(fn (Get $get) ..)` | `.visible_when(..)`, a [condition](./forms.md#conditional-fields) |
| a dependent `Select` | `.depends_on(..)`, a [dependent choice](./forms.md#dependent-choices) |
| `Repeater` | `Field::repeater` over a `#[document]` list |
| `->minLength()`, `->maxLength()`, `->minValue()`, `->maxValue()`, `->regex()` | `.min_length(n)`, `.max_length(n)`, `.min(..)`, `.max(..)`, `.pattern(..)` on a text field |
| `rules()`, `->unique()` | `Resource::validate_record`, `.unique()` |
| `mutateFormDataBeforeCreate`, `handleRecordCreation` | `Resource::create_record`, `update_record`, delegating to `write_create`, `write_update` |
| `infolist()` | `ResourceDef::detail`: a [`Detail`](./detail-pages.md) of columns |
| `RelationManager` | `ResourceDef::relation(Relation::has_many::<R>(..))`, the related resource's table |
| `BelongsToMany` relation manager | `Relation::belongs_to_many`, or a [many-to-many field](./forms.md#many-to-many-fields) |
| `$recordTitleAttribute` | `ResourceDef::record_title(lens)` |
| policies (`viewAny`, `update`, …) | one [policy](./policy-auth-tenancy.md#policy) answering each `Ability` |
| table actions, bulk actions | `Action<R>` types with `PLACES`, added by `ResourceDef::action` |
| header actions | `HeaderAction` types, added by `ResourceDef::header_action` |
| custom pages | a type implementing `Page`, registered by `Panel::page` |
| dashboard | `Panel::home::<P>()` |
| `$navigationGroup`, `$navigationSort`, `$navigationIcon` | `navigation_group`, `navigation_order`, `icon` on the def |
| tenancy | `ResourceDef::tenancy`: `Tenancy::column` or `Tenancy::via` |
| `Notification::make()` | `Notification::success(..)` and its siblings |

## What works differently

- **Mistakes refuse the mount.** A field the record form does not have, a slug two resources
  share, a unique field without a unique index: `.panel(..)` returns a `MountError` naming each
  one before the server takes a request. See [Mount errors](./mount-errors.md).
- **Lenses, not strings.** A column, field or filter names its model field with `lens!(Post.title)`,
  so a renamed or retyped field is a compile error.
- **The record form is a type.** A submission parses into your `RecordForm` struct, and the
  `Schema` only lays its controls out. A POST with a key the form does not declare answers 400.
- **The policy denies by default.** A new resource shows nothing until its policy allows
  `ViewAny`, and every write re-checks the policy on the row it loads.
- **No inline editing.** A relation renders the related resource's table on the detail and edit
  pages, and its rows link to that resource's own forms. A list of values edited in place is a
  repeater over a `#[document]` field.
- **Server-rendered.** There is no Livewire or JavaScript component model: Topcoat reruns the
  page on the server and patches it in place.
