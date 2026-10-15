//! Claim/commit protocol for the outbox (upgrade plan Phase 2):
//!
//! ```text
//! outbox/            entries waiting for a run
//! outbox/processing/ entries claimed by a run, not yet delivered
//! outbox/archive/    entries delivered successfully (kept, never deleted)
//! outbox/failed/     dead letters (failed more than MAX_ATTEMPTS times)
//! ```
//!
//! A failed send moves the entry back to the outbox and records an attempt
//! in a `<entry>.state` sidecar; over the attempt limit it is dead-lettered.
//! A run that dies mid-send leaves entries in `processing/`; the next run
//! reclaims them (`reclaim_orphans`) so no entry is lost or double-sent
//! (plan gaps G1-4 / G1-3 / G2-1).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// An entry that has failed this many times is dead-lettered.
pub const MAX_ATTEMPTS: u32 = 5;

const PROCESSING_DIR: &str = "processing";
const ARCHIVE_DIR: &str = "archive";
const FAILED_DIR: &str = "failed";
const STATE_SUFFIX: &str = ".state";

#[derive(Debug)]
pub struct OutboxPaths {
    pub outbox: PathBuf,
    pub processing: PathBuf,
    pub archive: PathBuf,
    pub failed: PathBuf,
}

impl OutboxPaths {
    pub fn new(base: &Path) -> Self {
        let outbox = base.join(super::ENTRY_DIR);
        Self {
            processing: outbox.join(PROCESSING_DIR),
            archive: outbox.join(ARCHIVE_DIR),
            failed: outbox.join(FAILED_DIR),
            outbox,
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [&self.processing, &self.archive, &self.failed] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct AttemptState {
    attempts: u32,
}

/// The attempt-counter sidecar of an outbox entry (`entry.json` ->
/// `entry.json.state`).
pub fn sidecar_for(entry: &Path) -> PathBuf {
    let mut name = entry
        .file_name()
        .expect("entry path has a file name")
        .to_os_string();
    name.push(STATE_SUFFIX);
    entry.with_file_name(name)
}

/// The recorded attempt count of an entry (0 when no sidecar exists).
pub fn read_attempts(entry: &Path) -> u32 {
    match fs::read_to_string(sidecar_for(entry)) {
        Ok(s) => serde_json::from_str::<AttemptState>(&s)
            .map(|s| s.attempts)
            .unwrap_or(0),
        Err(_) => 0,
    }
}

fn write_attempts(entry: &Path, attempts: u32) -> std::io::Result<()> {
    let state = serde_json::to_string(&AttemptState { attempts })
        .expect("attempt state serializes");
    fs::write(sidecar_for(entry), state)
}

/// Record a failed send for an entry that is (again) in the outbox:
/// increments the attempt counter, and moves the entry to `failed/`
/// (dead-letter) once the attempt limit is exceeded.
pub fn record_failure(entry: &Path, paths: &OutboxPaths) -> std::io::Result<()> {
    let attempts = read_attempts(entry) + 1;
    if attempts > MAX_ATTEMPTS {
        let dead_letter = paths
            .failed
            .join(entry.file_name().expect("entry path has a file name"));
        fs::rename(entry, dead_letter)?;
        let _ = fs::remove_file(sidecar_for(entry));
    } else {
        write_attempts(entry, attempts)?;
    }
    Ok(())
}

/// Commit a successful send: move the claimed entry from `processing/` to
/// `archive/` and remove its attempt sidecar. The entry is kept in the
/// archive (never simply deleted) so no delivery state is lost.
pub fn commit_sent(
    processing_entry: &Path,
    paths: &OutboxPaths,
    sidecar: &Path,
) -> std::io::Result<()> {
    let archived = paths
        .archive
        .join(processing_entry.file_name().expect("entry path has a file name"));
    fs::rename(processing_entry, archived)?;
    let _ = fs::remove_file(sidecar);
    Ok(())
}

/// Crash recovery: move every entry left in `processing/` (by a killed run)
/// back to the outbox so it can be retried.
pub fn reclaim_orphans(paths: &OutboxPaths) -> std::io::Result<Vec<PathBuf>> {
    let mut reclaimed = Vec::new();
    if !paths.processing.is_dir() {
        return Ok(reclaimed);
    }
    for direntry in fs::read_dir(&paths.processing)? {
        let direntry = direntry?;
        if direntry.file_type()?.is_file() {
            let dest = paths.outbox.join(direntry.file_name());
            fs::rename(direntry.path(), &dest)?;
            reclaimed.push(dest);
        }
    }
    Ok(reclaimed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_name_is_entry_plus_suffix() {
        assert_eq!(
            sidecar_for(&Path::new("outbox").join("retry.json")).file_name().unwrap(),
            "retry.json.state"
        );
    }

    #[test]
    fn attempts_default_to_zero_and_round_trip() {
        let dir = std::env::temp_dir().join(format!("osa_mailer_outbox_state_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let entry = dir.join("e.json");
        fs::write(&entry, "{}").unwrap();

        assert_eq!(read_attempts(&entry), 0);
        write_attempts(&entry, 3).unwrap();
        assert_eq!(read_attempts(&entry), 3);
        assert!(dir.join("e.json.state").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_failure_dead_letters_over_limit() {
        let dir = std::env::temp_dir().join(format!("osa_mailer_outbox_dl_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let paths = OutboxPaths::new(&dir);
        paths.ensure_dirs().unwrap();

        let entry = paths.outbox.join("e.json");
        fs::write(&entry, "{}").unwrap();

        for expected in 1..=MAX_ATTEMPTS {
            record_failure(&entry, &paths).unwrap();
            assert_eq!(read_attempts(&entry), expected);
            assert!(entry.exists(), "still retriable at attempt {expected}");
        }
        // The next failure crosses the limit: dead-lettered
        record_failure(&entry, &paths).unwrap();
        assert!(!entry.exists());
        assert!(paths.failed.join("e.json").exists());
        assert!(!paths.outbox.join("e.json.state").exists());

        let _ = fs::remove_dir_all(&dir);
    }
}
