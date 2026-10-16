//! Structured logging (plan Phase 2 — G2-2, G5-3, G4-3).
//!
//! Before this module, the `log` facade was used throughout the crate but no
//! subscriber was ever registered, so every log call was a silent no-op
//! (G2-2); output was captured by shell redirection in `osa_mailer.cmd` into
//! a file named from `%date%` substrings, which breaks on non-English system
//! locales (G4-3). Now `run()` registers a process-wide `tracing` subscriber
//! that writes to stdout and appends to `logs/osa_mailer.log` (locale-free
//! name).
//!
//! Redaction (G5-3): e-mail context values (BCC lists, bodies, raw entry
//! contents) are never logged at the default level — diagnostic detail that
//! can carry PII is logged at `debug!` only.

use std::io;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing_subscriber::{
    fmt::{self, time::LocalTime},
    prelude::*,
    EnvFilter,
};

/// Where and how verbose the run's log output is.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Level filter, e.g. `"info"` or `"info,osa_mailer=debug"`.
    pub level: String,
    /// Directory the log file is written into; `None` means stdout only.
    pub file_dir: Option<PathBuf>,
}

impl LogConfig {
    /// Build the log configuration from the environment: the level comes
    /// from `OSA_LOG` (default `"info"`), the log file is written into
    /// `<base_dir>/logs/`.
    pub fn from_env(base_dir: &Path) -> Self {
        Self {
            level: std::env::var("OSA_LOG").unwrap_or_else(|_| "info".to_string()),
            file_dir: Some(base_dir.join("logs")),
        }
    }

    /// Build a log configuration with a fixed level and file directory.
    pub fn new(level: impl Into<String>, file_dir: Option<PathBuf>) -> Self {
        Self {
            level: level.into(),
            file_dir,
        }
    }

    /// The level filter for this configuration.
    pub fn filter(&self) -> anyhow::Result<EnvFilter> {
        Ok(EnvFilter::try_new(&self.level)
            .with_context(|| format!("invalid log level \"{}\"", self.level))?)
    }
}

/// Register the process-wide `tracing` subscriber.
///
/// Idempotent: if a default subscriber is already installed (a previous
/// `run()` in the same process, as in the test suite), the existing one is
/// kept and this returns without changing it.
pub fn init(config: &LogConfig) -> anyhow::Result<()> {
    // A default subscriber already exists (a previous run() in the same
    // process, as in the test suite): keep it.
    if tracing::dispatcher::has_been_set() {
        return Ok(());
    }

    // `RUST_LOG` (when set) takes precedence over the configured level.
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => config.filter()?,
    };

    let stdout_layer = fmt::layer()
        .with_timer(LocalTime::rfc_3339())
        .with_ansi(false)
        .with_target(true)
        .with_writer(io::stdout);

    let subscriber = tracing_subscriber::registry().with(filter).with(stdout_layer);

    // `try_init` fails only if a global subscriber was installed in the
    // meantime (a concurrent run in the same process): keep that one.
    match &config.file_dir {
        Some(dir) => {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("unable to create the log directory \"{}\"", dir.display()))?;

            // Locale-free, stable file name (the .cmd used `%date%`
            // substrings, G4-3); each run appends to the same file.
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("osa_mailer.log"))
                .with_context(|| format!("unable to open the log file in \"{}\"", dir.display()))?;

            let file_layer = fmt::layer()
                .with_timer(LocalTime::rfc_3339())
                .with_ansi(false)
                .with_target(true)
                .with_writer(file);

            let _ = subscriber.with(file_layer).try_init();
        }
        None => {
            let _ = subscriber.try_init();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_registers_a_default_subscriber_and_is_idempotent() {
        // G2-2 regression: `run()` (via `init`) must leave a working global
        // subscriber behind. The work happens in a child thread so the test
        // holds no guard and installs the subscriber exactly the same way
        // in a fresh process or one that already has one.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let config = LogConfig::new("info", None);
            let first = init(&config);
            let second = init(&config);
            let current = tracing::dispatcher::has_been_set();
            let _ = tx.send((first, second, current));
        })
        .join()
        .expect("worker thread panicked");

        let (first, second, current) = rx
            .recv()
            .expect("worker thread must report back");
        first.expect("init must succeed on first call");
        second.expect("init must be idempotent (second call keeps the existing subscriber)");
        assert!(current, "a global subscriber must be installed after init");
    }

    #[test]
    fn a_valid_level_yields_a_filter() {
        let config = LogConfig::new("info", None);
        config.filter().expect("\"info\" is a valid level");
    }

    #[test]
    fn an_invalid_level_is_rejected() {
        // A bare word would parse as a module filter, so use the
        // `target=level` form: the level after `=` must be a real level.
        let config = LogConfig::new("definitely-not-a-target=bogus", None);
        assert!(
            config.filter().is_err(),
            "an invalid level must be rejected, not silently ignored"
        );
    }
}
