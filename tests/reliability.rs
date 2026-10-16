//! Phase 2 reliability integration tests (plan §4.2): structured logging
//! (G2-2, G5-3, G4-3) and per-e-mail SMTP robustness (G2-6).
//!
//! All tests share one base directory and serialize their `run()` calls
//! through a process mutex: the logging subscriber is process-global and
//! its log file is bound to the directory of the first `run()` in the
//! process, so a single shared directory keeps every log assertion
//! deterministic. Each test cleans the outbox top level before it runs.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use osa_mailer::logging::LogConfig;
use osa_mailer::send::Authentication;
use osa_mailer::RunConfig;

/// Serializes `run()` calls: the process-global subscriber and its log
/// file belong to the first run, and the shared outbox must not be
/// processed concurrently.
static RUN_LOCK: Mutex<()> = Mutex::new(());
static BASE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

fn base() -> &'static Path {
    BASE.get_or_init(|| {
        let base =
            std::env::temp_dir().join(format!("osa_mailer_reliability_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("outbox")).unwrap();
        std::fs::create_dir_all(base.join("templates").join("tpl")).unwrap();
        std::fs::write(
            base.join("templates").join("tpl").join("template.html"),
            "<!--template tera--><p>Hi {{ name }}!</p>",
        )
        .unwrap();
        base
    })
    .as_path()
}

/// Remove every entry (and its `.state` sidecar) from the outbox top level
/// so each test starts from a clean slate.
fn cleanup_outbox() {
    let outbox = base().join("outbox");
    for entry in std::fs::read_dir(&outbox).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") || name.ends_with(".json.state") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn write_entry(name: &str, id: &str, subject: &str, utc: &str, context: serde_json::Value) {
    let entry = serde_json::json!({
        "id": id,
        "utc": utc,
        "notify_error": ["Ops <ops@example.com>"],
        "email": {
            "system": "TestSystem",
            "subsystem": "TestSubsystem",
            "from": "From <from@example.com>",
            "to": ["To <to@example.com>"],
            "subject": subject,
            "template": "tpl"
        },
        "context": context
    })
    .to_string();
    std::fs::write(base().join("outbox").join(name), entry).unwrap();
}

fn config(port: u16, timeout: Duration) -> RunConfig {
    RunConfig {
        base_dir: base().to_path_buf(),
        server: "127.0.0.1".to_string(),
        port,
        auth: Authentication::NoAuth,
        credentials: None,
        smtp_timeout: timeout,
        log: LogConfig::new("info", Some(base().join("logs"))),
    }
}

fn log_content() -> String {
    std::fs::read_to_string(base().join("logs").join("osa_mailer.log")).unwrap_or_default()
}

/// How the mock relay treats each accepted TCP connection.
#[derive(Clone, Copy)]
enum Behavior {
    /// Answer 250 to everything, 354 to DATA; keep serving connections.
    Accept,
    /// First connection: greeting, then drop the TCP connection
    /// mid-session. Later connections behave like `Accept`.
    DropFirstThenAccept,
    /// Accept the TCP connection, then never say anything.
    Silent,
}

/// A multi-connection mock SMTP relay. `conns` collects the ephemeral
/// ports of the accepted connections — distinct ports prove distinct
/// TCP connections (per-e-mail dials, G2-6).
fn start_mock_smtp(behavior: Behavior) -> (u16, Arc<Mutex<Vec<u16>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
    let conns: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::new(AtomicUsize::new(0));

    let conns_in = conns.clone();
    let counter_in = counter.clone();
    std::thread::spawn(move || loop {
        let (mut stream, addr) = match listener.accept() {
            Ok(v) => v,
            Err(_) => break, // listener dropped: the test is over
        };
        conns_in.lock().unwrap().push(addr.port());
        let n = counter_in.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || match behavior {
            Behavior::Accept => serve_smtp(stream),
            Behavior::DropFirstThenAccept if n == 1 => {
                // Greeting, then a dead connection: the client's next read
                // fails, and the pool must re-dial for the following mail.
                let _ = stream.write_all(b"220 mock ESMTP ready\r\n");
                std::thread::sleep(Duration::from_millis(100));
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
            Behavior::DropFirstThenAccept => serve_smtp(stream),
            Behavior::Silent => {
                // Hold the socket open and never respond: the client's
                // read must hit the configured per-e-mail timeout.
                std::thread::sleep(Duration::from_secs(30));
                drop(stream);
            }
        });
    });

    (port, conns)
}

/// Plain-250 SMTP server: EHLO → 250, DATA → 354, QUIT → 221,
/// everything else → 250.
fn serve_smtp(mut stream: std::net::TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.write_all(b"220 mock ESMTP ready\r\n");

    let mut buf = [0u8; 8192];
    let mut data_mode = false;

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();

        if data_mode {
            if chunk.contains("\r\n.\r\n") {
                data_mode = false;
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
                let _ = stream.write_all(b"250 OK\r\n");
            }
        }
    }
}

/// Plan gap G2-2: `run()` must register a working subscriber and write a
/// log file with the run's events (before Phase 2 every `log!`/`eprintln!`
/// was captured by shell redirection, and `log!` calls were no-ops).
#[test]
fn run_registers_logging_and_writes_the_log_file() {
    let _lock = RUN_LOCK.lock().unwrap();
    cleanup_outbox();
    let (port, _conns) = start_mock_smtp(Behavior::Accept);

    write_entry(
        "log_probe.json",
        "log_probe",
        "LogProbe",
        "2026-03-01T00:00:00+00:00",
        serde_json::json!({ "name": "World" }),
    );

    osa_mailer::run(config(port, Duration::from_secs(60)))
        .expect("the e-mail must be delivered to the mock relay");

    assert!(
        tracing::dispatcher::has_been_set(),
        "run() must register the process-wide logging subscriber"
    );
    assert!(
        base().join("logs").join("osa_mailer.log").exists(),
        "run() must create the log file at <base>/logs/osa_mailer.log"
    );
    let log = log_content();
    assert!(
        log.contains("LogProbe"),
        "the delivery of the e-mail must be logged (subject visible):\n{log}"
    );
}

/// Plan gap G5-3: e-mail context values and the raw content of broken
/// entries (which can carry BCC and other PII) must never reach the log at
/// the default level, while the delivery outcome still must.
#[test]
fn e_mail_context_and_broken_entries_are_not_logged() {
    let _lock = RUN_LOCK.lock().unwrap();
    cleanup_outbox();
    let (port, _conns) = start_mock_smtp(Behavior::Accept);

    // A working entry whose context carries a marker that must never hit
    // the log (it is rendered into the e-mail body and sent, not logged).
    write_entry(
        "redact_ok.json",
        "redact_ok",
        "RedactionProbe",
        "2026-03-02T00:00:00+00:00",
        serde_json::json!({ "name": "OSA_CTX_MARKER_9f31" }),
    );

    // A broken entry whose raw content carries a second marker. Before
    // Phase 2 the parse error dumped the whole entry content to the log;
    // now it is `debug`-level only.
    std::fs::write(
        base().join("outbox").join("redact_broken.json"),
        r#"{"id": "redact_broken", "bcc_secret": "OSA_BCC_MARKER_4277", oops"#,
    )
    .unwrap();

    osa_mailer::run(config(port, Duration::from_secs(60)))
        .expect("the healthy e-mail must be delivered");

    let log = log_content();
    assert!(
        log.contains("RedactionProbe"),
        "the delivery outcome must still be logged:\n{log}"
    );
    assert!(
        !log.contains("OSA_CTX_MARKER_9f31"),
        "e-mail context values must not be logged (G5-3):\n{log}"
    );
    assert!(
        !log.contains("OSA_BCC_MARKER_4277"),
        "raw entry content of a broken entry must not be logged (G5-3):\n{log}"
    );
}

/// Plan gap G2-6: when the relay drops the connection mid-session, the
/// failed e-mail keeps its entry in the outbox (for retry) and the next
/// e-mail is sent over a fresh TCP connection — the relay must see at
/// least two distinct connections.
#[test]
fn a_failed_send_reconnects_for_the_next_mail() {
    let _lock = RUN_LOCK.lock().unwrap();
    cleanup_outbox();
    let (port, conns) = start_mock_smtp(Behavior::DropFirstThenAccept);

    write_entry(
        "reconnect_a.json",
        "reconnect_a",
        "Reconnect A",
        "2026-03-03T00:00:00+00:00",
        serde_json::json!({ "name": "A" }),
    );
    write_entry(
        "reconnect_b.json",
        "reconnect_b",
        "Reconnect B",
        "2026-03-04T00:00:00+00:00",
        serde_json::json!({ "name": "B" }),
    );

    let result = osa_mailer::run(config(port, Duration::from_secs(60)));
    let err = result.expect_err("the first e-mail's connection is dropped by the relay");
    assert!(
        err.to_string().contains("1 e-mail(s) failed"),
        "exactly one e-mail must fail, got: {err}"
    );

    // The relay must have seen a fresh TCP connection for the second
    // e-mail (distinct ephemeral ports = distinct connections).
    let distinct = {
        let mut ports = conns.lock().unwrap().clone();
        ports.sort_unstable();
        ports.dedup();
        ports
    };
    assert!(
        distinct.len() >= 2,
        "the second e-mail must be sent over a new TCP connection, saw {} connection(s)",
        distinct.len()
    );

    // Exactly one e-mail was delivered (archived), the other was kept in
    // the outbox with one recorded attempt.
    let archive = base().join("outbox").join("archive");
    let archived: Vec<String> = std::fs::read_dir(&archive)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
        .unwrap_or_default();
    let outbox = base().join("outbox");
    let kept: Vec<String> = std::fs::read_dir(&outbox)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect())
        .unwrap_or_default();

    assert_eq!(
        archived.iter().filter(|n| n.starts_with("reconnect_")).count(),
        1,
        "exactly one of the two e-mails must be archived"
    );
    let failed = kept
        .iter()
        .find(|n| n.starts_with("reconnect_") && n.ends_with(".json"))
        .expect("the failed entry must be kept in the outbox");
    let state = std::fs::read_to_string(outbox.join(format!("{failed}.state")))
        .expect("the failed entry must have an attempt sidecar");
    let parsed: serde_json::Value = serde_json::from_str(&state).unwrap();
    assert_eq!(parsed["attempts"], 1, "the failure must record one attempt");
}

/// Plan gap G2-6: the configured per-e-mail timeout must bound the SMTP
/// wait. A relay that accepts the TCP connection but never answers must
/// fail the send after `smtp_timeout` (500 ms here) — with the library's
/// default of 60 s the bound below would be missed.
#[test]
fn the_per_mail_timeout_bounds_the_smtp_wait() {
    let _lock = RUN_LOCK.lock().unwrap();
    cleanup_outbox();
    let (port, _conns) = start_mock_smtp(Behavior::Silent);

    write_entry(
        "timeout_probe.json",
        "timeout_probe",
        "TimeoutProbe",
        "2026-03-05T00:00:00+00:00",
        serde_json::json!({ "name": "World" }),
    );

    let started = Instant::now();
    let result = osa_mailer::run(config(port, Duration::from_millis(500)));
    let elapsed = started.elapsed();

    let err = result.expect_err("the silent relay must cause a send failure");
    assert!(
        elapsed < Duration::from_secs(10),
        "the per-e-mail timeout (500 ms) must bound the SMTP wait, took {elapsed:?} \
         (the 60 s library default would mean the configured timeout was not applied)"
    );
    let _ = err;
}
