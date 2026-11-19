//! A durable job queue for Tablo apps, in the app's own database, and the queued auth mail built
//! on it.
//!
//! A job enqueued through a write's transaction commits with that write or not at all, so "the
//! invitation exists" and "its mail will go out" cannot disagree. A [`Worker`] runs the queue
//! against a panel's context, and any number of workers can share it. It is built on Tablo's
//! public API alone (ADR-0041).
//!
//! ```rust,no_run
//! use serde::{Deserialize, Serialize};
//! use tablo_jobs::{Job, Worker, enqueue};
//! use topcoat::context::Cx;
//!
//! #[derive(Serialize, Deserialize)]
//! struct SendInvitation {
//!     invitation: u64,
//! }
//!
//! impl Job for SendInvitation {
//!     const KIND: &'static str = "send-invitation";
//!
//!     async fn run(self, cx: &Cx) -> topcoat::Result<()> {
//!         // Load the invitation, and mail it unless it is gone.
//!         # let _ = cx;
//!         Ok(())
//!     }
//! }
//!
//! # async fn invite(tx: &mut dyn toasty::Executor, id: u64) -> topcoat::Result<()> {
//! // Inside the transaction that writes the invitation:
//! enqueue(&SendInvitation { invitation: id }).exec(tx).await?;
//! # Ok(())
//! # }
//! # async fn work(panel: &tablo_core::PanelHandle) -> topcoat::Result<()> {
//! // In the worker process, or beside the server:
//! Worker::new(panel)?
//!     .register::<SendInvitation>()
//!     .drain()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! The `Db` registers [`QueuedJob`] and [`JobSchedule`]: list them in the app's
//! [`models!`](tablo_core::models!), as
//! `tablo::models!(Post, tablo::jobs::QueuedJob, tablo::jobs::JobSchedule)`.

mod job;
mod mail;
mod model;
mod queue;
mod schedule;
mod worker;

pub use self::{
    job::Job,
    mail::AuthMailQueue,
    model::{JobSchedule, JobState, QueuedJob},
    queue::{Enqueue, enqueue},
    worker::Worker,
};
