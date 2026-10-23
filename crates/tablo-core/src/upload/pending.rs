//! A [`DirUploader`](super::DirUploader)'s pending files: each one it stores carries an empty
//! marker under `.pending/` until a committed write keeps it, and a sweep deletes a file whose
//! marker outlived its age, the upload of a submission that never saved.

use std::{
    io::ErrorKind,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The subdirectory holding the markers; `store` never writes a name starting with a dot.
const MARKS: &str = ".pending";

/// How often a store sweeps at most, in seconds.
const SWEEP_EVERY: u64 = 3600;

/// When a pending file is old enough to delete, and when the last sweep ran.
#[derive(Debug)]
pub(super) struct Pending {
    age: Duration,
    /// Seconds since the epoch; 0 before the first sweep.
    swept: AtomicU64,
}

impl Pending {
    pub(super) fn new(age: Duration) -> Self {
        Self {
            age,
            swept: AtomicU64::new(0),
        }
    }

    /// Marks the file `name` in `dir` pending.
    pub(super) async fn mark(&self, dir: &Path, name: &str) -> std::io::Result<()> {
        tokio::fs::create_dir_all(dir.join(MARKS)).await?;
        tokio::fs::write(dir.join(MARKS).join(name), b"").await
    }

    /// Sweeps `dir` unless a sweep ran within the last hour.
    pub(super) async fn sweep_if_due(&self, dir: &Path) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let last = self.swept.load(Ordering::Relaxed);
        if now < last + SWEEP_EVERY
            || self
                .swept
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
        {
            return;
        }
        if let Err(error) = sweep(dir, self.age).await {
            tracing::error!(%error, dir = %dir.display(), "pending uploads not swept");
        }
    }
}

/// Keeps the file `name` in `dir`: drops its marker, if it has one.
pub(super) async fn keep(dir: &Path, name: &str) -> std::io::Result<()> {
    match tokio::fs::remove_file(dir.join(MARKS).join(name)).await {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Deletes each file in `dir` whose marker is older than `age`, then the marker.
pub(super) async fn sweep(dir: &Path, age: Duration) -> std::io::Result<()> {
    let mut marks = match tokio::fs::read_dir(dir.join(MARKS)).await {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        marks => marks?,
    };
    while let Some(mark) = marks.next_entry().await? {
        let stale = mark
            .metadata()
            .await?
            .modified()?
            .elapsed()
            .is_ok_and(|elapsed| elapsed >= age);
        if !stale {
            continue;
        }
        let name = mark.file_name();
        match tokio::fs::remove_file(dir.join(&name)).await {
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        tokio::fs::remove_file(mark.path()).await?;
    }
    Ok(())
}
