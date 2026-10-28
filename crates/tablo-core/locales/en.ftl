# Tablo's English: every message Tablo renders. An app's catalog overrides any of them by id
# (`Panel::translations`), and a locale's catalog leaves out what reads as here.

## Shell

tablo-shell-navigation = Navigation
tablo-shell-close-sidebar = Close sidebar
tablo-shell-home = Home
tablo-shell-account = Account
tablo-shell-profile = Profile
tablo-shell-two-factor = Two-factor sign-in
tablo-shell-sign-out = Sign out
tablo-shell-theme = Theme
tablo-shell-theme-light = Light
tablo-shell-theme-dark = Dark
tablo-shell-theme-system = System
tablo-shell-language = Language
    .en = English
    .fr = French
tablo-shell-tenants = Tenants
tablo-shell-switch-tenant = Switch tenant
tablo-shell-select-tenant = Select a tenant
tablo-shell-tenant-settings = Tenant settings
tablo-shell-create-tenant = Create tenant
tablo-crumb-create = Create
tablo-crumb-view = View
tablo-crumb-edit = Edit

## Error pages

tablo-error-not-found = Page not found
tablo-error-not-found-description = The page you asked for does not exist, or it has moved.
tablo-error-forbidden = Access denied
tablo-error-forbidden-description = You do not have permission to open this page.
tablo-error-failed = Something went wrong
tablo-error-failed-description = The page could not be shown. Try again in a moment.
tablo-error-home = Go home

## Global search

tablo-search = Search
tablo-search-placeholder = Search…
tablo-search-no-results = No results
tablo-search-all = See all results
tablo-search-nothing-matches = Nothing matches “{ $term }”.
# A detail's label before its value in a result.
tablo-search-detail = { $label }:

## Common

tablo-cancel = Cancel
tablo-confirm = Confirm
tablo-action-view = View
tablo-action-edit = Edit
tablo-action-delete = Delete
tablo-yes = Yes
tablo-no = No

## Lists

tablo-list-search = Search table
tablo-list-columns = Columns
tablo-list-columns-reset = Reset
tablo-list-columns-apply = Apply
tablo-list-delete-selected = Delete
tablo-list-select-row = Select row
tablo-list-actions = Actions
tablo-confirm-run = Run this action?
tablo-confirm-delete-record = Delete this record?
tablo-confirm-delete-selected = Delete the selected records?
tablo-confirm-irreversible = This action cannot be undone.
tablo-confirm-selected = Selected records:
tablo-list-select-all = Select all rows
tablo-list-select-matching = Select all matching
tablo-list-all-matching = All matching records
tablo-list-sort = Sort by { $column } { $direction ->
    [descending] descending
   *[ascending] ascending
}
tablo-list-empty = No records yet
tablo-list-empty-search = No matches for “{ $term }”
tablo-list-empty-filters = No results for these filters
tablo-list-clear-search = Clear search
tablo-list-clear-filters = Clear filters
tablo-list-filters = Filters
tablo-list-reset-filters = Reset
tablo-list-first-page = Back to first page
tablo-pager-showing = Showing { $first }–{ $last } of { $total }
tablo-pager-page = Page { $page }
tablo-pager-previous = Previous
tablo-pager-next = Next
tablo-pager-per-page = Per page
tablo-pager-apply = Apply
tablo-list-select-first = Select at least one row first
tablo-list-too-many-matching = More than { $max } records match: narrow the list to act on all of them
tablo-list-none-matching = No record matches the list
tablo-list-failed = Couldn't load { $label }
tablo-list-failed-description = Something went wrong while loading the records.
tablo-list-retry = Retry
tablo-list-export = Export CSV
tablo-list-import = Import CSV

# A date column's day, `month` from 1 to 12.
tablo-column-date = { $month ->
        [1] Jan
        [2] Feb
        [3] Mar
        [4] Apr
        [5] May
        [6] Jun
        [7] Jul
        [8] Aug
        [9] Sep
        [10] Oct
        [11] Nov
       *[12] Dec
    } { $day }, { $year }

## Filters

tablo-filter-ignored = Ignored filter(s):
# One filter the list ignores, and why.
tablo-filter-ignored-entry = { $filter } ({ $reason })
tablo-filter-ignored-consequence = { $unfiltered ->
    [true] — showing unfiltered results.
   *[false] — other filter(s) still apply.
}
tablo-filter-ignored-unknown = unknown filter
tablo-filter-ignored-dropped = more than { $filters } filters, more than { $values } values of one, over { $bytes } bytes, or the retired filters= form
tablo-filter-ignored-invalid = invalid value
tablo-filter-all = All
tablo-filter-true = True
tablo-filter-false = False
tablo-trash = Trashed
tablo-trash-without = Without trashed
tablo-trash-with = With trashed
tablo-trash-only = Only trashed

## Actions

tablo-action-restore = Restore
tablo-action-force-delete = Delete permanently
tablo-action-deleted = { $count ->
    [one] Deleted
   *[other] Deleted { $count } records
}
tablo-action-restored = { $count ->
    [one] Restored
   *[other] Restored { $count } records
}
tablo-action-force-deleted = { $count ->
    [one] Deleted permanently
   *[other] Deleted { $count } records permanently
}
tablo-confirm-restore = Restore this record?
tablo-confirm-force-delete = Delete this record permanently?
tablo-confirm-force-delete-selected = Delete the selected records permanently?
tablo-confirm-restore-selected = Restore the selected records?
# A run the action refused on some of the selection, after its success note.
tablo-action-skipped = ({ $skipped } of { $selected } skipped)
# The title of an action's input page over one record, and over a selection.
tablo-action-on-record = { $action }: { $record }
tablo-action-on-records = { $action }: { $count ->
    [one] { $count } record
   *[other] { $count } records
}
# A header action's success note.
tablo-action-done = { $action }: done
# A bulk action that no selected record may take.
tablo-action-refused-all = { $action }: { $count ->
    [one] { $count } selected record cannot take this action; nothing was changed
   *[other] { $count } selected records cannot take this action; nothing was changed
}

## Write failures

tablo-failed-description = Nothing was changed — try again.
tablo-failed-create = Couldn't create the record
tablo-failed-update = Couldn't save the changes
tablo-failed-import = Couldn't import the file
tablo-failed-delete = Couldn't delete the record
tablo-failed-bulk-delete = Couldn't delete the selected rows
tablo-failed-restore = Couldn't restore the records
tablo-failed-force-delete = Couldn't delete the records permanently
tablo-failed-run = Couldn't run the action
tablo-failed-attach = Couldn't attach the record
tablo-failed-detach = Couldn't detach the records

## Forms

tablo-form-created = Created
tablo-form-updated = Updated
tablo-form-create-title = Create { $label }
tablo-form-edit-title = Edit { $label }
tablo-form-create = Create
tablo-form-create-another = Create & create another
tablo-form-unsaved = Leave this page? Your changes are not saved.
tablo-form-save = Save
tablo-form-stale = This record changed since you opened it
tablo-form-stale-description = Saving again replaces those changes with yours. Reload the page to see them first.
tablo-form-unseen = A field you cannot see refuses this save
tablo-form-unseen-description = Ask someone who may edit it to correct the record.

## Relations

tablo-relation-attach = Attach
tablo-relation-attached = Attached
tablo-relation-detached = { $count ->
    [one] Detached { $count } record
   *[other] Detached { $count } records
}
tablo-relation-attach-title = Attach { $label }
tablo-relation-detach = Detach
tablo-relation-new = New { $label }

## Import

tablo-import-imported = { $count ->
    [one] Imported { $count } row
   *[other] Imported { $count } rows
}
tablo-import-title = Import { $label }
tablo-import-description = Each row of the file creates one record in { $label }. The first row names the columns; a refused row imports nothing, so correct the file and import it again.
tablo-import-template = Download template
tablo-import-file = CSV file
tablo-import-submit = Import
tablo-import-row = Row
tablo-import-column = Column
tablo-import-field = Field
tablo-import-problem = Problem
tablo-import-required = Required
tablo-import-repeat = Repeat the column for each value
tablo-import-refused = { $count ->
    [one] 1 value was refused: nothing was imported
   *[other] { $count } values were refused: nothing was imported
}
tablo-import-no-file = Choose a CSV file to import
tablo-import-not-utf8 = The file is not UTF-8 text: save it as CSV UTF-8
tablo-import-unterminated = A quoted cell opening on line { $line } is never closed
tablo-import-no-header = The file is empty: its first row names the columns
tablo-import-no-rows = The file holds a header but no row
tablo-import-too-many-rows = A file holds at most { $max } rows
tablo-import-too-many-cells = Row { $row } holds more than { $max } cells
tablo-import-unknown-columns = { $count ->
    [one] No field takes the column { $columns }
   *[other] No field takes the columns { $columns }
}
tablo-import-ambiguous-columns = More than one field is labelled { $columns }: name the column by its key
tablo-import-repeated-columns = The header names { $columns } more than once
tablo-import-stray-value = The row holds a value under no column of the header
tablo-import-unseen = A field you cannot see refuses this row

## Detail pages

tablo-detail-back = Back to list
# The title of a record whose resource names no title column: its label and its key.
tablo-record-title = { $label } { $key }

## Fields

tablo-field-required = { $label } is required
tablo-field-unreadable = The value could not be read
tablo-field-unavailable = { $label } is not available
tablo-field-select = -- Select --
tablo-repeater-add = Add item
tablo-field-invalid = { $label } is invalid
tablo-field-too-many-choices = { $label } takes at most { $max } new choices
tablo-field-options-retry = { $label } could not load options, retry
tablo-field-options-failed = { $label } could not load options
tablo-field-filter = Filter…
tablo-field-filter-options = Filter { $label } options
tablo-field-no-options = No options
tablo-field-no-matching-options = No matching options
tablo-field-searching = Searching…
tablo-field-too-many-options = Too many options — type to search
tablo-field-accepted = { $label } must be accepted
tablo-field-email = { $label } must be a valid email
tablo-field-url = { $label } must be a valid URL starting with http:// or https://
tablo-field-min-length = { $label } must be at least { $min } characters
tablo-field-max-length = { $label } must be at most { $max } characters
tablo-field-min = { $label } must be at least { $min }
tablo-field-max = { $label } must be at most { $max }
tablo-field-pattern = { $label } is not in the expected format
tablo-field-min-options = { $count ->
    [one] { $label } needs at least 1 option
   *[other] { $label } needs at least { $count } options
}
tablo-field-max-options = { $count ->
    [one] { $label } takes at most 1 option
   *[other] { $label } takes at most { $count } options
}
tablo-field-min-items = { $count ->
    [one] { $label } needs at least 1 item
   *[other] { $label } needs at least { $count } items
}
tablo-field-max-items = { $count ->
    [one] { $label } takes at most 1 item
   *[other] { $label } takes at most { $count } items
}
tablo-field-different = { $label } must differ from { $other }
tablo-field-gt = { $label } must be greater than { $other }
tablo-field-gte = { $label } must be at least { $other }
tablo-field-lt = { $label } must be less than { $other }
tablo-field-lte = { $label } must be at most { $other }
tablo-field-after = { $label } must be after { $other }
tablo-field-not-before = { $label } must be no earlier than { $other }
tablo-field-before = { $label } must be before { $other }
tablo-field-not-after = { $label } must be no later than { $other }
tablo-field-extension = { $count ->
    [one] { $label } must be a file ending in { $extensions }
   *[other] { $label } must be a file ending in one of { $extensions }
}
tablo-field-max-files = { $count ->
    [one] { $label } takes at most 1 file
   *[other] { $label } takes at most { $count } files
}
tablo-field-max-size = { $label } must be at most { $max }
tablo-field-upload-failed = { $label } could not be uploaded: { $reason }
tablo-size-bytes = { $count ->
    [one] 1 byte
   *[other] { $count } bytes
}
tablo-size-kib = { $count } KiB
tablo-size-mib = { $count } MiB
tablo-rich-toolbar = Formatting
tablo-rich-bold = Bold
tablo-rich-italic = Italic
tablo-rich-underline = Underline
tablo-rich-strike = Strikethrough
tablo-rich-code = Code
tablo-rich-link = Link
# The prompt the link tool asks for its URL with.
tablo-rich-link-url = Link URL
tablo-rich-heading-1 = Heading 1
tablo-rich-heading-2 = Heading 2
tablo-rich-heading-3 = Heading 3
tablo-rich-bullet-list = Bulleted list
tablo-rich-ordered-list = Numbered list
tablo-rich-blockquote = Quote
tablo-rich-code-block = Code block
tablo-rich-horizontal-rule = Horizontal rule
tablo-rich-undo = Undo
tablo-rich-redo = Redo
tablo-field-not-a = `{ $value }` is not a valid { $kind }
tablo-field-not-an-option = `{ $value }` is not a valid option
tablo-type-integer = whole number
tablo-type-boolean = yes/no value
tablo-type-number = number
tablo-type-identifier = identifier
tablo-type-timestamp = timestamp
tablo-type-date = date
tablo-type-time = time
tablo-type-date-time = date and time
tablo-file-keep = Leave empty to keep the current file.
tablo-file-remove = Remove the current file
tablo-file-current = Current:
tablo-files-add = Chosen files are added to the current ones.
tablo-files-remove = Remove
tablo-repeater-up = Move up
tablo-repeater-down = Move down
tablo-repeater-remove = Remove
tablo-field-taken = { $label } has already been taken
tablo-field-too-many-results = Too many results — keep typing

## Widgets

tablo-stat-trend-up = Up
tablo-stat-trend-down = Down
tablo-stat-trend-flat = Unchanged
tablo-chart-empty = No data
tablo-record-table-view-all = View all

## Notifications

tablo-notifications = Notifications
tablo-notifications-close = Close notifications
# The control that dismisses one toast.
tablo-toast-close = Close
tablo-notifications-none = No notifications
tablo-notifications-caught-up = You're all caught up.
tablo-notifications-unread = { $count ->
    [0] All read
   *[other] { $count } unread
}
tablo-notifications-read-all = Mark all as read
tablo-notifications-clear = Clear
tablo-notifications-more = Show more
tablo-notification-read = Mark as read
tablo-notification-unread = Mark as unread
tablo-notification-remove = Remove
tablo-ago-now = just now
tablo-ago-minutes = { $count } min ago
tablo-ago-hours = { $count } h ago
tablo-ago-days = { $count } d ago

## Sign-in

# The fields of the sign-in, sign-up, profile and password reset forms.
tablo-auth-name = Name
tablo-auth-email = Email
tablo-auth-password = Password
tablo-auth-confirm-password = Confirm password
tablo-auth-current-password = Current password
tablo-auth-new-password = New password
tablo-auth-confirm-new-password = Confirm new password
tablo-login-title = Sign in
tablo-login-description = Welcome back. Sign in to { $brand }.
tablo-login-identifier = Email or username
tablo-login-forgot = Forgot your password?
tablo-login-submit = Sign in
tablo-login-no-account = Don't have an account?
tablo-login-sign-up = Sign up
tablo-login-failed = Invalid email or password.
tablo-login-unavailable = Sign-in is unavailable right now. Try again shortly.
tablo-gate-description = Your account cannot open { $brand }.

## Sign-up

tablo-sign-up-title = Create an account
tablo-sign-up-description = Sign up for { $brand }.
tablo-sign-up-submit = Sign up
tablo-sign-up-have-account = Already have an account?
tablo-sign-up-sign-in-link = Sign in
tablo-sign-up-created = Account created
tablo-sign-up-pending = You can sign in once it is approved.
tablo-sign-up-sign-in = Sign in to continue.
tablo-sign-up-throttled = Too many sign-up attempts. Try again shortly.
tablo-sign-up-unavailable = Sign-up is unavailable right now. Try again shortly.
tablo-password-show = Show password
tablo-password-hide = Hide password
tablo-password-too-short = Password must be at least { $min } characters
tablo-password-mismatch = Passwords do not match

## Password reset

tablo-reset-forgot-title = Forgot your password?
tablo-reset-forgot-description = Enter your email and we will send you a link to choose a new password.
tablo-reset-request = Email me a reset link
tablo-reset-enter-email = Enter your email address
tablo-reset-throttled = Too many reset requests. Try again shortly.
tablo-reset-unavailable = Password reset is unavailable right now. Try again shortly.
tablo-reset-sent = Check your email
tablo-reset-sent-description = If an account uses that address, it has a link to reset its password.
tablo-reset-title = Choose a new password
tablo-reset-description = Use at least { $min } characters.
tablo-reset-submit = Reset password
tablo-reset-done = Password reset
tablo-reset-done-description = Sign in with your new password.
tablo-reset-invalid = This reset link is invalid or has expired.
tablo-reset-new-link = Request a new link
tablo-reset-expired-title = Reset link expired
tablo-reset-expired-description = Each link works once, for one hour.
tablo-reset-remember = Remember your password?
tablo-reset-mail-subject = Reset your { $brand } password
tablo-reset-mail-body =
    Someone asked to reset the password of your { $brand } account.

    Open this link to choose a new one. It works once, for the next { $minutes } minutes:

    { $link }

    If you did not ask, ignore this email: your password stays the same.

## Email verification

tablo-verify-title = Verify your email
tablo-verify-description = Open the link we mail to { $email } to confirm it is yours, then come back.
tablo-verify-request = Email me a verification link
tablo-verify-sent-description = A new verification link is on its way.
tablo-verify-done = Email verified
tablo-verify-invalid = This verification link is invalid or has expired.
tablo-verify-new-link = Sign in to get a new link
tablo-verify-expired-title = Verification link expired
tablo-verify-expired-description = Each link works once, for one day, for the address it was mailed to.
tablo-verify-mail-subject = Verify your { $brand } email address
tablo-verify-mail-body =
    Open this link to confirm that { $email } is the address of your { $brand } account. It works once, for the next 24 hours:

    { $link }

    If you did not create the account, ignore this email.

## Email change

tablo-email-change-confirm-title = Confirm your new email
tablo-email-change-confirm-description = Make { $email } the address you sign in with.
tablo-email-change-done = Email changed
tablo-email-change-done-description = You now sign in as { $email }.
tablo-email-change-keep-title = Keep your email
tablo-email-change-keep-description = Keep { $email } as the address you sign in with, even if the change went through, and sign every browser out.
tablo-email-change-keep = Keep my address
tablo-email-change-kept = Email kept
tablo-email-change-kept-description = You sign in as { $email }, and every browser is signed out. Reset your password if someone else knows it.
tablo-email-change-mail-subject = Confirm your new { $brand } email address
tablo-email-change-mail-body =
    Open this link to make { $new } the address of your { $brand } account. It works once, for the next 24 hours:

    { $link }

    If you did not ask for the change, ignore this email.
tablo-email-change-notice-subject = Your { $brand } email address is changing
tablo-email-change-notice-body =
    Someone asked to change the address of your { $brand } account to { $new }.

    If it was not you, open this link within 7 days to keep { $old }, even if the change went through, and sign every browser out, then reset your password:

    { $link }

## Profile

tablo-profile-saved = Profile saved
tablo-profile-email-pending = Open the link we mailed to your new address to start using it.
tablo-profile-current-password-needed = Enter your current password to change your email or password
tablo-profile-current-password-wrong = The current password is incorrect
tablo-profile-password-help = Leave blank to keep the current password.
tablo-profile-current-password-help = Required to change your email or password.

## Tenants

tablo-tenant-saved = Tenant saved
tablo-tenant-not-saved = Nothing saved
tablo-tenant-switched = You now act for { $tenant }. Check its settings before saving.
tablo-tenant-created = Tenant created
tablo-tenant-acting = You now act for { $tenant }.
tablo-tenant-create-title = Create a tenant
tablo-tenant-create-first = Create the tenant you work in on { $brand }.
tablo-tenant-create-another = Create another tenant to work in on { $brand }.
tablo-tenant-changed-mind = Changed your mind?
tablo-tenant-go-back = Go back

## Two-factor sign-in

tablo-two-factor-app-description = Enter the code your authenticator app shows.
tablo-two-factor-mailed-description = Enter the code we mailed you.
tablo-two-factor-code-label = Authentication code
tablo-two-factor-code-help = Or one of your recovery codes.
tablo-two-factor-verify = Verify
tablo-two-factor-resend = Send a new code
tablo-two-factor-sent = A new code is on its way.
tablo-two-factor-invalid = That code is not valid. Try the current one.
tablo-two-factor-not-you = Not you?
tablo-two-factor-switch = Sign in as someone else
tablo-two-factor-expired = Sign in again
tablo-two-factor-expired-description = The sign-in waited too long for its code.
tablo-two-factor-ended-description = Too many wrong codes. Enter your password to try again.
tablo-two-factor-alert-subject = Wrong sign-in codes on your { $brand } account
tablo-two-factor-alert-body =
    Your password signed in to { $brand }, then several wrong two-factor codes followed it.

    If this was not you, change your password: someone else knows it.
tablo-two-factor-off = Two-factor sign-in is off. Turn it on to ask for a code each time you sign in.
tablo-two-factor-password-help = Confirm it is you before adding a second factor.
tablo-two-factor-wrong-password = That password is not right.
tablo-two-factor-start-app = Set up an authenticator app
tablo-two-factor-start-email = Get codes by email
tablo-two-factor-email-setup = Enter the code mailed to { $address } to turn on codes by email.
tablo-two-factor-code = Code
tablo-two-factor-email-help = The six digits in the email.
tablo-two-factor-app-help = The six digits the app shows.
tablo-two-factor-scan = Scan the code with an authenticator app, or enter the key by hand, then enter the code it shows.
tablo-two-factor-qr = QR code for an authenticator app
tablo-two-factor-turn-on = Turn on
tablo-two-factor-turn-off = Turn off
tablo-two-factor-on = Two-factor sign-in is on
tablo-two-factor-on-app = Two-factor sign-in is on, with an authenticator app. { $left ->
    [one] 1 recovery code is left.
   *[other] { $left } recovery codes are left.
}
tablo-two-factor-on-email = Two-factor sign-in is on, with codes mailed to { $address }. { $left ->
    [one] 1 recovery code is left.
   *[other] { $left } recovery codes are left.
}
tablo-two-factor-on-app-help = A code from the app, or a recovery code.
tablo-two-factor-on-email-help = A mailed code, or a recovery code.
tablo-two-factor-new-codes = Show new recovery codes
tablo-two-factor-keep-codes = Keep these recovery codes somewhere safe. Each signs you in once without the app, and they are not shown again.
tablo-two-factor-done = Done
tablo-two-factor-sent-toast = Code sent
tablo-two-factor-off-toast = Two-factor sign-in turned off
tablo-two-factor-mail-subject = Your { $brand } sign-in code
tablo-two-factor-mail-body =
    Your code to sign in to { $brand } is { $code }. It works once, for the next 10 minutes.

    If you did not try to sign in, change your password: someone else knows it.
