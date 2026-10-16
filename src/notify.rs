//! The error-report e-mail (plan gaps G3-3/G3-4): when a run records
//! failures, one summary e-mail is sent to the `notify_error` recipients of
//! the affected entries. The e-mail is plain text and is built only from
//! the error reports (context = e-mail subject, error messages) — never
//! from e-mail context values, so the G5-3 redaction rule holds here too.

use crate::errors::ErrorReport;
use crate::send::Message;
use anyhow::Context;
use lettre::message::Message as LettreMessage;

/// The subject of the per-run summary e-mail.
pub(crate) fn failure_subject(failed: usize) -> String {
    format!("OSA Mailer: {failed} e-mail(s) failed to be sent")
}

/// The plain-text body: one block per error report, each error on its own
/// line with its timestamp.
pub(crate) fn failure_body(reports: &[ErrorReport]) -> String {
    let mut body = String::new();
    for report in reports {
        body.push_str("context: ");
        body.push_str(report.context().unwrap_or("unknown"));
        body.push('\n');
        for event in report.errors() {
            body.push_str(&format!("- {}: {}\n", event.0.to_rfc3339(), event.1));
        }
        body.push('\n');
    }
    body
}

/// Build the summary e-mail addressed to `to`, from `from`.
pub(crate) fn build_summary_email(
    from: &str,
    to: &[String],
    failed: usize,
    reports: &[ErrorReport],
) -> anyhow::Result<LettreMessage> {
    let to_joined = to.join(", ");
    let message = Message::default()
        .from(from)
        .with_context(|| "unable to parse the error-report `from` address")?
        .to_addresses(&to_joined)
        .with_context(|| "unable to parse the error-report `to` addresses")?
        .subject(&failure_subject(failed))
        .alternative_content(&failure_body(reports));
    message.try_into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{ErrorWrapper, EntryError};

    fn two_reports() -> Vec<ErrorReport> {
        vec![
            ErrorReport::new()
                .set_context("e-mail \"Subject one\"".to_string())
                .add_error(EntryError::MissingEmailSection),
            ErrorReport::new()
                .set_context("e-mail \"Subject two\"".to_string())
                .add_error(ErrorWrapper(anyhow::anyhow!("smtp dial failed"))),
        ]
    }

    #[test]
    fn failure_subject_names_the_failed_count() {
        assert_eq!(
            failure_subject(3),
            "OSA Mailer: 3 e-mail(s) failed to be sent"
        );
    }

    #[test]
    fn failure_body_lists_every_report_and_error() {
        let body = failure_body(&two_reports());
        assert!(body.contains("context: e-mail \"Subject one\""), "body: {body}");
        assert!(body.contains("context: e-mail \"Subject two\""), "body: {body}");
        assert!(
            body.contains(EntryError::MissingEmailSection.to_string().as_str()),
            "body: {body}"
        );
        assert!(body.contains("smtp dial failed"), "body: {body}");
    }

    /// Extract and decode the body of the message's `text/plain` part
    /// (base64-encoded and wrapped at line length in the message).
    fn decode_plain_part(raw: &str) -> String {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let lines: Vec<&str> = raw.lines().collect();
        let start = lines
            .iter()
            .position(|l| l.trim().to_ascii_lowercase().starts_with("content-type: text/plain"))
            .expect("message must contain a text/plain part");

        // Skip the remaining headers of the part (up to the blank line)
        let mut i = start + 1;
        while i < lines.len() && !lines[i].is_empty() && !lines[i].starts_with("--") {
            i += 1;
        }

        // Collect the (wrapped) base64 body lines up to the next boundary
        let mut body = String::new();
        while i < lines.len() && !lines[i].trim_start().starts_with("--") {
            body.push_str(lines[i].trim());
            i += 1;
        }

        String::from_utf8(STANDARD.decode(body.as_bytes()).expect("valid base64"))
            .expect("utf-8 body")
    }

    #[test]
    fn summary_email_carries_recipients_sender_subject_and_body() {
        let reports = two_reports();
        let msg = build_summary_email(
            "From <from@example.com>",
            &["Ops <ops@example.com>".to_string()],
            1,
            &reports,
        )
        .expect("the summary e-mail must build");

        let raw = String::from_utf8(msg.formatted()).expect("valid utf-8");
        assert!(raw.contains("To: Ops <ops@example.com>"), "raw: {raw}");
        assert!(raw.contains("From: From <from@example.com>"), "raw: {raw}");
        assert!(raw.contains(&failure_subject(1)), "raw: {raw}");

        // The plain part must carry exactly the report body (the message
        // formatting normalizes the text body to CRLF line endings).
        let decoded = decode_plain_part(&raw);
        assert_eq!(
            decoded,
            failure_body(&reports).replace('\n', "\r\n"),
            "the plain part must carry the report body"
        );
    }

    #[test]
    fn summary_email_rejects_bad_recipients() {
        assert!(build_summary_email(
            "not-an-address",
            &["Ops <ops@example.com>".to_string()],
            1,
            &two_reports(),
        )
        .is_err());
        assert!(build_summary_email(
            "From <from@example.com>",
            &["Ops <ops@example.com".to_string()],
            1,
            &two_reports(),
        )
        .is_err());
    }
}
