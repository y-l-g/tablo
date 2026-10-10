# Coming from Filament or Django

Tablo follows the shape of Filament's panels and Django's admin: one declaration per model, from
which the list, form and detail pages follow. This table maps their names to Tablo's.

| Filament | Django admin | Tablo |
| --- | --- | --- |
| `PanelProvider` | `AdminSite` | [`Panel`](./panel-and-routing.md), mounted on your router with `.panel(..)` |
| `Resource` | `ModelAdmin` | a type implementing [`Resource`](./resources.md), declaring a `ResourceDef` |
| `getEloquentQuery()` | `get_queryset()` | `Resource::query` |
| `table()`: columns | `list_display` | `ResourceDef::table(Table::new(columns))` |
| `->searchable()`, `->sortable()` | `search_fields`, `ordering` | `.searchable()`, `.sortable()` on a column |
| table filters | `list_filter` | `Table::filters`: `SelectFilter`, `TernaryFilter`, `DateFilter`, `QueryFilter` |
| `->paginated([..])` | `list_per_page` | `Table::paginate(n)` |
| `->groups([..])` | — | `Table::group_by(lens)` |
| `form()` | `ModelAdmin.form`, `fields` | `ResourceDef::form`: a `Schema` of the [record form](./forms.md)'s controls |
| `make:filament-resource --generate` | `ModelForm` | `#[derive(ModelForm)]` on the model, which writes the [record form](./forms.md#a-form-from-the-model) |
| form state | `cleaned_data` | the `#[derive(RecordForm)]` struct a submission parses into |
| `Section`, `Grid`, `Group` | `fieldsets` | the `Section`, `Grid` and `Group` layout blocks |
| `->required()`, `->nullable()` | `blank=True` | the record-form field's type: an `Option` or a blank answer makes it optional |
| `->disabled()` | `readonly_fields` | `.disabled()`, `.disabled_on_edit()` |
| `->visible(fn (Get $get) ..)` | — | `.visible_when(..)`, a [condition](./forms.md#conditional-fields) |
| a dependent `Select` | — | `.depends_on(..)`, a [dependent choice](./forms.md#dependent-choices) |
| `Repeater` | an inline of a JSON field | `Field::repeater` over a `#[document]` list |
| `rules()`, `->unique()` | `clean()` | `Resource::validate_record`, `.unique()` |
| `mutateFormDataBeforeCreate`, `handleRecordCreation` | `save_model()` | `Resource::create_record`, `update_record`, delegating to `write_create`, `write_update` |
| `infolist()` | the change page, read-only | `ResourceDef::detail`: a [`Detail`](./detail-pages.md) of columns |
| `RelationManager` | `TabularInline` | `ResourceDef::relation(Relation::has_many::<R>(..))`, the related resource's table |
| `BelongsToMany` relation manager | `filter_horizontal` | `Relation::belongs_to_many`, or a [many-to-many field](./forms.md#many-to-many-fields) |
| `$recordTitleAttribute` | `__str__` | `ResourceDef::record_title(lens)` |
| policies (`viewAny`, `update`, …) | `has_view_permission`, `has_change_permission`, … | one [policy](./policy-auth-tenancy.md#policy) answering each `Ability` |
| table actions, bulk actions | `actions` | `Action<R>` types with `PLACES`, added by `ResourceDef::action` |
| header actions | — | `HeaderAction` types, added by `ResourceDef::header_action` |
| custom pages | `get_urls()` views | a type implementing `Page`, registered by `Panel::page` |
| dashboard | `index()` | `Panel::home::<P>()` |
| `$navigationGroup`, `$navigationSort`, `$navigationIcon` | — | `navigation_group`, `navigation_order`, `icon` on the def |
| tenancy | — | `ResourceDef::tenancy`: `Tenancy::column` or `Tenancy::via` |
| `Notification::make()` | `message_user()` | `Notification::success(..)` and its siblings |

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
