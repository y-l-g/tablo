//! A [`DirUploader`](super::DirUploader)'s pending files: each one it stores carries an empty
//! marker under `.pending/` until a committed write keeps it, and a sweep deletes a file whose
//! marker outlived its age, the upload of a submission that never saved.
//!
//! A sweep claims a file by renaming its marker before deleting it, and a keep by removing the
//! marker, so only one of them wins: a kept file is never swept, and a keep that lost fails its
//! write.

use std::{
    io::ErrorKind,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The subdirectory holding the markers; `store` never writes a name starting with a dot.
const MARKS: &str = ".pending";

/// The prefix a sweep renames a marker to while it deletes the file.
const CLAIMED: &str = ".claimed-";

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

    /// Sweeps `dir` in the background unless a sweep ran within the last hour.
    pub(super) fn sweep_if_due(&self, dir: &Path) {
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
        let (dir, age) = (dir.to_path_buf(), self.age);
        tokio::spawn(async move {
            if let Err(error) = sweep(&dir, age).await {
                tracing::error!(%error, dir = %dir.display(), "pending uploads not swept");
            }
        });
    }
}

/// Keeps the file `name` in `dir`: drops its marker, if it has one. Fails when a sweep claimed
/// the file first, or it is gone.
pub(super) async fn keep(dir: &Path, name: &str) -> std::io::Result<()> {
    match tokio::fs::remove_file(dir.join(MARKS).join(name)).await {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
        Ok(()) => return Ok(()),
        Err(_) => {}
    }
    // A sweep renames the marker before it deletes the file, and the claim after: a claim still
    // there, or a file already gone, means the sweep won.
    let claimed = tokio::fs::try_exists(dir.join(MARKS).join(format!("{CLAIMED}{name}"))).await?;
    if claimed || !tokio::fs::try_exists(dir.join(name)).await? {
        return Err(std::io::Error::new(
            ErrorKind::NotFound,
            "the upload expired; upload the file again",
        ));
    }
    Ok(())
}

/// Drops the marker of the file `name` in `dir`, if it has one.
pub(super) async fn unmark(dir: &Path, name: &str) -> std::io::Result<()> {
    match tokio::fs::remove_file(dir.join(MARKS).join(name)).await {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Removes `path`; one already gone is no error.
async fn remove(path: &Path) -> std::io::Result<()> {
    match tokio::fs::remove_file(path).await {
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
        let Some(name) = mark.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // A claim left by a sweep that stopped halfway: finish it.
        if let Some(name) = name.strip_prefix(CLAIMED) {
            remove(&dir.join(name)).await?;
            remove(&mark.path()).await?;
            continue;
        }
        // A marker a keep removed since the listing is no longer pending.
        let modified = match mark.metadata().await {
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            metadata => metadata?.modified()?,
        };
        if !modified.elapsed().is_ok_and(|elapsed| elapsed >= age) {
            continue;
        }
        let claim = dir.join(MARKS).join(format!("{CLAIMED}{name}"));
        match tokio::fs::rename(mark.path(), &claim).await {
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            renamed => renamed?,
        }
        remove(&dir.join(&name)).await?;
        remove(&claim).await?;
    }
    Ok(())
}
