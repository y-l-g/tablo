# tablo-jobs

A durable job queue for Tablo apps, kept in the app's own database, and the queued auth mail built
on it. A job enqueued through a write's transaction commits with that write or not at all, and a
`Worker` runs the queue against a panel's context, with retries, backoff, lanes, dedupe keys and
schedules. Any number of workers share one queue.

It is built on `tablo-core`'s public API alone. The `tablo` facade re-exports it as `tablo::jobs`
behind the `jobs` feature:

```sh
cargo add tablo --features jobs
```

```rust
use tablo::jobs::{AuthMailQueue, Worker, enqueue};

let queue = AuthMailQueue::new(&cookie_key);
let mail = AuthMail::new(from, "https://app.example.com").delivery(queue.clone());

let worker = Worker::new(&panel)?
    .register::<SendInvitation>()
    .auth_mail(&queue)
    .context(move |cx| cx.with(Mailer::new(transport.clone())));
```

The `Db` registers `tablo::jobs::QueuedJob` and `tablo::jobs::JobSchedule` beside the app's
models.
