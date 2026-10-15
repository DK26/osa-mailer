use serde::{Deserialize, Serialize};
use std::fs;
use std::rc::Rc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::{DateTime, FixedOffset};
use walkdir::{DirEntry, WalkDir};

use crc::{Algorithm, Crc, CRC_32_ISO_HDLC};

// CRC_32_ISO_HDLC is compatible with Python 3
const CRC32_ALGORITHM: Algorithm<u32> = CRC_32_ISO_HDLC;

/// Returns a checksum calculated with CRC32 using the ISO HDLC algorithm for compatibility with Python.
pub fn crc32_iso_hdlc_checksum(bytes: &[u8]) -> u32 {
    let crc: Crc<u32> = Crc::<u32>::new(&CRC32_ALGORITHM);
    crc.checksum(bytes)
}

pub fn string_crc32_iso_hdlc_checksum(string: &str) -> String {
    format!("{:x}", crc32_iso_hdlc_checksum(string.as_bytes()))
}

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// from:
// +entries: [ { .. }, { .. } ]

// to:
// entries: [
//  { n: N, v: [ { .. }, { .. } ] },
//  { n: N, v: [ { .. }, { .. } ] }
// ]

// replace `+entries` with new `entries`

#[derive(Serialize, Debug)]
struct AccumulatedValue {
    order: u32,
    checksum: String,
    value: serde_json::Value,
}

/// Fields that are not present in an e-mail entry default to empty values,
/// so the documented optional fields (`cc`, `bcc`, `reply_to`,
/// `attachments`, `unique_by`, `alternative_content`, ...) are truly
/// optional (plan gap G1-2).
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(default)]
pub(crate) struct Email {
    pub(crate) system: String,
    pub(crate) subsystem: String,
    pub(crate) from: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) bcc: Vec<String>,
    pub(crate) reply_to: Vec<String>,
    pub(crate) subject: String,
    pub(crate) template: String,
    pub(crate) alternative_content: String,
    pub(crate) attachments: Vec<String>,
    pub(crate) unique_by: String,
}

/// A Composed E-mail is one that has all of its context gathered and ordered.
#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct ComposedEmail {
    pub(crate) id: u32,
    pub(crate) header: Email,
    pub(crate) context: serde_json::Map<String, serde_json::Value>,
    /// The ids (file paths) of the entries this e-mail was composed from.
    /// In single mode this is exactly one entry; in batch mode it is the
    /// whole group. Outbox cleanup must only remove these entries, never
    /// the whole group (plan gap G1-3).
    #[serde(default)]
    pub(crate) entry_ids: Vec<String>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum EntryComposeError {
    #[error(
        "Entry `{entry}`: `+{key}` conflicts with an existing non-array value of `{key}` (duplication is not allowed)"
    )]
    Duplication {
        entry: String,
        key: String,
    },

    #[error(
        "Entry `{entry}`: plain key `{key}` conflicts with an accumulated `+{key}` array (duplication is not allowed)"
    )]
    AccumulatedConflict {
        entry: String,
        key: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Entry {
    id: String,
    utc: DateTime<FixedOffset>,
    notify_error: Vec<String>,
    email: Email,
    context: serde_json::Map<String, serde_json::Value>,
}

/// Contains metadata about the parsed entry and the deserialized entry itself
// I couldn't find a proper name for an object that adds metadata about the entry but also contains the entry (like an extension for it).
pub(crate) struct EmailRequest {
    pub(crate) id: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) entry: Entry,
}

impl EmailRequest {
    /// Calculate the E-Mail ID for the current entry.
    pub fn email_id(&self) -> u32 {
        let email_string = serde_json::to_string(&self.entry.email)
            .expect("Deserialized from JSON but cannot be serialized into JSON?");
        crc32_iso_hdlc_checksum(email_string.as_bytes())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UnparsedEntry {
    id: String,
    content: String,
    path: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct EntryParseError {
    pub(crate) entry_content: UnparsedEntry,
    pub(crate) error: serde_json::Error,
}

fn parse_entities(
    unparsed_entries: &Vec<UnparsedEntry>,
    parsed_entries: &mut Vec<Rc<EmailRequest>>,
    parse_errors: &mut Vec<EntryParseError>,
) {
    for unparsed_entry in unparsed_entries {
        match serde_json::from_str::<Entry>(&unparsed_entry.content) {
            Ok(parsed_entry) => parsed_entries.push(Rc::new(EmailRequest {
                id: unparsed_entry.id.clone(),
                path: unparsed_entry.path.clone(),
                entry: parsed_entry,
            })),
            Err(e) => parse_errors.push(EntryParseError {
                entry_content: unparsed_entry.clone(),
                error: e,
            }),
        }
    }
}

fn is_entry(entry: &DirEntry, extension: &str) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.to_lowercase().ends_with(extension))
        .unwrap_or(false)
}

/// The results of parsing the entry files
pub(crate) struct EntryParseResults {
    pub(crate) ok: Vec<Rc<EmailRequest>>,
    pub(crate) err: Vec<EntryParseError>,
}

pub(crate) fn load_entries<P: AsRef<Path>>(dir: P, extension: &str) -> EntryParseResults {
    let mut unparsed_entries = Vec::new();

    // Top level only: the outbox's state subdirectories (processing/,
    // archive/, failed/) and `.state` sidecars must never be re-parsed as
    // new e-mail entries.
    for entry in WalkDir::new(dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| is_entry(e, extension))
    {
        let entry_content = fs::read_to_string(entry.path());

        match entry_content {
            Ok(v) => {
                unparsed_entries.push(UnparsedEntry {
                    id: entry.path().display().to_string(),
                    content: v,
                    path: Some(entry.path().to_owned()),
                });
            }
            Err(_) => continue,
        }
    }

    let mut result = Vec::new();
    let mut errors = Vec::new();

    parse_entities(&unparsed_entries, &mut result, &mut errors);

    EntryParseResults {
        ok: result,
        err: errors,
    }
}

enum EmailComposeMethod {
    /// Treat each entry as a single E-mail
    Single,

    /// Accumulate a batch of E-mails
    Batch,
}

type MergedEmailRequests = HashMap<u32, Vec<Rc<EmailRequest>>>;

/// Arrange all email requests by E-Mail ID in an ordered manure.
pub(crate) fn merge_email_requests(
    email_requests_pool: &Vec<Rc<EmailRequest>>,
) -> MergedEmailRequests {
    let mut merged_email_requests: MergedEmailRequests = HashMap::new();

    // Merge email requests context into a single E-mail
    for email_request in email_requests_pool {
        // This ID is used to group email requests
        let email_id = email_request.email_id();

        // Retrieve entries vector for E-Mail ID (or create one if doesn't exists)
        let email_requests_group = merged_email_requests
            .entry(email_id)
            .or_insert_with(Vec::new);

        // Append new email request to the E-Mail ID
        email_requests_group.push(Rc::clone(&email_request));
    }

    // Order entries by their UTC time
    for (_, value) in merged_email_requests.iter_mut() {
        value.sort_by(|a, b| a.entry.utc.cmp(&b.entry.utc))
    }

    merged_email_requests
}

type JsonObject = serde_json::Map<String, serde_json::Value>;

fn copy_and_accumulate(
    source: &JsonObject,
    target: &mut JsonObject,
    email_compose_method: &mut EmailComposeMethod,
    entry_id: &str,
) -> Result<(), EntryComposeError> {
    // Scan all key/value elements in the source JSON object
    for (k, v) in source {
        // Detected an accumulation sign in key name

        if let Some(key_name) = k.strip_prefix('+') {
            // When we detect the `+` key symbol, we automatically treat all entries as a batch meant for a single E-mail
            *email_compose_method = EmailComposeMethod::Batch;

            // Remove the prefixed version key from the target JSON object
            target.remove(k);

            // A plain (non-accumulated) value of the same name conflicts with
            // the `+key` accumulation: report it instead of silently dropping
            // data (plan gap G1-6).
            let value_vec = match target
                .entry(key_name)
                .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            {
                serde_json::Value::Array(value_vec) => value_vec,
                _ => {
                    return Err(EntryComposeError::Duplication {
                        entry: entry_id.to_string(),
                        key: key_name.to_string(),
                    })
                }
            };

            value_vec.push(serde_json::json!(AccumulatedValue {
                order: (value_vec.len() + 1) as u32,
                checksum: string_crc32_iso_hdlc_checksum(&v.to_string()),
                value: v.clone(),
            }));
        } else if let serde_json::Value::Object(json_obj_borrowed) = v {
            if let Some(serde_json::Value::Array(_)) = target.get(k) {
                // A plain object cannot sit on top of an accumulated array
                return Err(EntryComposeError::AccumulatedConflict {
                    entry: entry_id.to_string(),
                    key: k.to_string(),
                });
            }

            if !target.contains_key(k) {
                target.insert(
                    k.to_string(),
                    serde_json::Value::Object(json_obj_borrowed.to_owned()),
                );
            }

            if let serde_json::Value::Object(nested_target) =
                target.get_mut(k).expect("key was just inserted")
            {
                copy_and_accumulate(json_obj_borrowed, nested_target, email_compose_method, entry_id)?;
            }
        } else {
            match target.entry(k) {
                serde_json::map::Entry::Occupied(_) => {
                    // First value wins; a scalar over an accumulated array is
                    // a conflict, a scalar over a scalar is harmless.
                    if let Some(serde_json::Value::Array(_)) = target.get(k) {
                        return Err(EntryComposeError::AccumulatedConflict {
                            entry: entry_id.to_string(),
                            key: k.to_string(),
                        });
                    }
                }
                serde_json::map::Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(v.clone());
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn compose_emails(
    email_entries: &MergedEmailRequests,
) -> Result<Vec<ComposedEmail>, EntryComposeError> {
    let mut composed_emails = Vec::new();

    for (id, entries_metadata) in email_entries {
        let first_entry = entries_metadata
            .get(0)
            .expect("The vector was created empty when it was inserted to the map.");

        let email = first_entry.entry.email.clone();

        // Create accumulated context object
        let mut accumulated_context = first_entry.entry.context.clone();

        // By default, we assume the `single` mode for Email composition. e.g. Every single entry is an E-mail to send.
        // This can be changed if the `+` key symbol is detected during context scan of the first E-mail entry, which then changes the mode to `batch`.
        let mut email_compose_method = EmailComposeMethod::Single;

        for entry_metadata in entries_metadata {
            let entry_context = &entry_metadata.entry.context;
            copy_and_accumulate(
                entry_context,
                &mut accumulated_context,
                &mut email_compose_method,
                &entry_metadata.id,
            )?;

            if let EmailComposeMethod::Single = email_compose_method {
                // Create a single E-mail for a single entry
                composed_emails.push(ComposedEmail {
                    id: *id,
                    header: entry_metadata.entry.email.clone(),
                    context: entry_metadata.entry.context.clone(),
                    entry_ids: vec![entry_metadata.id.clone()],
                });
            };
        }

        if let EmailComposeMethod::Batch = email_compose_method {
            // Create a single E-mail from the entries batch with their accumulated context
            composed_emails.push(ComposedEmail {
                id: *id,
                header: email,
                context: accumulated_context,
                entry_ids: entries_metadata.iter().map(|e| e.id.clone()).collect(),
            });
        }
    }
    Ok(composed_emails)
}

/// The outbox file paths of the entries that were consumed by the given
/// composed e-mail (exactly one entry in single mode, the whole group in
/// batch mode) — used for the post-send cleanup (plan gap G1-3).
pub(crate) fn entry_paths_for(
    composed: &ComposedEmail,
    email_entries: &MergedEmailRequests,
) -> Vec<PathBuf> {
    email_entries
        .get(&composed.id)
        .map(|group| {
            group
                .iter()
                .filter(|request| composed.entry_ids.contains(&request.id))
                .filter_map(|request| request.path.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_email() -> serde_json::Value {
        serde_json::json!({
            "system": "TestSystem",
            "subsystem": "TestSubsystem",
            "from": "From <from@example.com>",
            "to": ["To <to@example.com>"],
            "subject": "Test subject",
            "template": "test_template"
        })
    }

    fn entry_json(utc: &str, email: &serde_json::Value, context: serde_json::Value) -> String {
        serde_json::json!({
            "id": "entry-id",
            "utc": utc,
            "notify_error": ["Ops <ops@example.com>"],
            "email": email,
            "context": context
        })
        .to_string()
    }

    fn request(json: &str, id: &str, path: Option<PathBuf>) -> Rc<EmailRequest> {
        let entry: Entry = serde_json::from_str(json).expect("entry must parse");
        Rc::new(EmailRequest {
            id: id.to_string(),
            path,
            entry,
        })
    }

    fn compose(pool: &Vec<Rc<EmailRequest>>) -> Result<Vec<ComposedEmail>, EntryComposeError> {
        compose_emails(&merge_email_requests(pool))
    }

    #[test]
    fn crc32_matches_python_zlib_value() {
        // CRC-32/ISO-HDLC standard check value for "123456789" (zlib.crc32
        // in Python) — this is the contract the producer side relies on.
        assert_eq!(crc32_iso_hdlc_checksum(b"123456789"), 0xCBF4_3926);
        // The hex string form is lowercase and matches the byte checksum
        assert_eq!(
            string_crc32_iso_hdlc_checksum("abc"),
            format!("{:x}", crc32_iso_hdlc_checksum(b"abc"))
        );
    }

    #[test]
    fn email_optional_fields_default() {
        // Only the fields the demo client always writes; everything else must
        // default instead of failing to parse (plan gap G1-2).
        let json = entry_json(
            "2026-01-01T00:00:00+00:00",
            &serde_json::json!({
                "system": "s",
                "subsystem": "ss",
                "from": "a@b.c",
                "to": ["x@y.z"],
                "subject": "hi",
                "template": "t"
            }),
            serde_json::json!({}),
        );
        let entry: Entry = serde_json::from_str(&json).expect("must parse");
        assert!(entry.email.cc.is_empty());
        assert!(entry.email.bcc.is_empty());
        assert!(entry.email.reply_to.is_empty());
        assert!(entry.email.attachments.is_empty());
        assert_eq!(entry.email.alternative_content, "");
        assert_eq!(entry.email.unique_by, "");
    }

    #[test]
    fn identical_headers_share_email_id() {
        let email = base_email();
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({"k": 1})), "a", None);
        let b = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({"k": 2})), "b", None);
        assert_eq!(a.email_id(), b.email_id());
    }

    #[test]
    fn different_headers_have_different_ids() {
        let email_b = base_email();
        let mut email_a = base_email();
        email_a["subject"] = serde_json::json!("Other subject");
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email_a, serde_json::json!({})), "a", None);
        let b = request(&entry_json("2026-01-01T00:00:00+00:00", &email_b, serde_json::json!({})), "b", None);
        assert_ne!(a.email_id(), b.email_id());
    }

    #[test]
    fn merge_groups_and_orders_by_utc() {
        let email = base_email();
        let later = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({})), "later", None);
        let earlier = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({})), "earlier", None);
        let pool = vec![later, earlier];
        let merged = merge_email_requests(&pool);
        let group = merged.values().next().expect("one group");
        let ids: Vec<&str> = group.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["earlier", "later"]);
    }

    #[test]
    fn single_mode_composes_one_email_per_entry_with_own_entry_id() {
        let email = base_email();
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({"v": 1})), "a", Some(PathBuf::from("a.json")));
        let b = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({"v": 2})), "b", Some(PathBuf::from("b.json")));
        let pool = vec![a, b];

        let composed = compose(&pool).expect("compose ok");
        assert_eq!(composed.len(), 2);
        for c in &composed {
            assert_eq!(c.entry_ids.len(), 1);
        }

        let merged = merge_email_requests(&pool);
        // Each composed e-mail owns exactly its own entry file (plan gap G1-3)
        let mut all: Vec<PathBuf> = composed
            .iter()
            .flat_map(|c| entry_paths_for(c, &merged))
            .collect();
        all.sort();
        assert_eq!(all, vec![PathBuf::from("a.json"), PathBuf::from("b.json")]);
    }

    #[test]
    fn batch_mode_accumulates_values_with_order_and_checksum() {
        let email = base_email();
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({"+items": ["x"]})), "a", None);
        let b = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({"+items": ["y"]})), "b", None);
        let pool = vec![a, b];

        let composed = compose(&pool).expect("compose ok");
        assert_eq!(composed.len(), 1);
        let items = composed[0]
            .context
            .get("items")
            .and_then(|v| v.as_array())
            .expect("accumulated array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["order"], 1);
        assert_eq!(items[0]["value"], serde_json::json!(["x"]));
        assert_eq!(items[1]["order"], 2);
        assert_eq!(items[1]["value"], serde_json::json!(["y"]));
        assert!(items[0].get("checksum").and_then(|v| v.as_str()).is_some());
        assert_eq!(composed[0].entry_ids.len(), 2);
        // The `+items` key itself must not remain in the context
        assert!(!composed[0].context.contains_key("+items"));
    }

    #[test]
    fn plus_key_over_plain_key_is_a_duplication_error() {
        // Plan gap G1-6: conflicts are reported, never silently dropped.
        let email = base_email();
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({"items": "scalar"})), "a", None);
        let b = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({"+items": ["x"]})), "b", None);
        let pool = vec![a, b];

        let err = compose(&pool).expect_err("must be a duplication error");
        assert!(matches!(err, EntryComposeError::Duplication { .. }));
    }

    #[test]
    fn plain_key_over_accumulated_array_is_a_conflict() {
        let email = base_email();
        let a = request(&entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({"+items": ["x"]})), "a", None);
        let b = request(&entry_json("2026-01-02T00:00:00+00:00", &email, serde_json::json!({"items": 5})), "b", None);
        let pool = vec![a, b];

        let err = compose(&pool).expect_err("must be an accumulated conflict");
        assert!(matches!(err, EntryComposeError::AccumulatedConflict { .. }));
    }

    #[test]
    fn parse_results_separate_ok_and_err() {
        let dir = std::env::temp_dir().join(format!("osa_mailer_entries_parse_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let email = base_email();
        let good = entry_json("2026-01-01T00:00:00+00:00", &email, serde_json::json!({}));
        fs::write(dir.join("good.json"), good).unwrap();
        fs::write(dir.join("bad.json"), "{ not json").unwrap();

        let results = load_entries(&dir, ".json");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(results.ok.len(), 1);
        assert_eq!(results.err.len(), 1);
    }
}
