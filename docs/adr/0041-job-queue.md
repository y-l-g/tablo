# 0041 Tablo ships a job queue in the app's database, in a crate of its own

`AuthMailDelivery` lets a panel's auth mail commit with its token instead of going out after the
response, where a crash or a mail-server outage loses it. But the seam does only half the work:
an app that installs it still writes the queue, the worker and the sealing the letter's live link
needs. Every production app writes the same few hundred lines, and the first app to do it got lane
exclusion wrong across workers. `tablo-jobs` ships the queue, a worker built from a
`PanelHandle`, schedules, and `AuthMailQueue`, the delivery that seals each letter under a key
derived from the cookie key.

- The queue lives in the app's database, through Toasty, so a job enqueued in a write's
  transaction commits with it: no broker, and no second store to keep consistent.
- It is a crate of its own built on the public API, as ADR-0040 has the audit log. The core gains
  nothing for it: `PanelHandle::context`, `AuthMailDelivery` and `AuthLetter` were enough.
- A job returns `topcoat::Result`, the error the rest of Tablo returns, so a job calls panel entry
  points with `?`.
- Claims are optimistic: a version-checked update per job, and a unique `running_lane` column that
  holds a lane while one of its jobs runs. Toasty has no `SELECT … FOR UPDATE SKIP LOCKED`, and
  the two checks need none.
- Delivery is at least once. A lapsed lock reruns the job, so a job is idempotent.
- A worker claims only the kinds it registers, so workers may split the kinds between them, and a
  kind a newer release queues waits for a worker that knows it instead of spending its attempts.
- Toasty's MySQL driver has no targeted upsert, so a dedupe key there is a look-up then an insert,
  and the unique index refusing the insert counts as the key held.
- The tables are `tablo_jobs` and `tablo_job_schedules`, so the app's own tables keep their names.

What the queue does not do: priorities, a cron syntax, a list page for the queue, or a job that
returns a value. `QueuedJob` is a panel model, so an app lists its queue through a resource of its
own.

## Rejected

- Leaving the queue to each app, with `AuthMailDelivery` as the only seam: each app writes the
  same queue and the same sealing, and gets them wrong in the same places.
- `AuthMail::queued()` in the core: it would put the queue's tables and worker in `tablo-core`,
  which every app builds whether it runs a worker or not.
- A broker such as Redis or a hosted queue: a second store that a write cannot commit with.
- Generic over the error type: two error types in one app is what the core's `topcoat::Result`
  exists to avoid.
