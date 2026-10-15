//! Phase-1 integration test (plan §4): a fixture outbox is driven through
//! the real `run()` pipeline against an in-process mock SMTP server. The
//! server captures the DATA payload so the generated MIME can be asserted:
//! inline-image cids, an attachment part, and — the G1-1 regression — the
//! `multipart/alternative` part built from `alternative_content`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use osa_mailer::send::Authentication;
use osa_mailer::RunConfig;

/// Minimal 1x1 transparent PNG (recognized by the `infer` crate as
/// `image/png`), same bytes the unit tests use.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15,
    0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01,
    0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// A just-enough SMTP server: plain (no TLS), one connection, one message.
/// Responds 250 to everything it does not specifically handle and records
/// the DATA payload.
fn start_mock_smtp() -> (u16, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let captured: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let captured_in_thread = captured.clone();

    std::thread::spawn(move || {
        let (mut stream, _) = match listener.accept() {
            Ok(conn) => conn,
            Err(_) => return,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
        let _ = stream.write_all(b"220 mock ESMTP ready\r\n");

        let mut buf = [0u8; 8192];
        let mut data_mode = false;
        let mut data = String::new();

        loop {
            let n = match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = String::from_utf8_lossy(&buf[..n]).to_string();

            if data_mode {
                data.push_str(&chunk);
                if data.ends_with("\r\n.\r\n") {
                    data_mode = false;
                    *captured_in_thread.lock().unwrap() =
                        data.trim_end_matches("\r\n.\r\n").to_string();
                    let _ = stream.write_all(b"250 OK queued\r\n");
                }
                continue;
            }

            for line in chunk.lines() {
                let cmd = line.to_ascii_uppercase();
                if cmd.starts_with("EHLO") || cmd.starts_with("HELO") {
                    let _ = stream.write_all(b"250-mock\r\n250 8BITMIME\r\n");
                } else if cmd == "DATA" {
                    let _ = stream.write_all(b"354 go ahead\r\n");
                    data_mode = true;
                } else if cmd == "QUIT" {
                    let _ = stream.write_all(b"221 bye\r\n");
                    return;
                } else {
                    // MAIL FROM / RCPT TO / RSET / NOOP / ...
                    let _ = stream.write_all(b"250 OK\r\n");
                }
            }
        }
    });

    (port, captured)
}

/// Decode the Base64 body of the `text/html` part of a captured MIME
/// message (the HTML part carries a Base64 content-transfer-encoding, so
/// the rendered body — including any `cid:` references — is not visible
/// in the raw bytes).
fn decode_html_part(mime: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let lines: Vec<&str> = mime.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim().to_ascii_lowercase().starts_with("content-type: text/html"))
        .expect("message must contain a text/html part");

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

    let decoded = STANDARD.decode(body).expect("valid base64");
    String::from_utf8(decoded).expect("utf-8 html")
}

fn setup_base(tag: &str) -> std::path::PathBuf {
    let base =
        std::env::temp_dir().join(format!("osa_mailer_it_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("outbox")).unwrap();
    std::fs::create_dir_all(base.join("templates").join("tpl").join("images")).unwrap();
    std::fs::create_dir_all(base.join("files")).unwrap();
    base
}

/// Full pipeline over a real (mock) SMTP conversation: the e-mail carries an
/// inline image (cid), an attachment and an alternative text/plain part.
/// The entry must be archived after a successful delivery.
#[test]
fn run_sends_full_mime_to_smtp_and_archives_entry() {
    let base = setup_base("mime");

    // Template with the Tera magic comment; references an image relative to
    // the template directory.
    std::fs::write(
        base.join("templates").join("tpl").join("template.html"),
        "<!--template tera--><html><body><p>Hi {{ name }}</p><img src=\"images/logo.png\" alt=\"l\"></body></html>",
    )
    .unwrap();
    std::fs::write(
        base.join("templates").join("tpl").join("images").join("logo.png"),
        PNG_1X1,
    )
    .unwrap();
    std::fs::write(base.join("files").join("report.txt"), "attachment body").unwrap();

    let entry = serde_json::json!({
        "id": "it1",
        "utc": "2026-01-01T00:00:00+00:00",
        "notify_error": ["Ops <ops@example.com>"],
        "email": {
            "system": "TestSystem",
            "subsystem": "TestSubsystem",
            "from": "From <from@example.com>",
            "to": ["To <to@example.com>"],
            "subject": "Integration",
            "template": "tpl",
            "alternative_content": "Plain fallback body",
            "attachments": ["files/report.txt"]
        },
        "context": { "name": "World" }
    })
    .to_string();
    std::fs::write(base.join("outbox").join("it.json"), entry).unwrap();

    let (port, captured) = start_mock_smtp();
    let config = RunConfig {
        base_dir: base.clone(),
        server: "127.0.0.1".to_string(),
        port,
        auth: Authentication::NoAuth,
        credentials: None,
    };

    osa_mailer::run(config).expect("the e-mail must be delivered to the mock relay");

    // Delivered entries are archived (claim/commit), never deleted.
    assert!(
        base.join("outbox").join("archive").join("it.json").exists(),
        "a delivered entry must be moved to outbox/archive/"
    );

    let mime = captured.lock().unwrap().clone();
    assert!(!mime.is_empty(), "the relay must receive a message");

    // Envelope / headers
    assert!(mime.contains("Subject: Integration"), "subject header missing:\n{mime}");

    // Inline image: the img src must become a cid with a related part.
    // The HTML body is base64-encoded in the part, so decode it first.
    assert!(mime.contains("multipart/related"), "no multipart/related part:\n{mime}");
    let html = decode_html_part(&mime);
    assert!(html.contains("cid:image_0"), "no cid reference in the rendered body:\n{html}");
    assert!(
        !html.contains("src=\"images/logo.png\""),
        "the img src must be replaced by the cid:\n{html}"
    );
    assert!(html.contains("Hi World"), "the template must be rendered:\n{html}");
    assert!(mime.contains("image/png"), "no image/png part:\n{mime}");

    // Attachment
    assert!(mime.contains("report.txt"), "no attachment part:\n{mime}");

    // G1-1 regression: alternative_content must produce a
    // multipart/alternative with both a text/plain and a text/html part.
    assert!(
        mime.contains("multipart/alternative"),
        "no multipart/alternative part (G1-1 regression):\n{mime}"
    );
    assert!(mime.contains("text/plain"), "no text/plain part:\n{mime}");
    assert!(mime.contains("text/html"), "no text/html part:\n{mime}");

    let _ = std::fs::remove_dir_all(&base);
}
