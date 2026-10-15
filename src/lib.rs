pub mod app;
pub mod entries;
pub mod errors;
pub mod outbox;
pub mod render;
pub mod send;

use anyhow::Context;
use lettre::transport::smtp::authentication::Credentials;
use std::{env, fs, path::PathBuf, rc::Rc};

use crate::render::{ContextData, TemplateData};

const ENTRY_DIR: &str = "outbox";
const ENTRY_EXT: &str = ".json";
const TEMPLATE_DIR: &str = "templates";

/// Everything a single mail-sending pass needs.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Directory that contains `outbox/` and `templates/` (normally the
    /// directory the executable lives in; tests point this at a temp dir).
    pub base_dir: PathBuf,
    pub server: String,
    pub port: u16,
    pub auth: send::Authentication,
    pub credentials: Option<Credentials>,
}

impl RunConfig {
    /// Build the config from the process environment
    /// (`SERVER` / `PORT` / `AUTH` / `USERNAME` / `PASSWORD`).
    pub fn from_env(base_dir: impl Into<PathBuf>) -> anyhow::Result<Self> {
        // TODO: Make static and use CLI ARGUMENTS instead
        let server = env::var("SERVER").unwrap_or_else(|_| "localhost".to_string());
        let port: u16 = env::var("PORT")
            .unwrap_or_else(|_| "25".to_string())
            .parse()
            .context("The `PORT` environment variable is not a valid port number")?;

        let auth: send::Authentication = env::var("AUTH")
            .unwrap_or_else(|_| "noauth".to_string())
            .parse()
            .context("The `AUTH` environment variable is not a valid authentication method")?;

        let credentials = match (env::var("USERNAME"), env::var("PASSWORD")) {
            (Ok(username), Ok(password)) => Some(Credentials::new(username, password)),
            _ => None,
        };

        Ok(Self {
            base_dir: base_dir.into(),
            server,
            port,
            auth,
            credentials,
        })
    }
}

/// One mail-sending pass: scan the outbox, merge the entries, render the
/// templates, send the e-mails and remove the entries that were sent.
///
/// Returns an error if at least one e-mail failed to be sent (plan gap
/// G1-13) so schedulers and callers can detect failures; template and
/// render failures are per-mail and do not abort the pass (G1-8).
pub fn run(config: RunConfig) -> anyhow::Result<()> {
    // `outbox/` and `templates/` live next to the executable; `app_path`
    // resolves them relative to a base directory that tests can override.
    let outbox_path = app_path::AppPath::new(ENTRY_DIR)
        .with_context(|| "Unable to resolve the outbox directory")?
        .with_base(&config.base_dir);
    let templates_path = app_path::AppPath::new(TEMPLATE_DIR)
        .with_context(|| "Unable to resolve the templates directory")?
        .with_base(&config.base_dir);

    // Claim/commit protocol (plan Phase 2): reclaim entries that a killed
    // run left in `outbox/processing/`, then scan the outbox top level only
    // (never the processing/archive/failed subdirectories or `.state`
    // sidecars).
    let outbox_paths = outbox::OutboxPaths::new(&config.base_dir);
    outbox_paths.ensure_dirs().with_context(|| "Unable to create outbox state directories")?;
    if let Ok(reclaimed) = outbox::reclaim_orphans(&outbox_paths) {
        for entry in reclaimed {
            log::debug!("Reclaimed orphaned entry: {}", entry.display());
        }
    }

    let entry_parse_results = entries::load_entries(outbox_path.full(), ENTRY_EXT);

    if !entry_parse_results.err.is_empty() {
        eprintln!("Entry parsing errors: {:?}", entry_parse_results.err);
    }

    let entries_pool = entry_parse_results.ok;

    let emails_map = entries::merge_email_requests(&entries_pool); // Each E-Mail ID with its E-mail contents, in order

    let composed_emails = entries::compose_emails(&emails_map)
        .with_context(|| "Unable to compose e-mails from the parsed entries")?;

    // The full composed e-mails (including BCC and context) must not be
    // printed: they are persisted by the documented .cmd log redirection
    // (plan gaps G1-14 / G5-3).
    println!(
        "Composed {} e-mail(s) from {} entr(y/ies)",
        composed_emails.len(),
        entries_pool.len()
    );

    let mut connection =
        send::Connection::new(&config.server, config.port, config.auth.clone());

    connection.establish(config.credentials.clone())?;

    let mut send_failures = 0usize;

    let mut app_state = app::AppState::new();

    for email in &composed_emails {
        let templates_root = templates_path.full().join(&email.header.template);
        let template_file = templates_root.join("template.html");
        let template_file_abs: render::AbsolutePath = template_file.clone().into();

        // Template loading and rendering are per-mail: a failure here logs
        // and continues instead of aborting the whole pass (plan gap G1-8).
        let rendered_template = (|| -> anyhow::Result<String> {
            let contents = fs::read_to_string(&template_file)
                .with_context(|| format!("Unable to load template file \"{}\"", template_file.display()))?;

            let template_data = TemplateData {
                contents: Rc::new(contents),
                file_path: Some(&template_file_abs),
            };

            let context_data = ContextData {
                context: serde_json::Value::Object(email.context.clone()),
                file_path: None,
            };

            render::render(
                &template_data,
                &context_data,
                render::DetectionMethod::Auto,
                render::TemplateExtension::Auto,
            )
            .map(|rendered| rendered.0.to_string())
        })();

        let html_payload = match rendered_template {
            Ok(html) => html,
            Err(e) => {
                // The e-mail was not delivered: count it as a failure so the
                // pass (and the scheduler) can see it (plan gap G1-13).
                eprintln!("Skipping e-mail \"{}\": {e:?}", email.header.subject);
                app_state.add_error_report(
                    errors::ErrorReport::new()
                        .set_context(format!("e-mail \"{}\"", email.header.subject))
                        .add_error(errors::ErrorWrapper(e)),
                );
                send_failures += 1;
                continue;
            }
        };

        let to = email.header.to.join(", ");
        let cc = email.header.cc.join(", ");
        let bcc = email.header.bcc.join(", ");
        let reply_to = email.header.reply_to.join(", ");
        let attachments = email.header.attachments.join(", ");

        // Build E-mail
        let message = match send::MessageBuilder::new()
            .from(&email.header.from)
            .to_addresses(&to)
            .cc_addresses(&cc)
            .bcc_addresses(&bcc)
            .reply_to_addresses(&reply_to)
            .subject(&email.header.subject)
            .alternative_content(&email.header.alternative_content)
            .content(&html_payload, Some(&templates_root))
            .attachments(&attachments, Some(&config.base_dir))
            .build()
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Skipping e-mail \"{}\": {e:?}", email.header.subject);
                app_state.add_error_report(
                    errors::ErrorReport::new()
                        .set_context(format!("e-mail \"{}\"", email.header.subject))
                        .add_error(errors::ErrorWrapper(e)),
                );
                send_failures += 1;
                continue;
            }
        };

        // Convert to Lettre Message
        let message = match message.try_into() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Skipping e-mail \"{}\": {e:?}", email.header.subject);
                app_state.add_error_report(
                    errors::ErrorReport::new()
                        .set_context(format!("e-mail \"{}\"", email.header.subject))
                        .add_error(errors::ErrorWrapper(e)),
                );
                send_failures += 1;
                continue;
            }
        };

        // The entries that make up this composed e-mail: in single mode this
        // is exactly the one entry it was built from, in batch mode the whole
        // group (plan gap G1-3).
        let entry_paths: Vec<PathBuf> = entries::entry_paths_for(email, &emails_map);

        // Claim: move each entry from `outbox/` to `outbox/processing/` so
        // at any moment an entry is in exactly one place — no duplicate
        // sends, no lost e-mails (plan gap G1-4).
        let mut claimed: Vec<(PathBuf, PathBuf)> = Vec::new(); // (processing, original)
        let mut claim_failed = false;
        for entry_path in &entry_paths {
            let claimed_path = outbox_paths
                .processing
                .join(entry_path.file_name().expect("entry path has a file name"));
            match fs::rename(entry_path, &claimed_path) {
                Ok(()) => claimed.push((claimed_path, entry_path.clone())),
                Err(e) => {
                    eprintln!(
                        "Failed to claim entry {} for e-mail \"{}\": {e}",
                        entry_path.display(),
                        email.header.subject
                    );
                    claim_failed = true;
                }
            }
        }
        if claim_failed || claimed.is_empty() {
            app_state.add_error_report(
                errors::ErrorReport::new()
                    .set_context(format!("e-mail \"{}\"", email.header.subject))
                    .add_error(errors::ErrorWrapper(anyhow::anyhow!(
                        "could not claim all entry files for the outbox"
                    ))),
            );
            send_failures += 1;
            continue;
        }

        match connection.send(message) {
            Ok(_) => {
                // Commit: delivered entries are archived (kept, never
                // deleted).
                for (processing_path, original_path) in &claimed {
                    let sidecar = outbox::sidecar_for(original_path);
                    match outbox::commit_sent(processing_path, &outbox_paths, &sidecar) {
                        Ok(()) => {
                            println!(
                                "Email sent successfully: \"{}\" (archived {})",
                                email.header.subject,
                                processing_path.display()
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: e-mail \"{}\" was sent but its entry could not be archived ({}): {e}",
                                email.header.subject,
                                processing_path.display()
                            );
                            send_failures += 1;
                        }
                    }
                }
            }
            // Sending failure
            Err(e) => {
                eprintln!("Failed to send e-mail \"{}\": {e}", email.header.subject);
                app_state.add_error_report(
                    errors::ErrorReport::new()
                        .set_context(format!("e-mail \"{}\"", email.header.subject))
                        .add_error(errors::ErrorWrapper(e)),
                );
                // Give the claimed entries back to the outbox and record the
                // attempt; over the attempt limit they are dead-lettered to
                // `outbox/failed/` (plan gap G2-1).
                for (processing_path, original_path) in &claimed {
                    match fs::rename(processing_path, original_path) {
                        Ok(()) => {
                            if let Err(e2) = outbox::record_failure(original_path, &outbox_paths) {
                                eprintln!(
                                    "Failed to record the attempt for {}: {e2}",
                                    original_path.display()
                                );
                                send_failures += 1;
                            }
                        }
                        Err(e2) => {
                            eprintln!(
                                "Failed to move claimed entry back to the outbox ({}): {e2}",
                                processing_path.display()
                            );
                            send_failures += 1;
                        }
                    }
                }
                send_failures += 1;
                continue;
            }
        }
    } // Each E-mail

    // Log the collected error reports (activates the previously dead
    // error-report scaffolding; plan gap G3-3).
    if let Some(reports) = app_state.error_reports() {
        for report in reports {
            for event in report.errors() {
                eprintln!(
                    "ErrorReport [{}]: {}",
                    report.context().unwrap_or("unknown context"),
                    event.0
                );
            }
        }
    }

    if send_failures > 0 {
        return Err(anyhow::anyhow!(
            "{} e-mail(s) failed to be sent",
            send_failures
        ));
    }

    Ok(())
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use std::path::Path;

    /// A minimal, always-parsable e-mail entry; `utc` and `subject` are
    /// unique per call so entries group separately unless intended.
    pub(crate) fn entry_json(id: &str, subject: &str, utc: &str, template: &str) -> String {
        serde_json::json!({
            "id": id,
            "utc": utc,
            "notify_error": ["Ops <ops@example.com>"],
            "email": {
                "system": "TestSystem",
                "subsystem": "TestSubsystem",
                "from": "From <from@example.com>",
                "to": ["To <to@example.com>"],
                "subject": subject,
                "template": template
            },
            "context": { "name": "World" }
        })
        .to_string()
    }

    fn setup(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("osa_mailer_run_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(ENTRY_DIR)).unwrap();
        fs::create_dir_all(base.join(TEMPLATE_DIR).join("tpl")).unwrap();
        fs::write(
            base.join(TEMPLATE_DIR).join("tpl").join("template.html"),
            "Hello {{ name }}!",
        )
        .unwrap();
        base
    }

    fn config(base: &Path) -> RunConfig {
        RunConfig {
            base_dir: base.to_path_buf(),
            server: "127.0.0.1".to_string(),
            // Port 1 is the "discard" port: nothing listens there, so the
            // TCP connection is refused quickly and deterministically.
            port: 1,
            auth: send::Authentication::NoAuth,
            credentials: None,
        }
    }

    #[test]
    fn run_fails_with_error_when_relay_is_unreachable() {
        // Plan gap G1-13: a failed send must surface as a process error
        // (non-zero exit), not a silent success.
        let base = setup("fail");
        fs::write(
            base.join(ENTRY_DIR).join("e1.json"),
            entry_json("e1", "Subject one", "2026-01-01T00:00:00+00:00", "tpl"),
        )
        .unwrap();

        let result = run(config(&base));
        assert!(result.is_err(), "run() must fail when the send fails");
        // The entry file is kept for a retry
        assert!(base.join(ENTRY_DIR).join("e1.json").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn run_continues_past_a_missing_template() {
        // Plan gap G1-8: a missing template must skip only its e-mail, not
        // abort the whole pass (previously the `?` aborted main()).
        let base = setup("missing_tpl");
        fs::write(
            base.join(ENTRY_DIR).join("ok.json"),
            entry_json("ok", "Good one", "2026-01-01T00:00:00+00:00", "tpl"),
        )
        .unwrap();
        fs::write(
            base.join(ENTRY_DIR).join("bad.json"),
            entry_json("bad", "Bad one", "2026-01-02T00:00:00+00:00", "no_such_template"),
        )
        .unwrap();

        // Both e-mails reach the (refusing) relay, so both fail — but the
        // pass completes and reports exactly two failures.
        let result = run(config(&base));
        let err = result.expect_err("relay refuses both sends");
        assert!(
            err.to_string().contains("2 e-mail(s) failed"),
            "both e-mails must be attempted, got: {err}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    /// Plan Phase 2 (claim/commit): a failed send keeps the entry in the
    /// outbox and records an attempt in a sidecar; an entry over the attempt
    /// limit is dead-lettered to `outbox/failed/`; the attempt dirs exist.
    /// No relay is needed: the local "relay" refuses every connection.
    #[test]
    fn run_records_attempts_and_dead_letters_over_limit_entries() {
        let base = setup("claim_commit");
        let outbox = base.join(ENTRY_DIR);

        let make_entry = |name: &str, subject: &str| {
            fs::write(
                outbox.join(name),
                entry_json(name, subject, "2026-01-01T00:00:00+00:00", "tpl"),
            )
            .unwrap();
        };
        make_entry("retry.json", "Will be retried");
        make_entry("dead.json", "Will be dead-lettered");

        // Simulate prior failed attempts for the entry that should die
        fs::write(outbox.join("dead.json.state"), r#"{"attempts": 5}"#).unwrap();

        let result = run(config(&base)).expect_err("the relay is down");
        // Both e-mails failed to deliver (one of them is now dead-lettered)
        assert!(result.to_string().contains("2 e-mail(s) failed"), "{result}");

        // Dead-lettered past the attempt limit
        assert!(
            outbox.join("failed").join("dead.json").exists(),
            "entry over the attempt limit must be dead-lettered to outbox/failed/"
        );
        assert!(!outbox.join("dead.json").exists());

        // Still retriable: kept in the outbox with an attempt sidecar
        assert!(outbox.join("retry.json").exists(), "failed entry stays in outbox");
        let state = fs::read_to_string(outbox.join("retry.json.state"))
            .expect("attempt sidecar must exist");
        let parsed: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(parsed["attempts"], 1, "first failure must record one attempt");

        // The claim/commit dirs exist
        assert!(outbox.join("archive").is_dir());
        assert!(outbox.join("processing").is_dir());

        // A second run retries `retry.json` (attempts now 2) — and the
        // dead-lettered entry must NOT be picked up again.
        let result = run(config(&base)).expect_err("the relay is still down");
        assert!(result.to_string().contains("1 e-mail(s) failed"), "{result}");
        let state = fs::read_to_string(outbox.join("retry.json.state")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(parsed["attempts"], 2, "second failure must increment to two");
        assert!(outbox.join("failed").join("dead.json").exists(), "dead letter stays put");

        let _ = fs::remove_dir_all(&base);
    }

    /// The archive side of claim/commit: the commit step of a successful
    /// send moves the entry from `processing/` to `outbox/archive/` and
    /// cleans up the attempt sidecar (it is never simply deleted).
    #[test]
    fn commit_sent_moves_entry_to_archive() {
        let base = setup("archive_direct");
        let paths = crate::outbox::OutboxPaths::new(&base);
        paths.ensure_dirs().unwrap();

        let processing_entry = paths.processing.join("a.json");
        fs::write(&processing_entry, "{}").unwrap();
        let sidecar = crate::outbox::sidecar_for(&paths.outbox.join("a.json"));
        fs::write(&sidecar, r#"{"attempts": 3}"#).unwrap();

        crate::outbox::commit_sent(&processing_entry, &paths, &sidecar).expect("commit");

        assert!(
            paths.archive.join("a.json").exists(),
            "a sent entry must be moved to outbox/archive/, not deleted"
        );
        assert!(!processing_entry.exists());
        assert!(!sidecar.exists(), "the attempt sidecar is cleaned up on commit");

        let _ = fs::remove_dir_all(&base);
    }

    /// Crash recovery: entries left in `processing/` by a killed run are
    /// claimed back into the outbox at the start of the next run.
    #[test]
    fn reclaim_moves_orphaned_processing_entries_back_to_outbox() {
        let base = setup("reclaim");
        let outbox = base.join(ENTRY_DIR);
        let paths = crate::outbox::OutboxPaths::new(&base);
        paths.ensure_dirs().unwrap();

        // Simulate a crash mid-send: the entry is stuck in processing/
        fs::write(paths.processing.join("orphan.json"), "{}").unwrap();

        let reclaimed = crate::outbox::reclaim_orphans(&paths).unwrap();
        assert_eq!(reclaimed.len(), 1);
        assert!(outbox.join("orphan.json").exists());
        assert!(!paths.processing.join("orphan.json").exists());

        let _ = fs::remove_dir_all(&base);
    }
}
