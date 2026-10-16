# OSA-Mailer — Upgrade Plan (Full Renewal + Missing Features)

> Status: **v1.1** (2026-10-15). Gap inventory verified by two independent passes:
> (1) direct code/doc reading by the planner, (2) the `gap-audit` review workflow
> (run `20261015-084544-gap-audit-0d77fd`, 5 critics × 30 findings, 9 findings
> independently confirmed by cross-model verification votes, ledger
> `…\ultra-runs\d56dad95975a0894\20261015-084544-gap-audit-0d77fd\ledger.json`).
>
> Verdict key used below: **C** = confirmed by verification vote; **V** = verified by
> direct code reading this session (vote budget exhausted for the finding); **U** =
> unverified, needs a test (kept, with the test named).
> The workflow's final completeness-critique step was **blocked** (the 131 KB verified
> evidence handoff exceeded the 24 KB input limit — a harness constraint, not a
> finding). The critique's role (missed-gap check) was covered by: the critics
> cross-checking adjacent dimensions (e.g. the modernization critic independently
> re-verifying the `serde(default)` and path-join defects), and the planner's own
> full-repo read. Ledger "gaps" (critic self-declared coverage limits) are folded into
> §2.6.
>
> **Progress (2026-10-15):** Phase 0 ✅ (`4f4fb6c`); Phase 2 claim/commit ✅
> (`4f4fb6c`); Phase 1 ✅ — stack renewal: `lettre 0.11.23` (source-compatible, no
> code changes needed), `thiserror 2`, `handlebars 6.4.4`, `strum 0.27.2`, `infer 0.16`,
> `tera 1.20.0`, `liquid 0.26.11` (newest stable), `lazy_static` →
> `std::sync::LazyLock`, edition 2024, `Cargo.lock` version-pin tests, mock-SMTP
> integration test (`tests/mock_smtp.rs`).
>
> **Progress (2026-10-16):** Phase 2 logging + SMTP robustness ✅ — structured
> logging: process-wide `tracing` subscriber registered by `run()` (G2-2),
> stdout + `logs/osa_mailer.log` with a locale-free name (G4-3), context
> values and broken-entry content redacted to `debug!` (G5-3); per-e-mail SMTP
> timeout via `SmtpTransportBuilder::timeout` (G2-6; `SMTP_TIMEOUT_SECS`,
> default 60 s) with reconnect-on-failure (lettre's pool dials per e-mail and
> drops broken connections); `osa_mailer.cmd` shell log redirection deprecated;
> `tests/reliability.rs` (4 integration tests). Open: Phase 1 acceptance extras
> (`cargo deny`, ≥90% branch coverage — tools not installed in this environment),
> the Phase 2 error-report e-mail (G3-3/G3-4), Phases 3–6 (Phase 6 telemetry
> added 2026-10-16 on owner request), §5 decisions.

## 1. Executive summary

OSA-Mailer is an MVP one-shot CLI: scan `outbox/*.json` → merge/accumulate requests →
render HTML template (tera / handlebars / liquid) → build a multipart SMTP message
(inline images + attachments) → send via one `lettre` 0.10 connection → delete sent
entries. Configuration is env vars only; there is no daemon, no retry, no logging,
no tests for core logic, and a large part of the documented feature set (template
manifests, custom functions, error e-mails, service mode, marketplace) is absent.
The working tree is currently **broken by an in-progress `relative_path` → `app_path`
crate rename** (`error[E0432]: unresolved import 'relative_path'` in
`src/send.rs:12`) — independently confirmed by two critics (one by `cargo build`,
exit 1).

The audit confirmed two headline bugs the README advertises as features:
1. **Inline image auto-attach does not work in the documented deployment** — MIME is
   probed at the raw CWD-relative `<img src>` instead of the template-relative resolved
   path, so images are silently dropped (F11).
2. **The plain-text `alternative_content` fallback is never sent** — a builder method
   writes it into the wrong field (F8, confirmed 2-0).

Target: a versioned, tested, cross-platform (Windows service + *nix daemon) tool with
a real config file, crash-safe outbox processing (claim/commit, retry, dead-letter),
structured logging, modern dependency stack, CI with clippy + release pipeline, and
all documented features implemented.

## 2. Gap inventory

### 2.0 Build & in-progress refactor
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G0-1 | **C (F7, blocker; dup F19)** | Working tree does not compile: `src/send.rs:12` imports `relative_path::PortablePath` (crate deleted; renamed to `app-path`, type is `AppPath` with `with_base()`, `cwd()` removed). Even after fixing the crate name, `PortablePath` → E0433, `.cwd()` → E0599 | `cargo build` → `error[E0432]` (planner), E0432/E0433/E0599 chain (critic) |
| G0-2 | **C** | Inconsistent rename: `ParsedEntry`→`EmailRequest`, `map_emails`→`merge_email_requests` done in `entries.rs`; `send.rs` not migrated | git diff |
| G0-3 | **V** | Compiler warnings: unused `#[macro_use] extern crate lazy_static` (`main.rs:1-2`), unused import `entries::Entry` (`main.rs:5`); **CI is red because of G0-1** | `cargo build` |

### 2.1 Correctness bugs
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G1-1 | **C (F8, 2-0; dup F20)** | `MessageBuilder::alternative_content()` writes into `self.content`, not `self.alternative_content` → the documented plain-text fallback is silently never attached (HTML-only mails; empty preview for text-only clients) | `src/send.rs:489-492`; sample entry has non-empty `alternative_content` |
| G1-2 | **V** (critic "gap" note, both item-1 & item-4) | `Email` struct has no `#[serde(default)]` — documented-optional fields (`cc`, `bcc`, `reply_to`, `attachments`, `unique_by`, `alternative_content`) are hard-required → documented entry variants fail to parse | `src/entries.rs:53-67` vs `docs/common_email_entry.html` |
| G1-3 | **C (F9, 2-0)** | **Group-wide deletion:** after the *first* composed e-mail of a group sends, `main.rs:158-164` deletes the outbox files of *every* entry in the group — in single mode the later e-mails of the same group become unrecoverable on failure | `src/main.rs:158-164`, `src/entries.rs:290-297` |
| G1-4 | **C (F1, 2-0)** | Duplicate-send window: deletion happens only after SMTP success and its failure is discarded (`let _ =`, FIXME-admitted); crash/locked-file between "relay accepted" and delete → identical mail re-sent next run; no archive, no Message-ID registry, no idempotency | `src/main.rs:162` |
| G1-5 | **C (F10, 2-0)** | Mixed single/batch group **double-sends**: entries before a `+key` entry are sent individually *and* re-accumulated into the final batch e-mail; the `+` entry's own non-`+` fields are dropped | `src/entries.rs:282-307` |
| G1-6 | **V** (critic "gap" note) | `+key` accumulation silently drops values when a same-named non-`+` key already holds a non-array value (FIXME-admitted); also: implemented output shape (`entries: [{order,checksum,value}]`) **contradicts the documented shape** in `demo_client.py:82` (`{idx, items:[…]}`) and the code comment at `entries.rs:30-44` (`{n, v:[…]}`) | `src/entries.rs:239-250` |
| G1-7 | **U (needs test)** | E-mail ID = CRC32 (32-bit, non-cryptographic) over the serialized `email` object → collision risk; grouping only works if all producers serialize the `email` object byte-identically (key order) | `src/entries.rs:96-100` |
| G1-8 | **C (F3, 2-0)** | **One missing template file aborts the entire run** (`?` at `main.rs:77-82`), stalling every later e-mail in the batch — inconsistent with the per-mail `continue` used elsewhere | `src/main.rs:77-82` |
| G1-9 | **V (F11)** | **Inline-image MIME is probed at the raw CWD-relative `<img src>`**, not the resolved template-relative path → `infer::get_from_path` fails in the documented deployment (images live under `templates/<name>/`) and **every image is silently dropped** from the HTML part. The substring-based `str::replace` cid rewrite (F "gap" note) adds collision risk (`logo.png` inside `blog.png`) | `src/send.rs:176-190` |
| G1-10 | **U (needs test)** | One external/absolute `<img src>` can abort the whole e-mail (`fs::read(full_file_path)?` at `send.rs:209`) instead of being skipped; narrow because the MIME probe usually fails first for URLs | `src/send.rs:178, 209` |
| G1-11 | **V** (critic "gap" note) | Tera is rebuilt from a `**/*.html` glob of the whole template dir **per e-mail**; two same-named `.html` files in nested subdirs collide (Tera key clash); perf cost per mail | `src/render.rs:474-502` |
| G1-12 | **V** (critic "gap" note) | `new_canonicalize_path_buf` touch-then-delete fallback can leave an **empty `template.html` behind** if the delete fails, silently turning a "missing template" into a render of an empty file | `src/render.rs:51-71` |
| G1-13 | **C (F2)** | Send failures silently swallowed: `eprintln!` + `continue`, **process exits 0 even when every send fails** — Windows Task Scheduler sees success; no retry, no dead-letter, no re-establish of the single SMTP connection | `src/main.rs:168-183` |
| G1-14 | **C (F28, via V)** | Full composed e-mail JSON (incl. BCC and all context) pretty-printed to stdout every run → persisted to plaintext `logs/*.log` by `osa_mailer.cmd` | `src/main.rs:41-44` |

### 2.2 Reliability & operability
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G2-1 | **C (F2)** | No retry / dead-letter / failed-entries handling; failed e-mails stay in outbox forever (see G1-4 for the duplicate side) | `src/main.rs:168-171`, README |
| G2-2 | **C (F4, 2-0; dup F15)** | No logger backend registered: `log` facade used throughout but no subscriber is ever initialized → **every `log!` call is a no-op** | `Cargo.toml:15`, grep for init = none |
| G2-3 | **V (F5, uncertain by vote)** | No configuration file; only env vars `SERVER/PORT/AUTH/USERNAME/PASSWORD` set in plaintext `osa_mailer.cmd`; `outbox`/`templates` dirs and poll interval hard-coded (TODO-admitted) | `src/main.rs:18-20, 48-56` |
| G2-4 | **V** | No CLI (no clap): TODO "Make static and use CLI ARGUMENTS instead" | `src/main.rs:48` |
| G2-5 | **C (F6, 2-0)** | Documented "pulls every X configured seconds" service/daemon mode does not exist — `ConnectionMode::{Once,Service}` is dead code; deployment relies on external Task Scheduler running the .cmd | `src/send.rs:710-714`, `README.md:42` |
| G2-6 | **V** | No rate limiting, no parallelism, no backoff; one SMTP connection for the whole run with no reconnect (F "gap": whether `lettre 0.10.4` re-dials per `send()` could not be checked — no registry access in critic env; treat as "reconnect logic absent regardless") | all of `src/` |

### 2.3 Missing documented / roadmap features
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G3-1 | **V (F14)** | Template **manifest file** + **default configurations** (guidebook ch. 7) not implemented; template = bare dir with `template.html`; no engine pinning (magic comment / extension only) | `docs/` TOC vs `src/main.rs:70-86` |
| G3-2 | **V (F14)** | **Custom template functions** (guidebook ch. 9; TODOs in code): dynamic QRCode, defang URLs/IPs/e-mail addresses | `src/render.rs:17-18`, `docs/index.html` |
| G3-3 | **V (F13)** | **E-mail report on error** (guidebook ch. 12): `Entry.notify_error` is parsed but never read; `ErrorReport`/`ErrorEvent`/`ErrorWrapper` (`errors.rs`) are dead scaffolding (`#![allow(dead_code)]`) and `app.rs` (`AppState`) is **not even declared as a module in `main.rs`** — it never compiles | `src/entries.rs:81`, `src/app.rs`, `src/errors.rs:1`, `src/main.rs:13-16` |
| G3-4 | **C (F4)** | **Structured logs** (guidebook ch. 13) — see G2-2 | |
| G3-5 | **V (F16)** | **Delayed / scheduled / conditional e-mails** (README "future ideas"; docs "delayed E-mails, etc.") | README, `docs/sending-an-email.html` |
| G3-6 | **V (F16)** | **Database-backed outbox** (docs: "In the future, we may use a database") | `docs/sending-an-email.html` |
| G3-7 | **V (F17)** | **Windows service + installer** and ***nix service package** (documented roadmap) | `docs/roadmap-community-vision.html` |
| G3-8 | **V (F18)** | **Hub / marketplace** for sharing smart templates (WIP) — no code, no index format | `docs/index.html` |
| G3-9 | **V (F18)** | Guidebook is a hollow shell: TOC promises ~10+ chapters with no page (flow, e-mail entry, the outbox, template anatomy, create-your-own-template, custom functions, rendit, hub/marketplace, troubleshooting, error report, logs, roadmap); only 10 pages exist | `docs/*.html` |
| G3-10 | **V (F17)** | Linux builds never produced; README links only alpha-3 i686-windows | README, `docs/setup.html` |
| G3-11 | **V** | Template **partials/extends** unsupported in any engine: Tera partial support commented out, Liquid partials a TODO, handlebars `render_template` is single-file | `src/render.rs:454-502, 554` |

### 2.4 Modernization & engineering
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G4-1 | **V (F22)** | Stale core deps: `lettre 0.10.4` (+ rustls 0.21; current 0.11.x), `thiserror 1`, `liquid 0.26`, `handlebars 4`, `tera 1`, `strum 0.24`, `infer 0.13`, `lazy_static` (std `OnceLock` alternative), chrono default-features-off | `Cargo.toml:8-35`, `Cargo.lock:556-557` |
| G4-2 | **V (F22)** | edition 2021 → 2024 opportunity; no `rust-toolchain.toml` pin | `Cargo.toml:4` |
| G4-3 | **V (F23)** | Dead / duplicated code: **three parallel path abstractions** (`AbsolutePath` in `render.rs` vs `app_path` crate vs `RelativePath` usage); `rendered_path`, `find_template_references`, `SmtpConnectionBuilder`/`SmtpConnectionInfo`, `ConnectionMode` unused; large commented-out blocks; placeholder test `somekind` in `app.rs`; `osa_mailer.cmd` log naming breaks on non-`MM-dd-yyyy` locales | `src/render.rs:77-179, 184-268`, `src/send.rs:324-426, 710-714`, `osa_mailer.cmd:13` |
| G4-4 | **V (F21)** | Zero tests for core logic (merge/accumulate/compose, template detection, MIME, multipart assembly, attachment paths); only `app_path` unit tests + one `errors.rs` test + placeholder | `src/**` |
| G4-5 | **V (F24)** | CI: deprecated `actions-rs/*` (archived), `actions/checkout@v2`, clippy job commented out, no OS/arch matrix, no deny job on source push; **currently red** (G0-1) | `.github/workflows/*.yml` |
| G4-6 | **V (F24)** | No release pipeline: manual alpha-3; no tag workflow, no x86_64 (not even for Windows), no checksums/signing, no `CHANGELOG` | README badges vs `.github/` |
| G4-7 | **V** | `app_path` crate: `cwd()` API removed in the rename but callers not updated (G0-1); tests use shared `env::temp_dir()` subdirs (collision risk under parallel test runs); crate unpublished (TODO) | `app_path/src/lib.rs` |

### 2.5 Security
| # | Verdict | Gap | Evidence |
|---|---------|-----|----------|
| G5-1 | **V (F27)** | Default `noauth` plain SMTP (port 25) via `builder_dangerous` — cleartext e-mails; additionally **credentials set with `AUTH=noauth` are silently ignored** (only applied in the Tls/Starttls branches) | `src/send.rs:761-790`, `osa_mailer.cmd:8` |
| G5-2 | **V (F29)** | Credentials in plaintext `osa_mailer.cmd` + env vars per the documented setup flow; no secrets-management story | `osa_mailer.cmd:9-10` |
| G5-3 | **C (F28, via V)** | Full e-mail request context (incl. BCC) printed to log on every run (G1-14) — PII/secret leakage into `logs/*.log` | `src/main.rs:41-44` |
| G5-4 | **V (F25, blocker per security critic; F26)** | **Arbitrary file read + exfiltration:** untrusted `attachments` paths are read from CWD with **no containment** (`..\..\..` and absolute paths both work) and attached to a mail addressed to any `to` the entry specifies; `template` field traverses into the template dir **and** the touch-fallback can create empty files at attacker-chosen relative paths | `src/send.rs:103-148`, `src/main.rs:70-73`, `src/render.rs:51-71` |
| G5-5 | **V** (nuanced per critic gap note) | Template-engine injection surface: `Handlebars::new()` is non-strict and liquid runs `with_stdlib`; rendered context originates from external JSON producers. Not confirmed exploitable — needs engine-level tests; mitigate by pinning engines per manifest + sandboxed stdlib subset | `src/render.rs:535, 553-558` |
| G5-6 | **V** | `deny.toml` is reasonable but CI runs `cargo-deny` only on Cargo manifest/lock changes, not on source pushes | `.github/workflows/security-audit.yml` |
| G5-7 | **V (F30, new)** | **No origin authentication on outbox intake:** any local user/process can drop a JSON file into `outbox/` and send e-mail as any `from` to any `to`; no sender allowlist, no signature of entries | `src/entries.rs:116` (load path) |

### 2.6 Audit coverage notes (from ledger "gaps")
- Letter 0.10.4 transport internals (per-call re-dial, default timeout) not verifiable
  in the critic sandbox — the no-reconnect gap stands on repo-level code alone.
- Runtime behavior (exit codes, actual duplicate sends) was inferred from code, not
  executed — Phase 1/2 acceptance tests cover this (mock SMTP server).
- `send_email_request/` Python tooling and `.github` workflows were only grep-level in
  one critic; planner read them in full (no additional gaps found beyond
  G1-6's documented-shape contradiction and G4-5).
- The live guidebook (dk26.github.io) may differ from the committed `docs/` copy; only
  the repo copy was inspected.
- The untracked `UPGRADE_PLAN.md` draft's claims were not independently re-verified by
  the critics; every finding above carries its own code/doc evidence.

## 3. Target architecture

```
osa-mailer/
├─ src/
│  ├─ main.rs            # thin: parse CLI (clap) → run
│  ├─ cli.rs             # clap: subcommands run | daemon | check (validate-only)
│  ├─ config.rs          # toml config + env override + validation (thiserror 2 errors)
│  ├─ outbox/            # watch, claim, commit, retry, dead-letter
│  │  ├─ scan.rs         # (was entries::load_entries) + atomic claim → processing/
│  │  ├─ merge.rs        # (was entries::merge/compose) — pure, fully unit-tested
│  │  └─ state.rs        # archive/failed dirs, attempt counters, backoff metadata
│  ├─ template/          # engine-agnostic rendering facade
│  │  ├─ engine.rs       # tera/handlebars/liquid behind a trait (feature-gated)
│  │  ├─ manifest.rs     # manifest: engine (pinned), defaults, custom fns, resources
│  │  └─ functions.rs    # custom functions: qrcode, defang, date, base64
│  ├─ message/           # (was send.rs)
│  │  ├─ build.rs        # MessageBuilder (fixed alternative_content), multipart assembly
│  │  ├─ media.rs        # resource scan: HTML-aware walk, exact-attr cid inlining,
│  │  │                  # resolved-path MIME probe (G1-9), containment checks (G5-4)
│  │  └─ smtp.rs         # per-e-mail reconnect, timeouts, auth (warn on ignored creds)
│  ├─ notify/            # error e-mails to notify_error (activates errors.rs/app.rs)
│  ├─ logging.rs         # tracing + file/stdout, redaction of context values (G5-3)
│  ├─ telemetry.rs       # per-run metrics (JSON report) + optional /health + /metrics
│  └─ errors.rs          # kept; single error type (thiserror 2)
├─ app_path/ (→ inlined or published; single canonical path type, G4-3)
├─ tests/                # integration: fixture outbox → in-process mock SMTP
└─ docs-src/             # mdbook source (generated docs/ committed from here)
```

Key design decisions (to confirm):
1. **Claim/commit protocol** — before sending, atomically move the entry from
   `outbox/` to `processing/`; on success → `archive/`, on failure → back to `outbox/`
   with an attempt counter in sidecar state; attempts > N → `failed/` + error e-mail.
   Removes the duplicate-send window (G1-4) *and* the group-wide deletion data loss
   (G1-3): each composed e-mail's entries are committed individually.
2. **E-mail ID** — keep CRC32 for compatibility with existing producers (Python
   `new_email_request.py` embeds it in the filename), add `unique_by`-extended
   grouping option, and log a loud warning when two distinct headers collide.
3. **Config** — `osa_mailer.toml` next to the exe; env vars override; CLI flags
   override both. Poll interval lives in config → enables daemon mode (G2-3..G2-5).
4. **Daemon mode** — `osa_mailer daemon` (plain `std::thread` + sleep loop; no async
   runtime needed while sending is blocking) + `windows-service` wrapper and
   systemd unit template (G2-5, G3-7).
5. **Engines** — feature-gated `tera` (default), `handlebars`, `liquid`; engine
   **pinned per template in the manifest** (no runtime guessing); magic comment kept
   for backward compatibility (G3-1, G5-5).

## 4. Phased delivery plan

### Phase 0 — Stabilize the tree (prerequisite for everything)
- Finish the `relative_path` → `app_path` migration in `src/send.rs`
  (`AppPath::with_base` replaces `RelativePath::cwd`) (G0-1, G0-2).
- Remove dead imports/warnings (G0-3).
- Fix the cheap correctness bugs: G1-1 (alternative_content field), G1-2
  (`#[serde(default)]` on `Email`), G1-6 (dup `+`/plain key → explicit
  `DuplicationError` instead of silent drop; **and reconcile the accumulated-output
  shape with `demo_client.py`/docs or update both** — decision: keep code shape, fix
  docs), G1-8 (per-mail template load failure → `continue` with error e-mail, not
  `?` abort), G1-14/G5-3 (drop the context dump; debug-level only), G5-4 (path
  containment: resolved attachment/image/template paths must stay under their roots;
  reject absolute paths and `..`), G1-12 (touch-fallback must not create files inside
  template dirs).
- Add `rust-toolchain.toml`; `cargo fmt` + clippy-clean as the new baseline;
  re-green the CI job that runs `cargo test` (G4-5).
- **Acceptance**: `cargo build && cargo test && cargo clippy --all-targets` green;
  sample entry from `send_email_request/samples` parses and composes identically;
  traversal fixtures (absolute path, `..`, missing file) are rejected with errors.

### Phase 1 — Renew the stack & prove it (renewal without behavior change)
- Edition 2021 → 2024; update deps: `lettre 0.11` (port `smtp.rs` to the new builder
  API), `thiserror 2`, latest `liquid`, `handlebars 6`, `strum 0.2x`, drop
  `lazy_static` for `std::sync::OnceLock`; pin everything; `cargo deny check` clean.
- Delete dead code (G4-3): `AbsolutePath` (use `app_path`), `rendered_path`,
  `SmtpConnectionBuilder`, `ConnectionMode`, commented blocks; consolidate the three
  path abstractions.
- Test the core: merge/accumulate/compose (fixture + property tests incl. G1-5
  mixed-mode and G1-7 ID-collision cases), media handling (unit tests incl.
  G1-9 resolved-path MIME probe, G1-10 external-URL skip), address splitting,
  multipart assembly. Integration test: fixture outbox + in-process mock SMTP
  server (`smtp-server` crate) asserting generated MIME (cids, attachments,
  **alternative part present** — regression test for G1-1).
- **Acceptance**: ≥ 90% branch coverage of `merge`/`compose`/`build`; all
  `tests/` green; `cargo deny check` clean; mock-SMTP assertions pass.

### Phase 2 — Reliability (crash-safe processing)
- Claim/commit protocol (design decision 1) (G1-3, G1-4, G2-1): `outbox/` →
  `processing/` → `archive/` | `failed/`; attempt counter + exponential backoff;
  orphan re-claim on start; **per-composed-e-mail commit** (never group-wide).
- SMTP reconnect per e-mail on transport failure; per-e-mail timeout (G2-6).
- **Non-zero exit code on any send failure** (G1-13) so Task Scheduler/alerting sees it.
- Structured logging: `tracing` + `tracing-subscriber` (file + stdout), level from
  config, **redaction of context values** (G2-2, G5-3); deprecate `osa_mailer.cmd`
  log redirection (also fixes G4-3 locale-fragile log naming).
- Activate the error-report e-mail: wire `errors.rs`/`app.rs` into the pipeline; on
  failure send one summary e-mail to the entry's `notify_error` (G3-3, G3-4).
- **Acceptance**: kill-the-process mid-batch test → no duplicate sends, no lost
  single-mode e-mails; failing relay → retries then dead-letter + notify e-mail +
  exit ≠ 0; log-redaction test.

### Phase 3 — Configuration, CLI & daemon
- `osa_mailer.toml` (relay, auth, credentials via env-var reference only, poll
  interval, dirs, retry policy, log config, **sender allowlist** — G5-7); CLI via
  `clap`: `run` (one-shot, current behavior), `daemon`, `check` (validate config +
  entry files without sending) (G2-3, G2-4).
- Daemon loop with configured poll interval; graceful shutdown; Windows service
  (`windows-service`) + Inno Setup installer + `osa_mailer.service` systemd unit per
  the documented roadmap (G2-5, G3-7).
- Warn explicitly when credentials are configured but `AUTH=noauth` (G5-1).
- **Acceptance**: `check` validates the shipped sample entry; service starts/stops
  cleanly under `sc.exe` and `systemctl`; no behavior change for one-shot `run`.

### Phase 4 — Template platform (manifests, functions, partials, marketplace prep)
- Template manifest (`manifest.toml` per template dir): pinned engine, default
  context, enabled custom functions, required resources, version (G3-1, G3-11, G5-5).
- Partials/extends support for all three engines (G3-11).
- Custom functions registry: `qrcode` (image embedded as cid), `defang` for
  URLs/IPs/e-mail addresses, `date`, `base64` (G3-2).
- Safe media inlining: HTML-aware attribute walk replacing the regex scan; exact
  replacement, dedup, containment, resolved-path MIME (G1-9, G1-10, G5-4); Tera
  instance caching per template dir instead of per e-mail (G1-11).
- Delayed e-mails: entries with `utc` in the future are skipped until due (G3-5).
- **Acceptance**: shipped `ops_department` template gets a manifest; QR-code e-mail
  end-to-end test via mock SMTP; delayed entry not sent before due (test); image
  e-mail end-to-end test **with the documented layout** (regression for G1-9).

### Phase 5 — Docs, CI/CD, releases, cross-platform
- CI renewal: replace `actions-rs` with `dtolnay/rust-toolchain` + `Swatinem/rust-
  cargo` / native `cargo` steps; matrix (ubuntu/windows; x86_64 + i686);
  `cargo clippy -D warnings` re-enabled; `cargo deny check` on every push;
  mock-SMTP integration job (G4-5, G5-6).
- Release pipeline: tag → build all targets → checksums + artifact upload → GitHub
  release notes from conventional commits; `CHANGELOG.md` (G4-6).
- Cross-platform: Linux (gnu/musl) builds + packaging (`.deb`/`.rpm`/tarball +
  systemd) (G3-10).
- Docs: restore the missing guidebook chapters (G3-9) from a checked-in `docs-src/`
  mdbook; update README (engine versions, config reference, daemon/service install,
  corrected feature claims — G1-9, G1-1); fix the accumulation-shape documentation
  (G1-6).
- Marketplace (G3-8): defer to its own design (index format, signatures,
  distribution) — track as a separate project.

### Phase 6 — Observability (telemetry & health)
- Per-run telemetry (owner request, 2026-10-16): every one-shot `run` and daemon
  pass records machine-readable metrics — entries scanned, e-mails composed, sent,
  failed, skipped (render/build/claim), dead-lettered, attempts, run duration,
  per-mail SMTP round-trip (debug level) — as structured `tracing` events with
  fields, plus a JSON report line appended to `logs/` (e.g. `osa_mailer.run.jsonl`).
- Telemetry is **local-only**: no external transmission; counters carry subjects
  and paths only, never context values or addresses (the G5-3 redaction rule
  applies to telemetry as well).
- Daemon pull endpoint (behind a config flag, bound to localhost only):
  `/health` (liveness + last-run outcome) and `/metrics` (Prometheus exposition
  text) so monitoring and Task Scheduler alerting can query the service.
- **Acceptance**: a one-shot `run` writes a JSON report whose fields match the
  resulting outbox state (test); `/metrics` on an ephemeral port returns the
  counters (integration test); the report contains no context markers (redaction
  test, same pattern as Phase 2 G5-3).

## 5. Decision points (need owner sign-off)
1. **Daemon vs one-shot**: ship `daemon` in Phase 3 as the primary mode, or keep
   one-shot + external scheduler as the only supported mode? (Docs promise polling.)
2. **`notify_error` e-mail**: it must work even when the entry's own relay is the
   failing one — confirm a separate `error_relay` config is wanted.
3. **E-mail ID compatibility**: keep CRC32-based IDs byte-compatible with existing
   producers, or move to canonical serialization + a stronger hash (breaking the
   grouping contract)?
4. **Accumulation output shape**: keep the code's `[{order, checksum, value}]` and
   fix the docs/scripts, or change the code to the documented `{idx, items}` shape
   (breaking for existing templates iterating `order`/`value`)?
5. **Database outbox** (G3-6): adopt SQLite as the durable outbox in this upgrade,
   or keep file-based (claim/commit dirs) and park the database?
6. **Windows i686**: continue shipping i686 for 32-bit compatibility, or move to
   x86_64-only?
7. **`app_path` crate**: inline into the main crate (recommended — it is ~60 lines),
   keep vendored, or publish on crates.io?
8. **Outbox intake hardening** (G5-7): sender allowlist in config (Phase 3) is enough
   for the threat model, or do we need entry signing/verification?
9. **Telemetry format** (Phase 6, requested by the owner 2026-10-16): local-only
   (JSON report + optional localhost `/health` + `/metrics`), no external
   transmission — confirm Prometheus text format for `/metrics`, or is plain JSON
   enough?

## 6. Risks & mitigations
- **lettre 0.10 → 0.11 API churn**: isolated in `smtp.rs`/`build.rs`; mock-SMTP
  integration tests protect behavior.
- **Grouping contract with external producers**: any change to e-mail ID or
  accumulation-shape semantics is breaking — Phase 0/1 keep the algorithm
  byte-compatible; decisions 3 & 4 gate the rest.
- **Windows-only production today**: daemon/service work needs a Windows target in
  CI (`windows-latest`); systemd work is verifiable on `ubuntu-latest`.
- **Docs drift**: the guidebook is generated from an external mdbook; pin
  `docs-src/` in-repo so the site and code evolve together.
- **Audit verification limits** (§2.6): runtime-behavior claims (duplicate sends,
  exit codes) are code-inferred; Phase 1/2 acceptance tests convert them into
  executed evidence.

## 7. Sequencing summary
```
P0 stabilize ──► P1 renew+test ──► P2 reliability ──► P3 config/CLI/daemon
                                                        │
                                                        ▼
                                     P5 docs/CI/release ◄── P4 template platform
```
P0 and P1 are sequential and blocking. P2–P4 are largely independent once P1 lands
(P2 and P4 can proceed in parallel on separate branches). P5 wraps everything.
